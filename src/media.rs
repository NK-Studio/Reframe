//! `media://` 프로토콜: 열려 있는 영상 파일을 Range 요청에 맞춰 스트리밍하고,
//! 재생 불가 코덱용으로 특정 시점의 프레임(JPEG)을 내려준다.

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use wry::http::{Request, Response, StatusCode};

/// 한 번의 Range 응답에 담을 최대 바이트 수
const MAX_CHUNK: u64 = 4 * 1024 * 1024;

type Resp = Response<Cow<'static, [u8]>>;

pub fn handle(req: &Request<Vec<u8>>, path: Option<&Path>, ffmpeg: Option<&Path>) -> Resp {
    let Some(path) = path else {
        return error(StatusCode::NOT_FOUND, "no file");
    };
    match req.uri().path() {
        "/video" => stream(req, path).unwrap_or_else(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())),
        "/frame" => {
            let t = query_param(req.uri().query().unwrap_or(""), "t")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            match ffmpeg.ok_or("ffmpeg 없음".to_string()).and_then(|f| crate::ffmpeg::frame_jpeg(f, path, t)) {
                Ok(jpg) => Response::builder()
                    .header("Content-Type", "image/jpeg")
                    .header("Cache-Control", "no-store")
                    .body(Cow::Owned(jpg))
                    .unwrap(),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

fn query_param<'a>(q: &'a str, key: &str) -> Option<&'a str> {
    q.split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("ogv") => "video/ogg",
        Some("ts" | "mts" | "m2ts") => "video/mp2t",
        _ => "application/octet-stream",
    }
}

fn error(status: StatusCode, msg: &str) -> Resp {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(Cow::Owned(msg.as_bytes().to_vec()))
        .unwrap()
}

/// "bytes=start-end" / "bytes=start-" / "bytes=-suffix" 를 (start, end) 로 해석 (단일 범위만)
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (s, e) = spec.split_once('-')?;
    let (start, end) = match (s.trim(), e.trim()) {
        ("", suffix) => {
            let n: u64 = suffix.parse().ok()?;
            (len.saturating_sub(n), len - 1)
        }
        (s, "") => (s.parse().ok()?, len - 1),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.min(len - 1)),
    };
    (start <= end && start < len).then_some((start, end))
}

fn stream(req: &Request<Vec<u8>>, path: &Path) -> std::io::Result<Resp> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let builder = Response::builder()
        .header("Content-Type", mime(path))
        .header("Accept-Ranges", "bytes");

    let range = req.headers().get("range").and_then(|h| h.to_str().ok());
    let Some(range) = range else {
        // Range 없이 오면 앞부분만 주고 206 으로 응답 (전체를 메모리에 올리지 않기 위함)
        return read_part(&mut file, builder, 0, (MAX_CHUNK - 1).min(len.saturating_sub(1)), len);
    };
    match parse_range(range, len) {
        Some((start, end)) => read_part(&mut file, builder, start, end.min(start + MAX_CHUNK - 1), len),
        None => Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{len}"))
            .body(Cow::Borrowed(&[][..]))
            .unwrap()),
    }
}

fn read_part(
    file: &mut File,
    builder: wry::http::response::Builder,
    start: u64,
    end: u64,
    len: u64,
) -> std::io::Result<Resp> {
    let mut buf = Vec::with_capacity((end + 1 - start) as usize);
    file.seek(SeekFrom::Start(start))?;
    file.take(end + 1 - start).read_to_end(&mut buf)?;
    Ok(builder
        .status(StatusCode::PARTIAL_CONTENT)
        .header("Content-Range", format!("bytes {start}-{end}/{len}"))
        .header("Content-Length", buf.len())
        .body(Cow::Owned(buf))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(parse_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=200-", 100), None);
    }
}
