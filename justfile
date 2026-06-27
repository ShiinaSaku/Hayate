set shell := ["sh", "-cu"]

default:
    @just --list

fmt:
    cargo fmt

fmt-check:
    cargo fmt -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

check: fmt-check clippy test

build target="hayate":
    cargo build --release -p hayate-cli

android-aarch64:
    rustup target add aarch64-linux-android
    CC="{{justfile_directory()}}/scripts/aarch64-linux-android-clang" \
    CXX="{{justfile_directory()}}/scripts/aarch64-linux-android-clang++" \
    AR="{{justfile_directory()}}/scripts/android-llvm-ar" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="{{justfile_directory()}}/scripts/aarch64-linux-android-clang" \
    cargo build --target aarch64-linux-android --release

android-x86_64:
    rustup target add x86_64-linux-android
    CC="{{justfile_directory()}}/scripts/x86_64-linux-android-clang" \
    CXX="{{justfile_directory()}}/scripts/x86_64-linux-android-clang++" \
    AR="{{justfile_directory()}}/scripts/android-llvm-ar" \
    CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="{{justfile_directory()}}/scripts/x86_64-linux-android-clang" \
    cargo build --target x86_64-linux-android --release

android-all: android-aarch64 android-x86_64

run *args:
    cargo run -p hayate-cli -- {{args}}

receive port="50001" output=".":
    cargo run -p hayate-cli -- receive --port "{{port}}" --output "{{output}}"

send file peer:
    cargo run -p hayate-cli -- send "{{file}}" --peer "{{peer}}"

discover timeout="5":
    cargo run -p hayate-cli -- discover --timeout "{{timeout}}"

clean:
    cargo clean

changelog:
    git cliff -o CHANGELOG.md
