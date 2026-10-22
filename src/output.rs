//! 출력 파일 경로 만들기 (GUI·CLI 공용)

use std::path::{Path, PathBuf};

/// 저장 폴더 아래 Output 폴더를 쓴다. 없으면 만든다.
pub fn output_subdir(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(format!("저장 폴더가 없습니다: {}", dir.display()));
    }
    let sub = dir.join("Output");
    std::fs::create_dir_all(&sub).map_err(|e| format!("Output 폴더를 만들 수 없습니다: {e}"))?;
    Ok(sub)
}

/// 폴더 + 이름으로 출력 경로를 만든다. 확장자는 형식(ext)을 따른다
pub fn output_path(dir: &Path, name: &str, ext: &str) -> Result<PathBuf, String> {
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

#[cfg(test)]
mod tests {
    use super::output_path;

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
}
