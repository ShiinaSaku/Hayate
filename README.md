<div align="center">

<img src="./assets/logo.svg" alt="Hayate" height="96" />

# Hayate

**Encrypted LAN file transfer at wire speed.**

[![CI](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/ShiinaSaku/Hayate?sort=semver)](https://github.com/ShiinaSaku/Hayate/releases)
[![crates.io](https://img.shields.io/crates/v/hayate?color=orange)](https://crates.io/crates/hayate)
[![docs.rs](https://img.shields.io/docsrs/hayate?color=green)](https://docs.rs/hayate)
[![Stars](https://img.shields.io/github/stars/ShiinaSaku/Hayate?style=social)](https://github.com/ShiinaSaku/Hayate)

[Install](#install) • [Usage](#usage) • [Why Hayate](#why-hayate) • [Performance](#performance) • [Security](#security) • [Build](#build)

</div>

---

## What is Hayate?

Hayate sends files and folders across your local network — encrypted, compressed, and at full wire speed. No cloud, no accounts, no SSH setup.

It runs on **macOS, Linux, Windows, and Android (Termux)**. Peers find each other automatically via mDNS + UDP broadcast. You just share a 4-word code phrase.

```text
    __  _______  _____  ____________
   / / / /   \ \/ /   |/_  __/ ____/
  / /_/ / /| |\  / /| | / / / __/
 / __  / ___ |/ / ___ |/ / / /___
/_/ /_/_/  |_/_/_/  |_/_/ /_____/
```

Built on **QUIC** + **io_uring** / **IOCP** — the same kernel primitives that power modern CDNs and databases. Hayate saturates your LAN, not your CPU.

---

## What's new in v6

v6 is the performance release. One month of shipping, one week of tuning:

| Area | v5 | v6 |
|------|----|----|
| Frame size | 1 MiB | **4 MiB** — 4× fewer frames, 4× less per-frame overhead |
| Read-ahead pipeline | 4 frames | **8 frames** — 32 MiB of disk reads overlap with encryption |
| QUIC flow window | 24 / 48 MiB | **64 / 128 MiB** — no stalls on 10 GbE |
| Socket buffers | 25 MiB | **64 MiB** — matches QUIC window, no kernel drops |
| Release profile | `opt-level = "z"` | **`opt-level = 3`** — SIMD and AES-NI fully enabled |
| Dead code | `hkdf` + `sha2` crates | **removed** — ring handles both |
| `rand_core` compat | single ver | **0.6 (x25519) + 0.10 (rand) coexist** — Rust 1.96 ecosystem |

**Result**: Hayate v6 matches or exceeds SFTP throughput on Gigabit LAN on AES-NI hardware. On ARM (Apple Silicon, Android) ChaCha20-Poly1305 is selected automatically — no AES hardware required.

---

## Why Hayate?

| Tool           | Transport  | Discovery         | Encryption        | Android    |
| -------------- | ---------- | ----------------- | ----------------- | ---------- |
| `scp`/`rsync`  | TCP/SSH    | Manual IP only    | SSH               | No         |
| Magic Wormhole | TCP/TLS    | Rendezvous server | PAKE              | Via Python |
| LocalSend      | HTTP/HTTPS | mDNS              | TLS               | Yes        |
| **Hayate**     | **QUIC**   | **mDNS + UDP**    | **X25519 + AEAD** | **Yes**    |

Hayate is the only tool that combines QUIC transport, automatic LAN peer discovery without a rendezvous server, hardware-accelerated AEAD encryption, and native Android support — all in a single 4 MB binary.

**Feature highlights:**

- **QUIC transport** — 0-RTT handshakes, multiplexed streams, no head-of-line blocking
- **mDNS + UDP discovery** — peers on macOS, Linux, Windows, Android find each other automatically
- **X25519 + HKDF** key exchange with optional passphrase salting
- **AES-256-GCM** (hw-accelerated) or **ChaCha20-Poly1305** per-frame AEAD — auto-selected at runtime
- **Blake3**, **RapidHash**, or **SHA-256** integrity verification
- **Zstd compression** — auto-skips pre-compressed formats (.zip, .mp4, .jpg, etc.)
- **Tar streaming** for directories with path-traversal protection
- **Thread-per-core runtime** — compio with io_uring (Linux), IOCP (Windows), kqueue (macOS)

---

## Install

Automated multi-platform binary releases (cargo-dist) have been removed while a replacement
pipeline is worked out — for now, build from source.

### Build from source

```bash
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate
cargo build --release -p hayate-cli
./target/release/hayate --help
```

### From crates.io (library only)

`hayate` (the engine) is published; `hayate-cli` (the binary) is not — install it from source
above, or depend on the library directly:

```bash
cargo add hayate
```

### Android (Termux)

Build from source with the Android target — see `just android-aarch64` / `just android-all`.

---

## Usage

### Pairing Mode — auto-discovery

The sender generates a 4-word code phrase. Share it with the receiver. They find each other on the LAN using mDNS and UDP broadcast — no IP addresses needed.

```bash
# Sender (auto-generates a code phrase)
hayate send ./photos.zip

# Receiver (uses the same code)
hayate receive --code "apple-bravo-charlie-delta"
```

### Direct Mode — known address

```bash
# Receiver
hayate receive --port 50001

# Sender
hayate send ./archive.tar.gz 192.168.1.50:50001
```

### Discover peers on your network

```bash
hayate discover
# Scans all detected subnets, streams results live
```

### Directory transfers

Folders are streamed as tar archives — compressed, encrypted, and extracted safely:

```bash
hayate send ./my-project --code "forest-river-mountain"
hayate receive --code "forest-river-mountain" --output ./downloads/
```

---

## Commands

### `hayate receive`

| Flag            | Description              | Default   |
| --------------- | ------------------------ | --------- |
| `-b, --bind`    | IP to bind               | `0.0.0.0` |
| `-p, --port`    | UDP port                 | `50001`   |
| `-o, --output`  | Save directory           | `.`       |
| `--auto-accept` | Skip confirmation prompt | off       |
| `--code`        | Pairing code phrase      | none      |
| `--no-progress` | Disable progress bar     | off       |

### `hayate send`

| Flag             | Description                                           | Default        |
| ---------------- | ----------------------------------------------------- | -------------- |
| `PATH`           | File or directory to send                             | required       |
| `TARGET`         | Receiver `ip:port`                                    | optional       |
| `--code`         | Pairing code phrase                                   | auto-generated |
| `-z, --compress` | Zstd compression                                      | on             |
| `--hash`         | Integrity algorithm (`blake3`, `rapidhash`, `sha256`) | `blake3`       |
| `--no-progress`  | Disable progress bar                                  | off            |

### `hayate discover`

| Flag            | Description                            | Default |
| --------------- | -------------------------------------- | ------- |
| `-t, --timeout` | Scan timeout (seconds)                 | `15`    |
| `--cidr`        | Subnet to scan (e.g. `192.168.1.0/24`) | auto    |

---

## Performance

### How the pipeline works

```
Disk ──► [8 async reads × 4 MiB] ──► [N CPU workers: zstd + AEAD] ──► [QUIC 128 MiB window] ──► Wire
```

- **Reader**: 8 concurrent `read_at` calls keep the disk saturated; SSD latency is fully hidden behind compression and encryption.
- **Workers**: one thread per logical CPU core, each holding a pre-built `AeadKey` for the entire transfer — no per-frame key expansion.
- **Cipher**: `is_x86_feature_detected!("aes")` / `is_aarch64_feature_detected!("aes")` at runtime — AES-256-GCM on hardware that has it, ChaCha20-Poly1305 everywhere else. Both run at memory bandwidth.
- **QUIC window**: 64 MiB stream / 128 MiB connection — large enough that the receiver never sends backpressure on GbE or 10 GbE.

### Linux: unlock full socket buffers

The Linux kernel caps `SO_RCVBUF` at `rmem_max` (default **208 KiB** on most distros). Hayate v6 requests 64 MiB — without raising the cap the kernel silently clips it and QUIC stalls under load.

**One-time (current session):**

```bash
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
```

**Permanent (survives reboot):**

```bash
echo "net.core.rmem_max=134217728" | sudo tee -a /etc/sysctl.d/99-hayate.conf
echo "net.core.wmem_max=134217728" | sudo tee -a /etc/sysctl.d/99-hayate.conf
sudo sysctl -p /etc/sysctl.d/99-hayate.conf
```

> macOS and Windows manage UDP buffers automatically — no manual steps needed.

### Disabling compression for already-compressed data

Zstd is skipped automatically for common pre-compressed extensions (`.zip`, `.mp4`, `.jpg`, `.mkv`, …). For everything else, if you know your data is incompressible:

```bash
hayate send ./rawdump.bin --compress=false
```

---

## Security

Hayate builds its trust model at the application layer on top of QUIC's transport encryption.

| Layer            | Primitive                                                          |
| ---------------- | ------------------------------------------------------------------ |
| Key agreement    | X25519 ECDH (ephemeral, forward-secret)                            |
| Key derivation   | HKDF-SHA256 with passphrase as optional salt                       |
| Frame encryption | AES-256-GCM (hardware-accelerated) or ChaCha20-Poly1305            |
| Integrity        | Blake3 / RapidHash / SHA-256 stream hash                           |
| Directory safety | Path-traversal rejection (no `..`, no symlinks, no absolute paths) |
| Metadata         | Filename and size encrypted before transmission                    |

> **Pairing mode**: only peers who know the exact code phrase can derive the session key. **Direct mode**: relies on network locality (no passphrase).

All crypto is implemented via [`ring`](https://github.com/briansmith/ring) — no hand-rolled primitives, no `unsafe` crypto code.

---

## Build

```bash
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate

# Build release binary
cargo build --release -p hayate-cli

# Run checks
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or use [just](https://github.com/casey/just):

```bash
just build     # release build
just check     # fmt + clippy + test
```

### Minimum Rust version

Hayate v6 requires **Rust 1.96** (edition 2024). Install or update via:

```bash
rustup update stable
```

---

## Library

Hayate is also a Rust library. Embed the transfer engine in your own application:

```toml
[dependencies]
hayate = "6"
compio = { version = "0.19", features = ["macros", "runtime", "fs", "net", "time"] }
```

```rust
use hayate::{HayateSender, HayateReceiver, Metadata, DiscoveredPeer};

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let sender = HayateSender::new()
        .code("my-code".to_owned())
        .compress(true);

    let checksum = sender.send("./file.txt", |bytes| {
        println!("sent {bytes} bytes");
    }).await?;

    Ok(())
}
```

Full API docs at [docs.rs/hayate](https://docs.rs/hayate).

---

## Acknowledgements

Built with [compio](https://github.com/compio-rs/compio) (io_uring/IOCP runtime), [quinn-proto](https://github.com/quinn-rs/quinn) (QUIC), [rustls](https://github.com/rustls/rustls) (TLS), [ring](https://github.com/briansmith/ring) (crypto), [blake3](https://github.com/BLAKE3-team/BLAKE3), and [zstd](https://github.com/facebook/zstd).

---

<div align="center">

**[⬆ back to top](#hayate)**

</div>
