# Third-party notices

Video Crop 자체는 [GPL-3.0](LICENSE)으로 배포합니다. 함께 쓰는 외부 소프트웨어의 라이선스는 아래와 같습니다.

## FFmpeg

Video Crop은 FFmpeg(`ffmpeg`, `ffprobe`)를 별도 프로그램으로 실행해 영상 정보를 읽고 크롭·인코딩합니다.

- 저장소의 소스 코드에는 FFmpeg가 포함되어 있지 않습니다. 소스로 빌드해 쓸 때는 시스템에 설치된 FFmpeg를 사용합니다 (예: `brew install ffmpeg`).
- 릴리스에 FFmpeg 실행 파일을 함께 넣어 배포하는 경우, 그 FFmpeg는 libx264·libx265를 포함한 GPL 빌드이며 **GPL-3.0** 조건을 따릅니다. 릴리스 노트에 해당 FFmpeg의 버전, 빌드 옵션(`ffmpeg -buildconf`), 소스 위치를 함께 적습니다.

FFmpeg: https://ffmpeg.org · 소스: https://git.ffmpeg.org/ffmpeg.git · 라이선스: https://ffmpeg.org/legal.html

FFmpeg 빌드에 포함될 수 있는 주요 라이브러리:

| 라이브러리 | 라이선스 |
|---|---|
| x264 | GPL-2.0-or-later |
| x265 | GPL-2.0-or-later |

## Rust 크레이트

`Cargo.lock`에 있는 의존성은 모두 GPL-3.0과 함께 쓸 수 있는 라이선스입니다 (MIT, Apache-2.0, Zlib, Unicode-3.0, MPL-2.0 등). 주요 크레이트:

| 크레이트 | 라이선스 |
|---|---|
| [wry](https://github.com/tauri-apps/wry) | MIT OR Apache-2.0 |
| [tao](https://github.com/tauri-apps/tao) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [serde](https://github.com/serde-rs/serde), [serde_json](https://github.com/serde-rs/json) | MIT OR Apache-2.0 |

전체 목록은 `cargo metadata` 또는 [cargo-about](https://github.com/EmbarkStudios/cargo-about)으로 확인할 수 있습니다.
