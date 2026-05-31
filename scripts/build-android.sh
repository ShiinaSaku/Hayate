#!/bin/bash
set -e

# Android Termux Cross-Compilation script for Hayate
# Target: aarch64-linux-android (64-bit ARM Android)

echo "=== Adding rustup target aarch64-linux-android ==="
rustup target add aarch64-linux-android

# Path to Android NDK on macOS
NDK_VERSION="29.0.13846066"
NDK_HOME="/Users/saksham/Library/Android/sdk/ndk/${NDK_VERSION}"

if [ ! -d "$NDK_HOME" ]; then
    echo "Error: Android NDK not found at $NDK_HOME"
    exit 1
fi

echo "=== Setting up environment variables ==="
NDK_BIN="${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin"

# Prepend the NDK compilers to PATH
export PATH="${NDK_BIN}:${PATH}"

# Define linker, C compiler, and archiver for the target
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="aarch64-linux-android24-clang"
export CC_aarch64_linux_android="aarch64-linux-android24-clang"
export AR_aarch64_linux_android="llvm-ar"

# Force compilation with optimized release flags
echo "=== Building release binary for target aarch64-linux-android ==="
cargo build --target aarch64-linux-android --release

echo "=== Build Succeeded! ==="
echo "Binary located at: target/aarch64-linux-android/release/hayate"
