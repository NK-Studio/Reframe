# Reframe

ffmpeg 기반 영상 **화면 영역(가로·세로) 크롭** 도구. Rust + [wry](https://github.com/tauri-apps/wry) / tao.

## 실행

배포본(macOS `.app`, Windows zip)에는 ffmpeg가 들어 있어 따로 설치할 필요가 없습니다.

소스에서 바로 실행할 때는 시스템에 설치된 ffmpeg를 사용합니다.

```bash
brew install ffmpeg   # ffmpeg, ffprobe 필요
cargo run --release              # 또는
cargo run --release -- 영상.mp4  # 파일 바로 열기
```

**Windows**: 배포본과 같은 GPL 빌드 ffmpeg를 `target\debug\ffmpeg\`, `target\release\ffmpeg\`에 받아 둡니다. 시스템 ffmpeg가 LGPL 빌드(예: `--disable-libx264`)면 H.264/H.265 소프트웨어 코덱이 목록에서 빠지므로 이 방법을 권장합니다. `cargo clean` 후에는 다시 실행하세요.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-ffmpeg-windows.ps1
cargo run --release
```

앱은 ffmpeg를 다음 순서로 찾습니다: 실행 파일 옆 `ffmpeg/` 폴더 → 실행 파일 옆 → `.app/Contents/Resources/ffmpeg/` → `PATH` → Homebrew 경로.

## 배포본 만들기 (ffmpeg 동봉)

**macOS (Apple Silicon)**: FFmpeg·x264·x265·SVT-AV1을 공식 소스에서 정적으로 빌드해 `.app`에 넣습니다. macOS 12 이상에서 실행됩니다.

```bash
brew install cmake pkgconf
./scripts/build-ffmpeg-macos.sh   # build/ffmpeg-macos/ 에 ffmpeg, ffprobe 생성 (수 분)
./scripts/package-macos.sh        # dist/Reframe.app, dist/Reframe-<버전>-macos-arm64.zip
```

**Windows (x64)**: [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)의 정적 GPL 빌드를 받아 `Reframe.exe` 옆 `ffmpeg\` 폴더에 넣습니다.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1   # dist\Reframe-<버전>-windows-x64.zip
```

동봉한 ffmpeg의 버전, 빌드 옵션, 소스 위치는 배포본 안의 `ffmpeg/BUILD_INFO.txt`에 기록됩니다.

**앱 아이콘**: 원본은 `assets/icon/reframe.prism`(Prism 문서)입니다. 고친 뒤 `reframe-1024.png`로 내보내고 아래를 실행하면 Windows `.ico`, macOS `.icns`, 창 아이콘, 정보 창 로고가 다시 만들어집니다 (Pillow 필요).

```bash
python scripts/make-icons.py
```

## 기능
- 비율 프리셋: 자유 · 원본 · 1:1 · 4:5 · 4:3 · 3:4 · 16:9 · 9:16 · 3:2 · 2:3 · 21:9 · 커스텀(a:b, ⇄ 뒤집기)
- 드래그 이동 / 8방향 핸들 리사이즈 (비율 고정 유지), 방향키 미세 이동, 3분할 가이드
- X/Y/너비/높이 숫자 입력 즉시 반영 (↑↓ · 라벨 드래그로 조절), 가로/세로로 늘리기, 가운데 정렬, 최대 크기
- [크롭 내보내기] → 내보내기 창에서 형식(MP4 / GIF)과 형식별 설정, 파일 이름·저장 폴더 지정
  - 덮어쓰기 확인, 원본 덮어쓰기 방지
  - "Output 폴더에 저장": 저장 폴더 아래 `Output` 폴더에 내보내기 (없으면 생성, 설정 기억)
- MP4 코덱: H.264/H.265 하드웨어(macOS VideoToolbox, 기본값) · H.264/H.265 소프트웨어(x264/x265) · AV1(SVT-AV1)
- MP4 오디오: 자동(mp4 호환이면 복사, 아니면 AAC) / 복사 / AAC / 제거
- GIF: 초당 프레임(10/15/20/30), 가로 크기 축소, 영상에서 뽑은 팔레트로 변환 (무한 반복, 소리 없음)
- 진행률 · 취소 · 실행한 ffmpeg 명령어 표시 · 완료 후 Finder 에서 보기
- 폰 영상 회전 메타데이터 반영, 가로·세로 짝수 자동 보정
- WebView 가 재생 못 하는 형식(AVI, 일부 MKV)은 ffmpeg 정지 프레임으로 미리보기

## 테스트
```bash
cargo test
VC_TEST_DIR=<샘플 폴더(land.mp4, rot.mov, odd.avi)> cargo test -- --ignored --nocapture
```

## 라이선스

[GPL-3.0](LICENSE). FFmpeg 등 외부 소프트웨어의 라이선스는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)를 참고하세요.
