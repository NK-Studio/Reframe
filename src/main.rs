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
    #[serde(flatten)]
    opts: ExportOptions,
}

/// 폴더 + 이름으로 출력 경로를 만든다. 확장자는 항상 .mp4
fn output_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    let stem = match name.len().checked_sub(4) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".mp4") => &name[..i],
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
    Ok(dir.join(format!("{stem}.mp4")))
}

#[derive(Default)]
struct Current {
    path: Option<PathBuf>,
    info: Option<ProbeInfo>,
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
        .with_title("Video Crop")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
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

    // 커맨드라인 인자로 파일을 넘긴 경우 바로 열기
    if let Some(arg) = std::env::args_os().nth(1) {
        let _ = proxy.send_event(UserEvent::Load(arg.into()));
    }
    if ffmpeg_bin.is_none() || ffprobe_bin.is_none() {
        let _ = proxy.send_event(UserEvent::ToJs(json!({
            "type": "error",
            "message": "ffmpeg/ffprobe 를 찾을 수 없습니다. macOS: brew install ffmpeg"
        })));
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
            let Some(ffprobe) = ffprobe_bin else {
                return send(proxy, json!({"type": "error", "message": "ffprobe 가 없습니다"}));
            };
            match ffmpeg::probe(ffprobe, &path) {
                Ok(info) => {
                    let mut cur = current.lock().unwrap();
                    cur.path = Some(path.clone());
                    cur.info = Some(info.clone());
                    // 같은 URL 이면 WebView 가 캐시를 쓰므로 파일마다 다른 쿼리를 붙인다
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    send(proxy, json!({
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
                Err(e) => send(proxy, json!({"type": "error", "message": format!("파일을 읽을 수 없습니다: {e}")})),
            }
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
        UserEvent::Export(ExportRequest { dir, name, opts }) => {
            let (Some(ffmpeg), (Some(input), Some(info))) = (ffmpeg_bin.clone(), {
                let c = current.lock().unwrap();
                (c.path.clone(), c.info.clone())
            }) else {
                return send(proxy, json!({"type": "error", "message": "먼저 영상을 열어주세요"}));
            };
            if job.lock().unwrap().is_some() {
                return;
            }
            let output = match output_path(&dir, &name) {
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
    use super::{output_path, IpcMsg};

    #[test]
    fn parse_export_msg() {
        // UI 가 실제로 보내는 메시지 그대로
        let msg = r#"{"cmd":"export","dir":"/tmp/out","name":"land_crop_608x1080","x":656,"y":0,"w":608,"h":1080,"out_width":null,"codec":"x264","quality":75,"audio":"auto"}"#;
        let Ok(IpcMsg::Export(req)) = serde_json::from_str::<IpcMsg>(msg) else { panic!("parse failed") };
        assert_eq!(req.name, "land_crop_608x1080");
        assert_eq!(req.opts.w, 608);
        let msg = r#"{"cmd":"pickFolder","current":null}"#;
        assert!(matches!(serde_json::from_str::<IpcMsg>(msg), Ok(IpcMsg::PickFolder { current: None })));
    }

    #[test]
    fn output_names() {
        let dir = std::env::temp_dir();
        assert_eq!(output_path(&dir, "a_crop").unwrap(), dir.join("a_crop.mp4"));
        assert_eq!(output_path(&dir, " 영상.MP4 ").unwrap(), dir.join("영상.mp4"));
        assert_eq!(output_path(&dir, "로그인").unwrap(), dir.join("로그인.mp4"));
        assert!(output_path(&dir, ".mp4").is_err());
        assert!(output_path(&dir, "a/b").is_err());
        assert!(output_path(&dir.join("no-such-dir-xyz"), "a").is_err());
    }
}
