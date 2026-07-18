set shell := ["sh", "-cu"]

default:
    @just --list

fmt:
    cargo +nightly fmt

fmt-check:
    cargo +nightly fmt -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --all-targets

check: fmt-check clippy test

build target="hayate":
    cargo build --release -p hayate-cli

# Cross-compile for every target this host can reach.
# Delegates to `bun run build.ts`, which handles zigbuild/ndk/linker setup
# correctly (a bare `cargo build --target` fails on most dev machines).
build-all:
    bun run build.ts --all

# Windows (and other non-host) cross-compiles via the build script.
build-windows:
    bun run build.ts --target x86_64-pc-windows-msvc

# Debian/Ubuntu .deb packages for Linux targets.
build-deb:
    bun run build.ts --all --deb

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
    cargo run -p hayate-cli -- send "{{file}}" "{{peer}}"

discover timeout="5":
    cargo run -p hayate-cli -- discover --timeout "{{timeout}}"

clean:
    cargo clean

# --- release helpers ---

# Show current version and recent tags
release-status:
    @echo "Workspace version: $$(grep 'version = ' Cargo.toml | head -1 | sed 's/.*\"\(.*\)\"/\1/')"
    @echo "hayate on crates.io: $$(cargo search hayate --limit 1 2>/dev/null | head -1 | cut -d'"' -f2)"
    @echo ""
    @echo "Recent tags:"
    @git tag -l 'v*' --sort=-version:refname | head -5
    @echo ""
    @echo "Pending commits since last tag:"
    @git log --oneline $(git describe --tags --abbrev=0 2>/dev/null || echo HEAD)..HEAD 2>/dev/null || echo "  (none)"
