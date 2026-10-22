# Video Crop

ffmpeg 기반 영상 **화면 영역(가로·세로) 크롭** 도구. Rust + [wry](https://github.com/tauri-apps/wry) / tao.

## 실행

```bash
brew install ffmpeg   # ffmpeg, ffprobe 필요
cargo run --release              # 또는
cargo run --release -- 영상.mp4  # 파일 바로 열기
```

## 기능
- 비율 프리셋: 자유 · 원본 · 1:1 · 4:5 · 4:3 · 3:4 · 16:9 · 9:16 · 3:2 · 2:3 · 21:9 · 커스텀(a:b, ⇄ 뒤집기)
- 드래그 이동 / 8방향 핸들 리사이즈 (비율 고정 유지), 방향키 미세 이동, 3분할 가이드
- X/Y/너비/높이 숫자 입력 즉시 반영 (↑↓ · 라벨 드래그로 조절), 가로/세로로 늘리기, 가운데 정렬, 최대 크기
- 저장 칸에서 파일 이름(기본: 원본 이름)과 저장 폴더 지정, 덮어쓰기 확인, 원본 덮어쓰기 방지
- 출력 가로 지정 시 리사이즈 (예: 9:16 + 1080 → 1080×1920)
- 코덱: H.264/H.265 (소프트웨어) · VideoToolbox 하드웨어 (macOS)
- 오디오: 자동(mp4 호환이면 복사, 아니면 AAC) / 복사 / AAC / 제거
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
