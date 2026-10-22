//! ffmpeg / ffprobe 호출 래퍼.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// ffmpeg/ffprobe 실행 명령. 릴리스 빌드는 콘솔 없는 GUI 앱이라
/// Windows 에서는 그냥 실행하면 자식 프로세스마다 콘솔 창이 잠깐 떴다 사라지므로 막는다.
fn tool_cmd(exe: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 앱과 함께 배포한 ffmpeg 를 먼저 찾고, 없으면 시스템에 설치된 것을 쓴다.
/// Finder 등에서 실행하면 셸 PATH(Homebrew 경로)가 없으므로 흔한 설치 경로도 함께 찾는다.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs = bundled_dirs();
    dirs.extend(
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    dirs.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/opt/local/bin"]
            .iter()
            .map(PathBuf::from),
    );
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// 동봉 위치: 실행 파일 옆, `ffmpeg/` 하위 폴더, macOS 앱 번들의 `Contents/Resources`
fn bundled_dirs() -> Vec<PathBuf> {
    // 심볼릭 링크(예: /usr/local/bin/reframe-cli → 앱 번들 안)로 실행해도 번들 위치를 찾도록 실제 경로로 푼다
    let exe = std::env::current_exe().map(|p| p.canonicalize().unwrap_or(p));
    let Some(exe_dir) = exe.ok().and_then(|p| p.parent().map(Path::to_path_buf)) else {
        return Vec::new();
    };
    let resources = exe_dir.join("../Resources");
    vec![exe_dir.join("ffmpeg"), exe_dir.clone(), resources.join("ffmpeg"), resources]
}

/// 동봉한 ffmpeg 인지 (UI 표시용)
pub fn is_bundled(path: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let tool_dir = path.parent().map(canon);
    bundled_dirs().iter().any(|d| Some(canon(d)) == tool_dir)
}

/// 이 ffmpeg 에서 쓸 수 있는 인코더를 UI 코덱 키("x264" 등)로 돌려준다.
pub fn available_encoders(ffmpeg: &Path) -> Vec<&'static str> {
    let out = tool_cmd(ffmpeg).args(["-hide_banner", "-encoders"]).output();
    let list = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let has = |enc: &str| list.lines().any(|l| l.split_whitespace().nth(1) == Some(enc));
    [
        ("vt264", "h264_videotoolbox"),
        ("vt265", "hevc_videotoolbox"),
        ("x264", "libx264"),
        ("x265", "libx265"),
        ("av1", "libsvtav1"),
    ]
        .into_iter()
        .filter(|(_, enc)| has(enc))
        .map(|(key, _)| key)
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    /// 회전 메타데이터를 반영한 "보이는" 크기
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub duration: f64,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    /// 파일 크기 (바이트)
    pub size: u64,
}

pub fn probe(ffprobe: &Path, input: &Path) -> Result<ProbeInfo, String> {
    let out = tool_cmd(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe 실행 실패: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or("비디오 스트림이 없습니다")?;
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");

    let coded_w = video["width"].as_u64().unwrap_or(0) as u32;
    let coded_h = video["height"].as_u64().unwrap_or(0) as u32;

    // 회전: 최신 ffmpeg 은 side_data_list 의 displaymatrix, 구버전은 tags.rotate
    let rotation = video["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|sd| sd["rotation"].as_f64()))
        .or_else(|| video["tags"]["rotate"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.0)
        .round() as i32;
    let (width, height) = if rotation.rem_euclid(180) == 90 {
        (coded_h, coded_w)
    } else {
        (coded_w, coded_h)
    };

    let duration = json["format"]["duration"]
        .as_str()
        .or(video["duration"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(|s| {
            let (n, d) = s.split_once('/')?;
            let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
            (d > 0.0).then(|| n / d)
        })
        .unwrap_or(30.0);

    Ok(ProbeInfo {
        width,
        height,
        rotation,
        duration,
        fps,
        video_codec: video["codec_name"].as_str().unwrap_or("?").to_string(),
        audio_codec: audio.and_then(|a| a["codec_name"].as_str()).map(str::to_string),
        size: std::fs::metadata(input).map(|m| m.len()).unwrap_or(0),
    })
}

/// 미리보기용 단일 프레임(JPEG). WebView 가 재생 못 하는 코덱(mkv, avi 등)일 때 사용.
pub fn frame_jpeg(ffmpeg: &Path, input: &Path, t: f64) -> Result<Vec<u8>, String> {
    let mut child = tool_cmd(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{t:.3}")])
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-vf", "scale='min(1600,iw)':-2", "-f", "image2pipe", "-c:v", "mjpeg", "-q:v", "3", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    child.wait().map_err(|e| e.to_string())?;
    if buf.is_empty() { Err("프레임 추출 실패".into()) } else { Ok(buf) }
}

/// 출력 형식
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Mp4,
    Gif,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::Gif => "gif",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportOptions {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    #[serde(default)]
    pub format: Format,
    /// MP4: "x264" | "x265" | "vt264" | "vt265" | "av1"
    pub codec: String,
    /// MP4: 0~100, 높을수록 고화질
    pub quality: u32,
    /// MP4: "auto" | "copy" | "aac" | "none"
    pub audio: String,
    /// GIF: 초당 프레임 수
    #[serde(default = "default_gif_fps")]
    pub gif_fps: u32,
    /// GIF: 가로 크기 (None 이면 크롭 크기 그대로)
    #[serde(default)]
    pub gif_width: Option<u32>,
}

fn default_gif_fps() -> u32 {
    15
}

fn even(v: u32) -> u32 {
    (v / 2 * 2).max(2)
}

pub fn build_args(input: &Path, output: &Path, o: &ExportOptions, info: &ProbeInfo) -> Vec<String> {
    // yuv420p 인코딩은 가로/세로가 짝수여야 한다
    let w = even(o.w.min(info.width));
    let h = even(o.h.min(info.height));
    let x = o.x.min(info.width - w);
    let y = o.y.min(info.height - h);
    let crop = format!("crop={w}:{h}:{x}:{y},setsar=1");

    let mut a: Vec<String> = ["-y", "-hide_banner", "-nostats", "-progress", "pipe:1", "-i"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.push(input.to_string_lossy().into_owned());

    if o.format == Format::Gif {
        // 프레임 수·크기를 줄이고, 영상에서 뽑은 256색 팔레트로 변환해야 GIF 화질이 산다
        let fps = o.gif_fps.clamp(1, 50);
        let mut vf = format!("crop={w}:{h}:{x}:{y},fps={fps}");
        if let Some(gw) = o.gif_width.filter(|&gw| gw > 0 && gw < w) {
            vf += &format!(",scale={}:-2:flags=lanczos", even(gw));
        }
        // 짝수 보정으로 비율이 미세하게 달라져도 픽셀 비율은 1:1 로 (scale 뒤에 둬야 함)
        vf += ",setsar=1,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle";
        a.extend(["-map", "0:v:0", "-vf", &vf, "-an", "-loop", "0"].map(String::from));
        a.push(output.to_string_lossy().into_owned());
        return a;
    }

    let q = o.quality.min(100) as f64;
    a.extend(["-map", "0:v:0", "-map", "0:a:0?", "-vf", &crop].map(String::from));

    // 품질(0~100) → 코덱별 파라미터
    let crf = (35.0 - q * 0.2).round() as u32; // 100 → 15, 50 → 25, 0 → 35
    match o.codec.as_str() {
        "x265" => a.extend(["-c:v", "libx265", "-preset", "medium", "-crf", &(crf + 4).to_string(), "-tag:v", "hvc1"].map(String::from)),
        "vt264" => a.extend(["-c:v", "h264_videotoolbox", "-q:v", &(q.max(1.0) as u32).to_string()].map(String::from)),
        "vt265" => a.extend(["-c:v", "hevc_videotoolbox", "-q:v", &(q.max(1.0) as u32).to_string(), "-tag:v", "hvc1"].map(String::from)),
        // SVT-AV1: CRF 0~63 (100 → 20, 75 → 28, 50 → 36), preset 8 은 속도·화질 균형
        "av1" => a.extend(["-c:v", "libsvtav1", "-preset", "8", "-crf", &((52.0 - q * 0.32).round() as u32).to_string()].map(String::from)),
        _ => a.extend(["-c:v", "libx264", "-preset", "medium", "-crf", &crf.to_string()].map(String::from)),
    }
    a.extend(["-pix_fmt", "yuv420p"].map(String::from));

    let mp4_friendly = matches!(info.audio_codec.as_deref(), Some("aac" | "mp3" | "alac" | "ac3" | "eac3" | "opus" | "flac"));
    match (o.audio.as_str(), info.audio_codec.is_some()) {
        (_, false) | ("none", _) => a.push("-an".into()),
        ("copy", _) => a.extend(["-c:a", "copy"].map(String::from)),
        ("auto", _) if mp4_friendly => a.extend(["-c:a", "copy"].map(String::from)),
        _ => a.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from)),
    }
    a.extend(["-movflags", "+faststart"].map(String::from));
    a.push(output.to_string_lossy().into_owned());
    a
}

pub type Job = Arc<Mutex<Option<Child>>>;

/// ffmpeg 를 실행하고 진행률(0~1)을 콜백으로 알린다. 끝나면 Ok/Err.
pub fn run_export(
    ffmpeg: &Path,
    args: &[String],
    duration: f64,
    job: Job,
    mut on_progress: impl FnMut(f64),
) -> Result<(), String> {
    let mut child = tool_cmd(ffmpeg)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg 실행 실패: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    *job.lock().unwrap() = Some(child);

    // stderr 가 가득 차서 막히지 않도록 별도 스레드에서 읽는다
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
            if duration > 0.0 {
                on_progress((us / 1e6 / duration).clamp(0.0, 1.0));
            }
        }
    }

    let status = job.lock().unwrap().take().map(|mut c| c.wait());
    let stderr = err_reader.join().unwrap_or_default();
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        None => Err("취소되었습니다".into()),
        Some(Ok(_)) => {
            let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
            Err(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
        }
        Some(Err(e)) => Err(e.to_string()),
    }
}

pub fn cancel(job: &Job) {
    if let Some(mut c) = job.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// 사용자에게 보여줄 명령어 문자열 (복사해서 터미널에서 재현 가능하도록)
pub fn shell_join(args: &[String]) -> String {
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
    use super::*;

    #[test]
    fn av1_args() {
        let info = ProbeInfo {
            width: 1920, height: 1080, rotation: 0, duration: 1.0, fps: 30.0,
            video_codec: "h264".into(), audio_codec: Some("aac".into()), size: 0,
        };
        let o = ExportOptions {
            x: 0, y: 0, w: 608, h: 1080, format: Format::Mp4, codec: "av1".into(), quality: 75, audio: "auto".into(),
            gif_fps: 15, gif_width: None,
        };
        let args = build_args(Path::new("in.mp4"), Path::new("out.mp4"), &o, &info).join(" ");
        assert!(args.contains("-c:v libsvtav1 -preset 8 -crf 28"), "{args}");
        assert!(!args.contains("hvc1"));
    }

    #[test]
    fn gif_args() {
        let info = ProbeInfo {
            width: 1920, height: 1080, rotation: 0, duration: 1.0, fps: 30.0,
            video_codec: "h264".into(), audio_codec: Some("aac".into()), size: 0,
        };
        let o = ExportOptions {
            x: 0, y: 0, w: 608, h: 1080, format: Format::Gif, codec: "x264".into(), quality: 75, audio: "auto".into(),
            gif_fps: 12, gif_width: Some(481),
        };
        let args = build_args(Path::new("in.mp4"), Path::new("out.gif"), &o, &info).join(" ");
        assert!(args.contains("crop=608:1080:0:0,fps=12,scale=480:-2:flags=lanczos,setsar=1,split"), "{args}");
        assert!(args.contains("palettegen") && args.contains("paletteuse"), "{args}");
        assert!(args.contains("-an -loop 0 out.gif"), "{args}");
        assert!(!args.contains("-c:v") && !args.contains("0:a:0"), "{args}");
        // 크롭보다 크거나 같은 가로는 무시 (확대하지 않음)
        let o = ExportOptions { gif_width: Some(608), ..o };
        let args = build_args(Path::new("in.mp4"), Path::new("out.gif"), &o, &info).join(" ");
        assert!(!args.contains(",scale="), "{args}");
    }

    /// ffmpeg 이 설치된 환경에서 `VC_TEST_DIR=<샘플 폴더> cargo test -- --ignored` 로 실행
    #[test]
    #[ignore]
    fn export_samples() {
        let dir = PathBuf::from(std::env::var("VC_TEST_DIR").unwrap());
        let ffmpeg = find_tool("ffmpeg").unwrap();
        let ffprobe = find_tool("ffprobe").unwrap();
        for (name, codec) in [("land.mp4", "x264"), ("rot.mov", "vt264"), ("odd.avi", "x265")] {
            let input = dir.join(name);
            let info = probe(&ffprobe, &input).unwrap();
            println!("{name}: {info:?}");
            // 9:16 크롭, 홀수 크기 입력 → 짝수로 보정되는지
            let h = info.height;
            let w = h * 9 / 16 + 1;
            let o = ExportOptions {
                x: 101, y: 0, w, h, format: Format::Mp4, codec: codec.into(), quality: 70, audio: "auto".into(),
                gif_fps: 15, gif_width: None,
            };
            let out = dir.join(format!("out_{name}.mp4"));
            let args = build_args(&input, &out, &o, &info);
            let mut last = 0.0;
            run_export(&ffmpeg, &args, info.duration, Job::default(), |p| last = p).unwrap();
            let oi = probe(&ffprobe, &out).unwrap();
            println!("  -> {}x{} audio={:?} progress={last:.2}", oi.width, oi.height, oi.audio_codec);
            assert_eq!(oi.width % 2, 0);
            assert_eq!(oi.height % 2, 0);
        }
    }
}
