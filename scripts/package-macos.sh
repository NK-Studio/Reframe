#!/usr/bin/env bash
# ffmpeg 를 동봉한 Reframe.app 을 만들고 dist/ 에 zip 으로 묶는다. (macOS, Apple Silicon)
#
# 먼저 scripts/build-ffmpeg-macos.sh 로 build/ffmpeg-macos/ 를 만들어 둔다.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FFMPEG_DIR="$ROOT/build/ffmpeg-macos"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
APP="$ROOT/dist/Reframe.app"
ZIP="$ROOT/dist/Reframe-$VERSION-macos-$(uname -m).zip"

[ -x "$FFMPEG_DIR/ffmpeg" ] || { echo "먼저 scripts/build-ffmpeg-macos.sh 를 실행하세요" >&2; exit 1; }

cd "$ROOT"
# 바이너리에 빌드 PC 경로(홈 폴더, 저장소 위치)가 남지 않게 바꿔 기록한다
RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$ROOT=/src --remap-path-prefix=$HOME=/home" cargo build --release
if strings target/release/Reframe | grep -q "$HOME"; then
    echo "오류: 바이너리에 홈 폴더 경로가 남아 있습니다" >&2
    exit 1
fi

rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/ffmpeg"
cp target/release/Reframe "$APP/Contents/MacOS/"
cp "$FFMPEG_DIR"/{ffmpeg,ffprobe,BUILD_INFO.txt,LICENSE-ffmpeg.txt} "$APP/Contents/Resources/ffmpeg/"
cp LICENSE THIRD_PARTY_NOTICES.md "$APP/Contents/Resources/"
cp ui/fonts/OFL.txt "$APP/Contents/Resources/LICENSE-IBMPlexSansKR.txt"
cp assets/icon/reframe.icns "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Reframe</string>
    <key>CFBundleDisplayName</key><string>Reframe</string>
    <key>CFBundleIdentifier</key><string>com.nkstudio.reframe</string>
    <key>CFBundleExecutable</key><string>Reframe</string>
    <key>CFBundleIconFile</key><string>reframe</string>
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
