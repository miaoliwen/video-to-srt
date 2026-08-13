#!/bin/bash
# Build minimal audio-only FFmpeg for Windows (MinGW64 + Git Bash)
set -e

BUILD_DIR="/c/Users/ROG/AppData/Local/Temp/ffmpeg-build"
SRC_DIR="$BUILD_DIR/FFmpeg-n8.1.2"
INSTALL_DIR="$BUILD_DIR/install"
LOGS_DIR="$BUILD_DIR/logs"

# Must precede any tool use: when spawned from PowerShell/cmd, git-bash
# inherits the raw Windows PATH and its own /usr/bin tools are not on it.
export PATH="/d/mingw64/bin:/d/Git/usr/bin:$PATH"
export CC="gcc"
MAKE=make
command -v "$MAKE" >/dev/null 2>&1 || MAKE=mingw32-make

mkdir -p "$INSTALL_DIR" "$LOGS_DIR"

cd "$SRC_DIR"

# Clean previous config if interrupted
$MAKE distclean >/dev/null 2>&1 || true

echo "=== [$(date +%H:%M:%S)] Configuring minimal audio-only FFmpeg ==="
# Git Bash on Windows can kill forked children transiently (0xC0000142); retry.
for i in 1 2 3; do
    if ./configure \
        --prefix="$INSTALL_DIR" \
        --target-os=mingw64 \
        --arch=x86_64 \
        --enable-cross-compile \
        --disable-shared \
        --enable-static \
        --disable-everything \
        --disable-debug \
        --disable-doc \
        --disable-ffplay \
        --disable-network \
        --disable-autodetect \
        --disable-x86asm \
        --disable-hwaccels \
        --disable-avdevice \
        --disable-swscale \
        --enable-avfilter \
        --enable-filter=aresample \
        --enable-ffmpeg \
        --enable-ffprobe \
        --enable-small \
        --enable-decoder=aac,aac_latm,mp3,mp3float,opus,vorbis,flac,pcm_s16le,pcm_s16be,pcm_u8,pcm_s8,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le \
        --enable-encoder=pcm_s16le \
        --enable-parser=aac,aac_latm,mp3,opus,vorbis,flac,pcm \
        --enable-demuxer=mov,mp3,ogg,wav,matroska,avi,flv \
        --enable-muxer=wav \
        --enable-protocol=file \
        --enable-swresample \
        --disable-bzlib \
        --disable-iconv \
        --disable-lzma \
        --disable-sdl2 \
        --disable-xlib \
        --disable-zlib \
        --extra-cflags="-Os -ffunction-sections -fdata-sections" \
        --extra-ldflags="-Wl,--gc-sections" \
        > "$LOGS_DIR/configure.log" 2>&1; then
        break
    fi
    echo "configure attempt $i failed, retrying..." | tee -a "$LOGS_DIR/configure.log"
done

echo "Configure exit: $?"
tail -20 "$LOGS_DIR/configure.log"

echo "=== [$(date +%H:%M:%S)] Building (make -j4) ==="
$MAKE -j4 > "$LOGS_DIR/make.log" 2>&1
echo "Make exit: $?"
tail -10 "$LOGS_DIR/make.log"

echo "=== [$(date +%H:%M:%S)] Result ==="
if [ -f "./ffmpeg.exe" ]; then
    cp ./ffmpeg.exe "$BUILD_DIR/ffmpeg-lite.exe"
    cp ./ffprobe.exe "$BUILD_DIR/ffprobe-lite.exe"
    ls -lh "$BUILD_DIR/ffmpeg-lite.exe" "$BUILD_DIR/ffprobe-lite.exe"
    SIZE=$(stat -c%s "$BUILD_DIR/ffmpeg-lite.exe" 2>/dev/null || stat -f%z "$BUILD_DIR/ffmpeg-lite.exe")
    echo "ffmpeg-lite.exe: $SIZE bytes ($((SIZE / 1048576)) MB)"
else
    echo "ffmpeg.exe not found in build dir!"
    ls -la ./*.exe 2>/dev/null || echo "no exe in root"
fi
