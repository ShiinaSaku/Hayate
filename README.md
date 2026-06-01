# Hayate

[![CI](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![Builds](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml)
[![Rust](https://img.shields.io/badge/rust-1.96%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/ShiinaSaku/Hayate?include_prereleases&sort=semver)](https://github.com/ShiinaSaku/Hayate/releases)

Fast encrypted file transfer for local networks, terminals, scripts, and Termux.

```text
    __ __                 __
   / // /___ ___ __ ___ _/ /____
  / _  / _ '/ // / _ '/ __/ -_)
 /_//_/\_,_/\_, /\_,_/\__/\___/
           /___/
```

Hayate is a Rust CLI for sending files between machines over QUIC. It uses ephemeral X25519 key exchange, ChaCha20-Poly1305 authenticated encryption, adaptive zstd compression, LAN discovery, IPv4/IPv6 direct mode, and a terminal UI that falls back cleanly for scripts and mobile terminals.

Built on [compio](https://github.com/compio-rs/compio) (completion-based async I/O: io_uring on Linux, IOCP on Windows, kqueue on macOS) and [quinn-proto](https://github.com/quinn-rs/quinn) for the QUIC state machine.

Current project version: **v2.0.0**.

## Highlights

- Encrypted by default with fresh X25519 session keys per transfer.
- QUIC over UDP with high-throughput flow-control windows for fast LAN and Wi-Fi 6/6E paths.
- IPv4 and IPv6 direct mode, including bracketed IPv6 peers like `[fd00::50]:50001`.
- mDNS peer discovery when the network and OS permit multicast.
- Adaptive compression: skip already-compressed media/archives and send raw chunks when zstd does not help.
- TUI and headless modes for desktop terminals, SSH, CI, scripts, and Termux.
- SHA-256 verification shown after each completed transfer.
- Release builds for macOS, Linux, Windows, and Termux arm64.

## Quick Start

Start a receiver:

```bash
hayate receive --port 50001 --output ~/Downloads
```

Send over IPv4:

```bash
hayate send ./video.mp4 --peer 192.168.1.50:50001
```

Send over IPv6:

```bash
hayate send ./video.mp4 --peer '[fd00::50]:50001'
```

Discover receivers on the LAN:

```bash
hayate discover --duration 5s
```

Use headless mode for Termux, SSH, and scripts:

```bash
hayate receive --port 50001 --output . --no-tui
hayate send ./file.bin --peer 192.168.1.50:50001 --no-tui
```

## Commands

```text
hayate send <file> [--peer ip:port|[ipv6]:port] [--duration 3s] [--compress auto|always|never] [--no-tui]
hayate receive [--port 50001] [--name name] [--output dir] [--no-tui]
hayate discover [--duration 3s]
hayate version
```

Flags may be placed before or after positional arguments:

```bash
hayate send ./file.bin --peer 192.168.1.50:50001 --compress auto
hayate send --peer '[fd00::50]:50001' --compress never ./archive.zip
```

## Compression

`auto` is the default. It skips known incompressible file types and also sends individual chunks raw when zstd does not reduce their size.

```text
auto    Use extension and per-chunk heuristics.
always  Force zstd frames.
never   Send raw encrypted frames.
```

Recommended choices:

```text
Photos, videos, APKs, ZIP/RAR/7z, PDFs:  --compress never or auto
Text, CSV, JSON, logs, source trees:     --compress auto
Known compressible backups:              --compress always
```

## Performance

Hayate is tuned for fast local networks:

- 8 MB encrypted frames to reduce per-frame overhead.
- Worker count scales with CPU capacity, capped to avoid runaway scheduling.
- Deep transfer queues to keep disk, crypto, compression, and QUIC busy.
- Large QUIC receive windows so fast Wi-Fi/LAN paths are not flow-control throttled.

Actual speed still depends on both devices, storage speed, CPU thermal limits, Wi-Fi signal quality, router behavior, and whether Android allows the needed network operations.

## Termux And Android

Android often restricts multicast discovery. Direct mode is the reliable path:

```bash
# Phone
./hayate-termux-arm64 receive --port 50001 --output ~/storage/downloads --no-tui

# Computer
./hayate send ./file.bin --peer PHONE_IP:50001 --no-tui --compress auto
```

If discovery fails, check that both devices are on the same network and use direct `--peer` mode. IPv6 direct mode works when both devices have routable IPv6 addresses on the LAN.

## Security

Hayate protects file contents in transit against passive local-network observers.

- Each connection negotiates a fresh X25519 shared secret.
- Payload frames are authenticated and encrypted with ChaCha20-Poly1305.
- Metadata is encrypted before transfer.
- Incoming filenames are sanitized before writing to disk.
- Frame and metadata sizes are bounded before allocation.
- Release artifacts include SHA-256 checksums.

Hayate does not currently provide persistent peer identity, certificate pinning, or remote attestation. Use direct `--peer` addresses on trusted local networks.

## Install

Download a release archive from the releases page, then verify checksums:

```bash
sha256sum -c SHA256SUMS
```

On macOS:

```bash
shasum -a 256 -c SHA256SUMS
```

Make the binary executable:

```bash
chmod +x hayate-linux-amd64
./hayate-linux-amd64 version
```

For Termux, use the Linux arm64/Termux build:

```bash
chmod +x hayate-termux-arm64
./hayate-termux-arm64 receive --no-tui --port 50001
```

## Build

Requirements:

- Rust toolchain (edition 2024, MSRV 1.96).
- `just` is optional, but recommended for local development.

Common tasks:

```bash
just check
just build
just run version
just receive 50001 .
just send ./file.bin 192.168.1.50:50001
```

Without `just`:

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p hayate-cli
```

## CI

GitHub Actions run:

- `ci.yml`: format check, clippy, tests on Linux/macOS/Windows.
- `builds.yml`: cross-platform static binaries and SHA-256 artifacts for macOS, Linux, Termux, and Windows.

## Release Targets

```text
hayate-darwin-amd64
hayate-darwin-arm64
hayate-linux-amd64
hayate-linux-arm64
hayate-termux-arm64
hayate-windows-amd64.exe
hayate-windows-arm64.exe
```

## Protocol Compatibility

v2.0.0 is not wire-compatible with v1.x peers because encrypted payload frames include a compression flag.

Both sender and receiver should run the same major version.

## Contributing

Keep changes small, tested, and security-conscious.

Before opening a pull request:

```bash
just check
```

Report security issues privately until a fix is available.

## License

MIT. See [LICENSE](LICENSE).
