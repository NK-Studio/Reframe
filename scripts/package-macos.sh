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
if strings target/release/Reframe target/release/reframe-cli | grep -q "$HOME"; then
    echo "오류: 바이너리에 홈 폴더 경로가 남아 있습니다" >&2
    exit 1
fi

rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/ffmpeg"
cp target/release/Reframe target/release/reframe-cli "$APP/Contents/MacOS/"
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
codesign --force --sign - "$APP/Contents/Resources/ffmpeg/ffmpeg" "$APP/Contents/Resources/ffmpeg/ffprobe" "$APP/Contents/MacOS/reframe-cli"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"

ditto -c -k --keepParent "$APP" "$ZIP"
echo "완료: $APP"
ls -lh "$ZIP"

# 설치 프로그램(pkg): 소개 → 라이선스 동의 → 설치. /Applications 에 앱을 넣고 reframe-cli 를 /usr/local/bin 에 링크한다
PKG="$ROOT/dist/Reframe-$VERSION-macos-$(uname -m).pkg"
WORK="$ROOT/build/pkg"
rm -rf "$WORK" "$PKG" && mkdir -p "$WORK/root/Applications" "$WORK/scripts" "$WORK/res"
ditto "$APP" "$WORK/root/Applications/Reframe.app"
cp installer/macos/postinstall "$WORK/scripts/" && chmod +x "$WORK/scripts/postinstall"
# 다른 위치에 있는 Reframe.app 을 찾아 그 자리에 덮어쓰지 않도록(재배치 끔)
pkgbuild --analyze --root "$WORK/root" "$WORK/component.plist"
plutil -replace 0.BundleIsRelocatable -bool NO "$WORK/component.plist"
pkgbuild --root "$WORK/root" --install-location / --component-plist "$WORK/component.plist" \
    --scripts "$WORK/scripts" --identifier com.nkstudio.reframe --version "$VERSION" "$WORK/Reframe.pkg"
cp LICENSE "$WORK/res/LICENSE.txt"
cat > "$WORK/res/welcome.html" <<EOF
<html><body style="font-family:-apple-system">
<h2>Reframe $VERSION</h2>
<p>영상의 화면 영역을 잘라 MP4·GIF 로 내보내는 도구입니다.</p>
<ul>
<li>앱: <b>/Applications/Reframe.app</b></li>
<li>명령줄 도구: <b>reframe-cli</b> (/usr/local/bin 에 링크)</li>
</ul>
</body></html>
EOF
cat > "$WORK/distribution.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>Reframe</title>
    <welcome file="welcome.html" mime-type="text/html"/>
    <license file="LICENSE.txt"/>
    <options customize="never" require-scripts="false" hostArchitectures="arm64"/>
    <domains enable_localSystem="true"/>
    <allowed-os-versions><os-version min="12.0"/></allowed-os-versions>
    <choices-outline><line choice="default"><line choice="app"/></line></choices-outline>
    <choice id="default"/>
    <choice id="app" visible="false"><pkg-ref id="com.nkstudio.reframe"/></choice>
    <pkg-ref id="com.nkstudio.reframe" version="$VERSION" onConclusion="none">Reframe.pkg</pkg-ref>
</installer-gui-script>
EOF
productbuild --distribution "$WORK/distribution.xml" --resources "$WORK/res" --package-path "$WORK" "$PKG"
echo "완료: $PKG"
ls -lh "$PKG"
