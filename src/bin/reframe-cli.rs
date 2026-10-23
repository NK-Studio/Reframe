//! Reframe 명령줄 도구: 창 없이 영상 정보 확인, 크롭·내보내기.
//!
//! GUI 와 같은 ffmpeg 를 쓴다. (실행 파일 옆 `ffmpeg/`, macOS 앱 번들의 `Contents/Resources/ffmpeg/`, 그다음 PATH)

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use reframe_core::ffmpeg::{self, ExportOptions, Format, ProbeInfo};
use reframe_core::output::output_path;

#[derive(Parser)]
#[command(name = "reframe-cli", version, about = "Reframe 명령줄 도구: 영상 화면 영역 크롭")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 영상 정보 (크기, 길이, fps, 코덱)
    Info {
        input: PathBuf,
        /// JSON 으로 출력
        #[arg(long)]
        json: bool,
    },
    /// 이 ffmpeg 에서 쓸 수 있는 MP4 코덱
    Codecs,
    /// 영역을 잘라 MP4 또는 GIF 로 내보낸다
    Crop(CropArgs),
}

#[derive(clap::Args)]
struct CropArgs {
    input: PathBuf,
    /// 자를 영역 x,y,w,h (픽셀, 회전을 반영한 화면 기준)
    #[arg(long, value_parser = parse_rect)]
    rect: (u32, u32, u32, u32),
    /// 출력 파일. 생략하면 원본 옆 <이름>_crop.<확장자>
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// 출력 형식. 생략하면 출력 파일 확장자, 그것도 없으면 mp4
    #[arg(short, long)]
    format: Option<Fmt>,
    /// MP4 코덱: vt264 vt265 (macOS 하드웨어), x264 x265, av1. 생략하면 vt264 → x264 순
    #[arg(short, long)]
    codec: Option<String>,
    /// MP4 품질 0~100 (높을수록 고화질)
    #[arg(short, long, default_value_t = 75, value_parser = clap::value_parser!(u32).range(0..=100))]
    quality: u32,
    /// MP4 오디오
    #[arg(long, default_value = "auto")]
    audio: Audio,
    /// GIF 초당 프레임 수
    #[arg(long, default_value_t = 15)]
    gif_fps: u32,
    /// 출력 가로 크기. --height 없이 주면 세로는 크롭 비율대로 (생략하면 크롭 크기 그대로). GIF 는 줄이기만 된다
    #[arg(long, alias = "gif-width", value_parser = clap::value_parser!(u32).range(2..))]
    width: Option<u32>,
    /// 출력 세로 크기. --width 와 함께 주면 정확히 그 크기로 (예: --width 1080 --height 1920)
    #[arg(long, value_parser = clap::value_parser!(u32).range(2..))]
    height: Option<u32>,
    /// 같은 이름의 파일이 있으면 덮어쓴다
    #[arg(short = 'y', long)]
    overwrite: bool,
    /// 실행할 ffmpeg 명령만 출력하고 끝낸다
    #[arg(long)]
    dry_run: bool,
    /// 진행률을 표시하지 않는다
    #[arg(long)]
    quiet: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Fmt {
    Mp4,
    Gif,
}

#[derive(Clone, Copy, ValueEnum)]
enum Audio {
    Auto,
    Copy,
    Aac,
    None,
}

fn parse_rect(s: &str) -> Result<(u32, u32, u32, u32), String> {
    let v: Vec<u32> = s
        .split(',')
        .map(|p| p.trim().parse().map_err(|_| format!("숫자가 아닙니다: {p}")))
        .collect::<Result<_, _>>()?;
    match v[..] {
        [x, y, w, h] if w > 0 && h > 0 => Ok((x, y, w, h)),
        [_, _, _, _] => Err("w, h 는 0보다 커야 합니다".into()),
        _ => Err("x,y,w,h 네 값이 필요합니다 (예: 0,0,1080,1920)".into()),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("오류: {e}");
            ExitCode::FAILURE
        }
    }
}

fn tool(name: &str) -> Result<PathBuf, String> {
    ffmpeg::find_tool(name).ok_or_else(|| format!("{name} 를 찾을 수 없습니다"))
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Cmd::Info { input, json } => {
            let info = ffmpeg::probe(&tool("ffprobe")?, &input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else {
                print_info(&info);
            }
            Ok(())
        }
        Cmd::Codecs => {
            for c in ffmpeg::available_encoders(&tool("ffmpeg")?) {
                println!("{c}");
            }
            Ok(())
        }
        Cmd::Crop(a) => crop(a),
    }
}

fn print_info(i: &ProbeInfo) {
    println!("크기     {}×{}", i.width, i.height);
    if i.rotation != 0 {
        println!("회전     {}°", i.rotation);
    }
    println!("길이     {:.2}초", i.duration);
    println!("fps      {:.3}", i.fps);
    println!("비디오   {}", i.video_codec);
    println!("오디오   {}", i.audio_codec.as_deref().unwrap_or("없음"));
    println!("파일     {:.1} MB", i.size as f64 / 1_048_576.0);
}

fn crop(a: CropArgs) -> Result<(), String> {
    let ffmpeg_bin = tool("ffmpeg")?;
    let info = ffmpeg::probe(&tool("ffprobe")?, &a.input)?;

    let ext_of = |p: &Path| p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    let format = match (a.format, a.output.as_deref().and_then(ext_of).as_deref()) {
        (Some(Fmt::Gif), _) | (None, Some("gif")) => Format::Gif,
        _ => Format::Mp4,
    };
    let output = match &a.output {
        Some(p) if ext_of(p).as_deref() == Some(format.ext()) => p.clone(),
        Some(p) => p.with_extension(format.ext()),
        None => {
            let stem = a.input.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
            let dir = a.input.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
            output_path(dir, &format!("{stem}_crop"), format.ext())?
        }
    };
    let same = |p: &Path, q: &Path| p.canonicalize().ok().zip(q.canonicalize().ok()).is_some_and(|(p, q)| p == q);
    if same(&output, &a.input) {
        return Err("원본 파일에 덮어쓸 수 없습니다".into());
    }
    if output.exists() && !a.overwrite && !a.dry_run {
        return Err(format!("이미 있는 파일입니다 (덮어쓰려면 -y): {}", output.display()));
    }

    let (x, y, w, h) = a.rect;
    if x >= info.width || y >= info.height {
        return Err(format!("영역이 화면({}×{}) 밖에 있습니다", info.width, info.height));
    }
    let codec = if format == Format::Mp4 {
        let available = ffmpeg::available_encoders(&ffmpeg_bin);
        match a.codec {
            Some(c) if available.contains(&c.as_str()) => c,
            Some(c) => return Err(format!("쓸 수 없는 코덱입니다: {c} (가능: {})", available.join(", "))),
            None => ["vt264", "x264"]
                .into_iter()
                .chain(available.iter().copied())
                .find(|c| available.contains(c))
                .ok_or("쓸 수 있는 MP4 코덱이 없습니다")?
                .to_string(),
        }
    } else {
        String::new()
    };

    let opts = ExportOptions {
        x,
        y,
        w,
        h,
        format,
        codec,
        quality: a.quality,
        audio: match a.audio {
            Audio::Auto => "auto",
            Audio::Copy => "copy",
            Audio::Aac => "aac",
            Audio::None => "none",
        }
        .into(),
        gif_fps: a.gif_fps,
        out_w: a.width,
        out_h: a.height,
    };
    let args = ffmpeg::build_args(&a.input, &output, &opts, &info);
    if a.dry_run {
        println!("{} {}", ffmpeg_bin.display(), ffmpeg::shell_join(&args));
        return Ok(());
    }

    // Ctrl+C: ffmpeg 는 콘솔 없이 실행되므로 신호를 받지 못한다. 직접 끝낸다
    let job: ffmpeg::Job = Arc::default();
    {
        let job = job.clone();
        let _ = ctrlc::set_handler(move || ffmpeg::cancel(&job));
    }

    let show = !a.quiet && std::io::stderr().is_terminal();
    let mut last = -1.0;
    let res = ffmpeg::run_export(&ffmpeg_bin, &args, info.duration, job, |p| {
        if show && p - last >= 0.001 {
            last = p;
            eprint!("\r내보내는 중 {:5.1}%", p * 100.0);
            let _ = std::io::stderr().flush();
        }
    });
    if show {
        eprint!("\r{:20}\r", "");
    }
    res?;
    println!("{}", output.display());
    Ok(())
}
