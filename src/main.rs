// 릴리스 빌드에서 Windows 콘솔 창 숨김
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ffmpeg;
mod media;

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::json;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::{Request, Response};
use wry::{DragDropEvent, WebViewBuilder};

use ffmpeg::{ExportOptions, ProbeInfo};

const INDEX_HTML: &str = include_str!("../ui/index.html");
/// ui/index.html 의 --bg (#16171a)
const BG_COLOR: (u8, u8, u8, u8) = (0x16, 0x17, 0x1a, 0xff);

/// 다른 스레드/IPC 에서 메인 이벤트 루프로 보내는 이벤트
enum UserEvent {
    /// 파일 열기 대화상자 (macOS 에서는 메인 스레드에서 띄워야 함)
    PickFile,
    Load(PathBuf),
    PickFolder(Option<PathBuf>),
    Export(ExportRequest),
    Cancel,
    Reveal(PathBuf),
    /// JS 쪽 `window.onHost(msg)` 로 전달할 메시지
    ToJs(serde_json::Value),
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum IpcMsg {
    Open,
    PickFolder { current: Option<PathBuf> },
    Export(ExportRequest),
    Cancel,
    Reveal { path: PathBuf },
    /// 디버깅용: JS 로그를 터미널로
    Log { msg: String },
}

/// 내보내기 요청: 인코딩 옵션 + 사이드바에서 지정한 저장 폴더/파일 이름
#[derive(Deserialize)]
struct ExportRequest {
    dir: PathBuf,
    name: String,
    /// true 면 dir 아래 Output 폴더에 저장
    #[serde(default)]
    subfolder: bool,
    #[serde(flatten)]
    opts: ExportOptions,
}

/// 저장 폴더 아래 Output 폴더를 쓴다. 없으면 만든다.
fn output_subdir(dir: &std::path::Path) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(format!("저장 폴더가 없습니다: {}", dir.display()));
    }
    let sub = dir.join("Output");
    std::fs::create_dir_all(&sub).map_err(|e| format!("Output 폴더를 만들 수 없습니다: {e}"))?;
    Ok(sub)
}

/// 폴더 + 이름으로 출력 경로를 만든다. 확장자는 형식(ext)을 따른다
fn output_path(dir: &std::path::Path, name: &str, ext: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    let suffix = format!(".{ext}");
    let stem = match name.len().checked_sub(suffix.len()) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(&suffix) => &name[..i],
        _ => name,
    };
    if stem.is_empty() {
        return Err("파일 이름을 입력하세요".into());
    }
    if stem.contains(['/', '\\', ':']) {
        return Err("파일 이름에 / \\ : 는 쓸 수 없습니다".into());
    }
    if !dir.is_dir() {
        return Err(format!("저장 폴더가 없습니다: {}", dir.display()));
    }
    Ok(dir.join(format!("{stem}.{ext}")))
}

#[derive(Default)]
struct Current {
    path: Option<PathBuf>,
    info: Option<ProbeInfo>,
    /// 열기 요청 번호. 분석 중 다른 파일을 열면 앞선 결과는 버린다.
    load_seq: u64,
}

/// custom protocol 의 기준 URL (Windows/Android 는 http://<scheme>.localhost 형식)
fn scheme_url(scheme: &str) -> String {
    if cfg!(any(windows, target_os = "android")) {
        format!("http://{scheme}.localhost/")
    } else {
        format!("{scheme}://localhost/")
    }
}

fn main() -> wry::Result<()> {
    let ffmpeg_bin = ffmpeg::find_tool("ffmpeg");
    let ffprobe_bin = ffmpeg::find_tool("ffprobe");

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Reframe")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
        // 페이지가 그려지기 전 흰 화면이 번쩍이지 않게 UI 배경색(--bg)과 맞춘다
        .with_background_color(BG_COLOR)
        .build(&event_loop)
        .expect("window");

    let current: Arc<Mutex<Current>> = Arc::default();
    let job: ffmpeg::Job = Arc::default();

    let media_state = current.clone();
    let media_ffmpeg = ffmpeg_bin.clone();
    let ipc_proxy = proxy.clone();
    let drop_proxy = proxy.clone();

    let builder = WebViewBuilder::new()
        .with_custom_protocol("app".into(), |_, _req| {
            Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Cow::Borrowed(INDEX_HTML.as_bytes()))
                .unwrap()
        })
        // 비디오 파일 스트리밍 + 프레임 추출은 무거우므로 별도 스레드에서 응답
        .with_asynchronous_custom_protocol("media".into(), move |_, req: Request<Vec<u8>>, responder| {
            let state = media_state.clone();
            let ffmpeg_bin = media_ffmpeg.clone();
            std::thread::spawn(move || {
                let path = state.lock().unwrap().path.clone();
                responder.respond(media::handle(&req, path.as_deref(), ffmpeg_bin.as_deref()));
            });
        })
        .with_ipc_handler(move |req: Request<String>| {
            let ev = match serde_json::from_str::<IpcMsg>(req.body()) {
                Ok(IpcMsg::Open) => UserEvent::PickFile,
                Ok(IpcMsg::PickFolder { current }) => UserEvent::PickFolder(current),
                Ok(IpcMsg::Export(o)) => UserEvent::Export(o),
                Ok(IpcMsg::Cancel) => UserEvent::Cancel,
                Ok(IpcMsg::Reveal { path }) => UserEvent::Reveal(path),
                Ok(IpcMsg::Log { msg }) => return eprintln!("[ui] {msg}"),
                Err(e) => {
                    eprintln!("bad ipc: {e}: {}", req.body());
                    return;
                }
            };
            let _ = ipc_proxy.send_event(ev);
        })
        .with_drag_drop_handler(move |ev| {
            if let DragDropEvent::Drop { paths, .. } = ev {
                if let Some(p) = paths.into_iter().next() {
                    let _ = drop_proxy.send_event(UserEvent::Load(p));
                }
            }
            true // WebView 기본 동작(파일을 페이지로 여는 것) 막기
        })
        .with_background_color(BG_COLOR)
        .with_devtools(cfg!(debug_assertions))
        .with_url(scheme_url("app"));

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let webview = builder.build(&window)?;
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().unwrap())?
    };

    match &ffmpeg_bin {
        Some(ffmpeg) if ffprobe_bin.is_some() => {
            let bundled = ffmpeg::is_bundled(ffmpeg);
            eprintln!("ffmpeg: {} ({})", ffmpeg.display(), if bundled { "bundled" } else { "system" });
            // 인코더 목록 조회(ffmpeg 실행)는 첫 실행 때 백신 검사 등으로 오래 걸릴 수 있어 창을 막지 않게 따로 돌린다
            let ffmpeg = ffmpeg.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                send(&proxy, json!({
                    "type": "tools",
                    "bundled": bundled,
                    "encoders": ffmpeg::available_encoders(&ffmpeg),
                }));
            });
        }
        _ => {
            let _ = proxy.send_event(UserEvent::ToJs(json!({
                "type": "error",
                "message": "ffmpeg/ffprobe 를 찾을 수 없습니다. 동봉 버전을 받거나 시스템에 설치하세요 (macOS: brew install ffmpeg)"
            })));
        }
    }
    // 커맨드라인 인자로 파일을 넘긴 경우 바로 열기
    if let Some(arg) = std::env::args_os().nth(1) {
        let _ = proxy.send_event(UserEvent::Load(arg.into()));
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                ffmpeg::cancel(&job);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(ev) => handle_event(ev, &proxy, &webview, &current, &job, &ffmpeg_bin, &ffprobe_bin),
            _ => {}
        }
    });
}

fn send(proxy: &EventLoopProxy<UserEvent>, v: serde_json::Value) {
    let _ = proxy.send_event(UserEvent::ToJs(v));
}

fn handle_event(
    ev: UserEvent,
    proxy: &EventLoopProxy<UserEvent>,
    webview: &wry::WebView,
    current: &Arc<Mutex<Current>>,
    job: &ffmpeg::Job,
    ffmpeg_bin: &Option<PathBuf>,
    ffprobe_bin: &Option<PathBuf>,
) {
    match ev {
        UserEvent::ToJs(v) => {
            let _ = webview.evaluate_script(&format!("window.onHost({v})"));
        }
        UserEvent::PickFile => {
            let picked = rfd::FileDialog::new()
                .set_title("크롭할 영상 선택")
                .add_filter("Video", &["mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv", "flv", "mts", "m2ts", "ts", "3gp"])
                .add_filter("All files", &["*"])
                .pick_file();
            if let Some(p) = picked {
                let _ = proxy.send_event(UserEvent::Load(p));
            }
        }
        UserEvent::Load(path) => {
            let Some(ffprobe) = ffprobe_bin.clone() else {
                return send(proxy, json!({"type": "error", "message": "ffprobe 가 없습니다"}));
            };
            // 분석(ffprobe)은 별도 스레드에서: 이벤트 루프를 막으면 창 전체가 멈춘다
            send(proxy, json!({"type": "loading", "name": path.file_name().map(|n| n.to_string_lossy())}));
            let seq = {
                let mut cur = current.lock().unwrap();
                cur.load_seq += 1;
                cur.load_seq
            };
            let proxy = proxy.clone();
            let current = current.clone();
            std::thread::spawn(move || {
                let res = ffmpeg::probe(&ffprobe, &path);
                let mut cur = current.lock().unwrap();
                if cur.load_seq != seq {
                    return;
                }
                match res {
                    Ok(info) => {
                        cur.path = Some(path.clone());
                        cur.info = Some(info.clone());
                        // 같은 URL 이면 WebView 가 캐시를 쓰므로 파일마다 다른 쿼리를 붙인다
                        let nonce = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis())
                            .unwrap_or(0);
                        send(&proxy, json!({
                            "type": "loaded",
                            "path": path,
                            "name": path.file_name().map(|n| n.to_string_lossy()),
                            "stem": path.file_stem().map(|n| n.to_string_lossy()),
                            "dir": path.parent(),
                            "videoUrl": format!("{}video?n={nonce}", scheme_url("media")),
                            "frameUrl": format!("{}frame?n={nonce}", scheme_url("media")),
                            "info": info,
                        }));
                    }
                    Err(e) => send(&proxy, json!({"type": "loadFailed", "message": format!("파일을 읽을 수 없습니다: {e}")})),
                }
            });
        }
        UserEvent::PickFolder(current) => {
            let mut dialog = rfd::FileDialog::new().set_title("저장할 폴더 선택");
            if let Some(dir) = current.filter(|d| d.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            if let Some(dir) = dialog.pick_folder() {
                send(proxy, json!({"type": "folder", "path": dir}));
            }
        }
        UserEvent::Export(ExportRequest { dir, name, subfolder, opts }) => {
            let (Some(ffmpeg), (Some(input), Some(info))) = (ffmpeg_bin.clone(), {
                let c = current.lock().unwrap();
                (c.path.clone(), c.info.clone())
            }) else {
                return send(proxy, json!({"type": "error", "message": "먼저 영상을 열어주세요"}));
            };
            if job.lock().unwrap().is_some() {
                return;
            }
            let output = match (if subfolder { output_subdir(&dir) } else { Ok(dir) }).and_then(|d| output_path(&d, &name, opts.format.ext())) {
                Ok(p) => p,
                Err(e) => return send(proxy, json!({"type": "error", "message": e})),
            };
            if output == input {
                return send(proxy, json!({"type": "error", "message": "원본 파일에 덮어쓸 수 없습니다"}));
            }
            if output.exists() {
                let overwrite = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("파일이 이미 있습니다")
                    .set_description(format!("{}\n\n덮어쓸까요?", output.display()))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();
                if overwrite != rfd::MessageDialogResult::Yes {
                    return;
                }
            }

            let args = ffmpeg::build_args(&input, &output, &opts, &info);
            send(proxy, json!({"type": "exportStart", "command": format!("ffmpeg {}", shell_join(&args))}));

            let proxy = proxy.clone();
            let job = job.clone();
            std::thread::spawn(move || {
                let mut last = -1.0;
                let res = ffmpeg::run_export(&ffmpeg, &args, info.duration, job, |p| {
                    // 너무 잦은 evaluate_script 를 피하려고 0.5% 단위로만 보낸다
                    if p - last >= 0.005 {
                        last = p;
                        send(&proxy, json!({"type": "progress", "value": p}));
                    }
                });
                match res {
                    Ok(()) => send(&proxy, json!({"type": "exportDone", "path": output})),
                    Err(e) => send(&proxy, json!({"type": "exportError", "message": e})),
                }
            });
        }
        UserEvent::Cancel => ffmpeg::cancel(job),
        UserEvent::Reveal(path) => reveal(&path),
    }
}

fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}

/// 사용자에게 보여줄 명령어 문자열 (복사해서 터미널에서 재현 가능하도록)
fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=+,".contains(c)) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{output_path, output_subdir, IpcMsg};
    use crate::ffmpeg::Format;

    #[test]
    fn parse_export_msg() {
        // UI 가 실제로 보내는 메시지 그대로
        let msg = r#"{"cmd":"export","dir":"/tmp/out","name":"land_crop_608x1080","subfolder":true,"x":656,"y":0,"w":608,"h":1080,"codec":"x264","quality":75,"audio":"auto"}"#;
        let Ok(IpcMsg::Export(req)) = serde_json::from_str::<IpcMsg>(msg) else { panic!("parse failed") };
        assert_eq!(req.name, "land_crop_608x1080");
        assert!(req.subfolder);
        assert_eq!(req.opts.w, 608);
        assert_eq!(req.opts.format, Format::Mp4);
        let msg = r#"{"cmd":"export","dir":"/tmp/out","name":"a","x":0,"y":0,"w":608,"h":1080,"format":"gif","codec":"x264","quality":75,"audio":"auto","gif_fps":12,"gif_width":480}"#;
        let Ok(IpcMsg::Export(req)) = serde_json::from_str::<IpcMsg>(msg) else { panic!("parse failed") };
        assert_eq!(req.opts.format, Format::Gif);
        assert_eq!((req.opts.gif_fps, req.opts.gif_width), (12, Some(480)));
        let msg = r#"{"cmd":"pickFolder","current":null}"#;
        assert!(matches!(serde_json::from_str::<IpcMsg>(msg), Ok(IpcMsg::PickFolder { current: None })));
    }

    #[test]
    fn output_names() {
        let dir = std::env::temp_dir();
        assert_eq!(output_path(&dir, "a_crop", "mp4").unwrap(), dir.join("a_crop.mp4"));
        assert_eq!(output_path(&dir, " 영상.MP4 ", "mp4").unwrap(), dir.join("영상.mp4"));
        assert_eq!(output_path(&dir, "로그인", "mp4").unwrap(), dir.join("로그인.mp4"));
        assert_eq!(output_path(&dir, "움짤.GIF", "gif").unwrap(), dir.join("움짤.gif"));
        assert_eq!(output_path(&dir, "a.mp4", "gif").unwrap(), dir.join("a.mp4.gif"));
        assert!(output_path(&dir, ".mp4", "mp4").is_err());
        assert!(output_path(&dir, "a/b", "mp4").is_err());
        assert!(output_path(&dir.join("no-such-dir-xyz"), "a", "mp4").is_err());
    }

    #[test]
    fn output_subdir_created() {
        let dir = std::env::temp_dir().join(format!("vc-subdir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 없으면 만들고, 이미 있으면 그대로 쓴다
        assert_eq!(output_subdir(&dir).unwrap(), dir.join("Output"));
        assert!(dir.join("Output").is_dir());
        assert_eq!(output_subdir(&dir).unwrap(), dir.join("Output"));
        // 저장 폴더 자체가 없으면 만들지 않는다
        assert!(output_subdir(&dir.join("no-such-dir")).is_err());
        assert!(!dir.join("no-such-dir").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
