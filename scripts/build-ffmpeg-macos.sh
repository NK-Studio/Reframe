#!/usr/bin/env bash
# 앱에 동봉할 정적 FFmpeg(ffmpeg, ffprobe)를 소스에서 빌드한다. (macOS, Apple Silicon)
#
# - FFmpeg, x264, x265, SVT-AV1 을 공식 소스에서 정적 라이브러리로 빌드한다 → GPL 빌드.
# - 결과는 build/ffmpeg-macos/ 에 둔다. Homebrew dylib 에 의존하지 않는다. (시스템 프레임워크만 사용)
# - Homebrew 의 x264/x265 는 최신 macOS 전용으로 빌드되어 있어 쓰지 않고 직접 빌드한다.
#
# 필요: Xcode Command Line Tools, cmake, pkgconf (brew install cmake pkgconf)
set -euo pipefail

FFMPEG_VERSION="${FFMPEG_VERSION:-9.0.2}"
X265_VERSION="${X265_VERSION:-4.2}"
X264_BRANCH="${X264_BRANCH:-stable}"
SVTAV1_VERSION="${SVTAV1_VERSION:-4.2.0}"
MACOS_MIN="${MACOS_MIN:-12.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/build/ffmpeg-src"
DEPS="$ROOT/build/ffmpeg-deps"
OUT="$ROOT/build/ffmpeg-macos"
JOBS="$(sysctl -n hw.ncpu)"
export MACOSX_DEPLOYMENT_TARGET="$MACOS_MIN"
# x264/x265/SVT-AV1 은 절대 경로로 컴파일되므로, 바이너리에 빌드 PC 경로가 남지 않게 바꿔 기록한다.
# (FFmpeg 은 상대 경로로 빌드되고 configure 옵션이 바이너리에 그대로 들어가므로 여기에 넣지 않는다)
PATH_MAP="-ffile-prefix-map=$WORK=/src -ffile-prefix-map=$DEPS=/deps"

for t in cmake pkg-config git; do
    command -v "$t" >/dev/null || { echo "$t 가 필요합니다 (brew install cmake pkgconf)" >&2; exit 1; }
done
rm -rf "$DEPS" && mkdir -p "$WORK" "$DEPS"

# 1) x264 (정적 라이브러리)
X264_SRC="$WORK/x264"
rm -rf "$X264_SRC"
git clone --quiet --depth 1 --branch "$X264_BRANCH" https://code.videolan.org/videolan/x264.git "$X264_SRC"
X264_COMMIT="$(git -C "$X264_SRC" rev-parse HEAD)"
(cd "$X264_SRC" && ./configure --prefix="$DEPS" --enable-static --disable-cli \
    --extra-cflags="-mmacosx-version-min=$MACOS_MIN $PATH_MAP" --extra-ldflags="-mmacosx-version-min=$MACOS_MIN" \
    && make -j"$JOBS" && make install-lib-static)

# 2) x265 (정적 라이브러리)
X265_TARBALL="$WORK/x265_$X265_VERSION.tar.gz"
[ -f "$X265_TARBALL" ] || curl -fL "https://download.videolan.org/pub/videolan/x265/x265_$X265_VERSION.tar.gz" -o "$X265_TARBALL"
X265_SRC="$WORK/x265_$X265_VERSION"
rm -rf "$X265_SRC" && tar -xf "$X265_TARBALL" -C "$WORK"
cmake -S "$X265_SRC/source" -B "$X265_SRC/build" -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_MIN" \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DENABLE_SHARED=OFF -DENABLE_CLI=OFF \
    -DCMAKE_C_FLAGS="$PATH_MAP" -DCMAKE_CXX_FLAGS="$PATH_MAP"
cmake --build "$X265_SRC/build" -j"$JOBS"
cmake --install "$X265_SRC/build"

# 3) SVT-AV1 (AV1 인코더, 정적 라이브러리)
SVT_TARBALL="$WORK/SVT-AV1-v$SVTAV1_VERSION.tar.gz"
[ -f "$SVT_TARBALL" ] || curl -fL "https://gitlab.com/AOMediaCodec/SVT-AV1/-/archive/v$SVTAV1_VERSION/SVT-AV1-v$SVTAV1_VERSION.tar.gz" -o "$SVT_TARBALL"
SVT_SRC="$WORK/SVT-AV1-v$SVTAV1_VERSION"
rm -rf "$SVT_SRC" && tar -xf "$SVT_TARBALL" -C "$WORK"
cmake -S "$SVT_SRC" -B "$SVT_SRC/build" -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$DEPS" -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_MIN" \
    -DBUILD_SHARED_LIBS=OFF -DBUILD_APPS=OFF -DBUILD_TESTING=OFF \
    -DCMAKE_C_FLAGS="$PATH_MAP" -DCMAKE_CXX_FLAGS="$PATH_MAP"
cmake --build "$SVT_SRC/build" -j"$JOBS"
cmake --install "$SVT_SRC/build"

# 4) FFmpeg 소스 받기
TARBALL="$WORK/ffmpeg-$FFMPEG_VERSION.tar.xz"
[ -f "$TARBALL" ] || curl -fL "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" -o "$TARBALL"
SRC="$WORK/ffmpeg-$FFMPEG_VERSION"
rm -rf "$SRC" && tar -xf "$TARBALL" -C "$WORK"

# 5) 빌드
cd "$SRC"
CONFIGURE_FLAGS=(
    --prefix=/opt/reframe/ffmpeg
    --enable-gpl
    --enable-version3
    --enable-static --disable-shared
    --disable-ffplay --disable-doc --disable-debug
    --disable-autodetect
    --enable-videotoolbox --enable-audiotoolbox
    --enable-libx264 --enable-libx265 --enable-libsvtav1
    --pkg-config-flags=--static
    --extra-ldflags="-mmacosx-version-min=$MACOS_MIN"
    --extra-cflags="-mmacosx-version-min=$MACOS_MIN"
)
PKG_CONFIG_PATH="$DEPS/lib/pkgconfig" ./configure "${CONFIGURE_FLAGS[@]}"
make -j"$JOBS"
rm -rf "$OUT" && mkdir -p "$OUT"
cp ffmpeg ffprobe "$OUT/"
strip "$OUT/ffmpeg" "$OUT/ffprobe"

# 6) GPL 고지에 필요한 정보 기록 (버전, 빌드 옵션, 소스 위치)
SHA="$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)"
cat > "$OUT/BUILD_INFO.txt" <<EOF
FFmpeg $FFMPEG_VERSION (static, macOS $MACOS_MIN+ $(uname -m))
Source: https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz
Source SHA-256: $SHA
x264: $X264_BRANCH @ $X264_COMMIT (https://code.videolan.org/videolan/x264)
x265: $X265_VERSION (https://download.videolan.org/pub/videolan/x265/x265_$X265_VERSION.tar.gz)
SVT-AV1: $SVTAV1_VERSION (https://gitlab.com/AOMediaCodec/SVT-AV1, BSD-3-Clause-Clear + AOM patent license)
License: GPL-3.0 (--enable-gpl --enable-version3)

configure ${CONFIGURE_FLAGS[*]}
EOF
cp COPYING.GPLv3 "$OUT/LICENSE-ffmpeg.txt"

# 7) Homebrew 경로 의존이나 빌드 PC 경로가 남아 있으면 실패 처리
if strings "$OUT/ffmpeg" "$OUT/ffprobe" "$OUT/BUILD_INFO.txt" | grep -q "$HOME"; then
    echo "오류: 바이너리에 빌드 PC 경로($HOME)가 남아 있습니다" >&2
    exit 1
fi
if otool -L "$OUT/ffmpeg" "$OUT/ffprobe" | grep -q -E '/opt/homebrew|/usr/local'; then
    echo "오류: Homebrew 라이브러리에 의존합니다" >&2
    otool -L "$OUT/ffmpeg" >&2
    exit 1
fi
echo "완료: $OUT"
ls -lh "$OUT"
