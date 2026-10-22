# Third-party notices

Video Crop 자체는 [GPL-3.0](LICENSE)으로 배포합니다. 함께 쓰는 외부 소프트웨어의 라이선스는 아래와 같습니다.

## FFmpeg

Video Crop은 FFmpeg(`ffmpeg`, `ffprobe`)를 별도 프로그램으로 실행해 영상 정보를 읽고 크롭·인코딩합니다.

- 저장소의 소스 코드에는 FFmpeg가 포함되어 있지 않습니다. 소스로 빌드해 쓸 때는 시스템에 설치된 FFmpeg를 사용합니다 (예: `brew install ffmpeg`).
- 배포본(macOS `.app`, Windows zip)에는 libx264·libx265·SVT-AV1을 포함한 GPL 빌드 FFmpeg가 들어 있으며, **GPL-3.0** 조건을 따릅니다.
  - macOS: `scripts/build-ffmpeg-macos.sh`로 FFmpeg·x264·x265·SVT-AV1 공식 소스에서 직접 빌드합니다.
  - Windows: [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)의 정적 GPL 빌드를 사용합니다.
- 동봉한 FFmpeg의 버전, 빌드 옵션, 소스 위치는 배포본의 `ffmpeg/BUILD_INFO.txt`에, FFmpeg 라이선스 전문은 `ffmpeg/LICENSE-ffmpeg.txt`에 들어 있습니다.

FFmpeg: https://ffmpeg.org · 소스: https://git.ffmpeg.org/ffmpeg.git · 라이선스: https://ffmpeg.org/legal.html

FFmpeg 빌드에 포함될 수 있는 주요 라이브러리:

| 라이브러리 | 라이선스 |
|---|---|
| x264 | GPL-2.0-or-later |
| x265 | GPL-2.0-or-later |
| SVT-AV1 | BSD-3-Clause-Clear + [AOM 특허 라이선스](https://aomedia.org/license/patent-license/) |

## Rust 크레이트

`Cargo.lock`에 있는 의존성은 모두 GPL-3.0과 함께 쓸 수 있는 라이선스입니다 (MIT, Apache-2.0, Zlib, Unicode-3.0, MPL-2.0 등). 주요 크레이트:

| 크레이트 | 라이선스 |
|---|---|
| [wry](https://github.com/tauri-apps/wry) | MIT OR Apache-2.0 |
| [tao](https://github.com/tauri-apps/tao) | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | MIT |
| [serde](https://github.com/serde-rs/serde), [serde_json](https://github.com/serde-rs/json) | MIT OR Apache-2.0 |

전체 목록은 `cargo metadata` 또는 [cargo-about](https://github.com/EmbarkStudios/cargo-about)으로 확인할 수 있습니다.
