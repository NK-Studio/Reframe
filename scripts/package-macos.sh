#!/usr/bin/env bash
# ffmpeg 를 동봉한 Video Crop.app 을 만들고 dist/ 에 zip 으로 묶는다. (macOS, Apple Silicon)
#
# 먼저 scripts/build-ffmpeg-macos.sh 로 build/ffmpeg-macos/ 를 만들어 둔다.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FFMPEG_DIR="$ROOT/build/ffmpeg-macos"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
APP="$ROOT/dist/Video Crop.app"
ZIP="$ROOT/dist/Video-Crop-$VERSION-macos-$(uname -m).zip"

[ -x "$FFMPEG_DIR/ffmpeg" ] || { echo "먼저 scripts/build-ffmpeg-macos.sh 를 실행하세요" >&2; exit 1; }

cd "$ROOT"
cargo build --release

rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/ffmpeg"
cp target/release/video-crop "$APP/Contents/MacOS/"
cp "$FFMPEG_DIR"/{ffmpeg,ffprobe,BUILD_INFO.txt,LICENSE-ffmpeg.txt} "$APP/Contents/Resources/ffmpeg/"
cp LICENSE THIRD_PARTY_NOTICES.md "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Video Crop</string>
    <key>CFBundleDisplayName</key><string>Video Crop</string>
    <key>CFBundleIdentifier</key><string>com.nkstudio.videocrop</string>
    <key>CFBundleExecutable</key><string>video-crop</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0. FFmpeg (GPL-3.0) included.</string>
</dict>
</plist>
EOF

# Apple Silicon 은 서명 없는 바이너리를 실행하지 않으므로 ad-hoc 서명 (안쪽부터)
codesign --force --sign - "$APP/Contents/Resources/ffmpeg/ffmpeg" "$APP/Contents/Resources/ffmpeg/ffprobe"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"

ditto -c -k --keepParent "$APP" "$ZIP"
echo "완료: $APP"
ls -lh "$ZIP"
