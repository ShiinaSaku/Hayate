# Hayate (はやて)

[![CI](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![Builds](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml)
[![Rust](https://img.shields.io/badge/rust-1.96%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/ShiinaSaku/Hayate?include_prereleases&sort=semver)](https://github.com/ShiinaSaku/Hayate/releases)

> Encrypted, compressed, blazing-fast cross-device file transfer for local networks, terminals, and Termux.

```text
  __   __     _____    __  __    _____    _______     _____  
 /\_\ /_/\   /\___/\ /\  /\  /\ /\___/\ /\_______)\ /\_____\ 
( ( (_) ) ) / / _ \ \\ \ \/ / // / _ \ \\(___  __\/( (_____/ 
 \ \___/ /  \ \(_)/ / \ \__/ / \ \(_)/ /  / / /     \ \__\   
 / / _ \ \  / / _ \ \  \__/ /  / / _ \ \ ( ( (      / /__/_  
( (_( )_) )( (_( )_) ) / / /  ( (_( )_) ) \ \ \    ( (_____\ 
 \/_/ \_\/  \/_/ \_\/  \/_/    \/_/ \_\/  /_/_/     \/_____/ 

   Swift File Transfer | Secure, Encrypted, & Compressed
```

Hayate is a zero-config, highly-optimized CLI tool written in Rust to send files and directories between machines on a local network. Built on **QUIC** (via `compio-quic` and `quinn-proto`) and completion-based asynchronous I/O (`io_uring` on Linux/Android, `IOCP` on Windows, `kqueue` on macOS), Hayate bypasses typical bottlenecks to maximize your Wi-Fi 6/6E or Ethernet pipelines.

---

## ✦ Features

* **Authenticated Encryption**: Ephemeral `X25519` key exchange (DH) and `ChaCha20-Poly1305` AEAD payload encryption.
* **Proactor Async Engine**: Driven by `compio` thread-per-core runtime for zero-cost async I/O.
* **Modern Progress Bar**: Enabled by default, featuring high-fidelity, sub-block unicode bars (`█▉▊▋▌▍▎▏  `), transfer rates, and ETA spinner.
* **Zero-Config Pairing**: Secure LAN pairing utilizing random code-phrase broadcasters—no manual IP swapping required.
* **Hardware Hashing**: Hashing utilizing `ring::digest` hardware-accelerated SHA-256 for integrity verification.
* **Smart Compression**: Concurrent `zstd` level 1 compression that automatically skips pre-compressed file extensions (e.g., `.zip`, `.mp4`, `.png`).
* **Direct Mode**: Support for direct peer connections via IPv4 and IPv6 (including bracketed IPv6 syntax like `[fd00::1]:50001`).
* **Cross-Platform**: Binary packages for macOS, Linux, Windows, and Android (Termux).

---

## ✦ Modern Progress Bar (Enabled by Default)

Hayate comes out-of-the-box with a high-fidelity visual progress indicator:

```text
 ⠋ [00:00:04] ▕█████████████████████████████████▍       ▏ 1.15 GiB/1.46 GiB (78%) 14.2 MiB/s 22s
```

* **Smooth Blocks**: Sub-character resolution fills standard terminals elegantly.
* **Steady Tick**: The progress spinner animates smoothly at 80ms ticks regardless of disk read speeds.
* **Headless Friendly**: Running in scripts, SSH sessions, or Termux environments? Suppress visual bars easily by passing `--no-progress` or the `--no-tui` alias.

---

## ✦ Quick Start

### 1. Pairing Mode (Secure & Automatic)

When you do not want to lookup IP addresses, use code-phrase pairing.

**On the Receiver:**
```bash
hayate receive --code "apple-bravo-charlie" --output ~/Downloads
```

**On the Sender:**
```bash
hayate send ./holiday_photos.zip --code "apple-bravo-charlie"
```
*Hayate will automatically scan the local subnet, pair the nodes, perform key exchanges, and transfer the file.*

### 2. Direct Mode (Immediate IP Target)

Specify the IP directly to skip pairing broadcasts.

**On the Receiver:**
```bash
hayate receive --port 50001
```

**On the Sender:**
```bash
hayate send ./large_archive.tar --peer 192.168.1.50:50001
```

---

## ✦ CLI Command Reference

### `hayate receive`
Starts a local receiver endpoint.
```text
Usage: hayate receive [OPTIONS]

Options:
  -b, --bind <BIND>      IP address to bind the QUIC listener [default: 0.0.0.0]
  -p, --port <PORT>      Port to listen on [default: 50001]
  -o, --output <OUTPUT>  Directory to save received files into [default: .]
      --auto-accept      Auto-accept all incoming transfers without prompting
      --no-progress      Suppress the progress bar (alias: --no-tui)
      --code <CODE>      Cryptographic code-phrase for automatic pairing
  -h, --help             Print help
```

### `hayate send`
Sends a file or directory to a receiver.
```text
Usage: hayate send [OPTIONS] <PATH> [TARGET]

Arguments:
  <PATH>    Path to the file or directory to send
  [TARGET]  Receiver address in the form `ip:port` or `hostname:port`

Options:
      --peer <PEER>  Receiver address in the form `ip:port` (compat option)
      --code <CODE>  Cryptographic code-phrase for pairing
  -z, --compress     Compress chunks with zstd level 1 before encrypting
      --no-progress  Suppress the progress bar (alias: --no-tui)
  -h, --help         Print help
```

### `hayate discover`
Scans the local network subnet for active receivers.
```text
Usage: hayate discover [OPTIONS]

Options:
  -t, --timeout <TIMEOUT>  Network scan timeout in seconds [default: 3]
      --cidr <CIDR>        Override the subnet CIDR to scan (e.g. 192.168.1.0/24)
  -h, --help               Print help
```

---

## ✦ Security Threat Model

A common question is: **Why does Hayate encrypt payloads using ChaCha20-Poly1305 if QUIC already encrypts all traffic via TLS 1.3?**

1. **Unauthenticated TLS**: Hayate uses self-signed ephemeral certificates generated dynamically. Because there is no PKI or Certificate Authority (CA) verifying these certificates on a local network, standard TLS is vulnerable to **Man-in-the-Middle (MITM)** spoofing attacks.
2. **Cryptographic Channel Binding**: To prevent MITM attacks, Hayate derives a shared key by salting a Diffie-Hellman key exchange with the user's code-phrase.
3. **Payload Protection**: If an attacker intercepts the connection, they cannot decrypt the metadata or payload frames without knowing the code-phrase. The application-layer encryption acts as an authenticated channel-binding mechanism.

---

## ✦ Termux (Android) Usage

Android OS limits multicast discovery. Direct IP connections are recommended:

**On Phone (Receiver):**
```bash
./hayate receive --port 50002 --auto-accept --no-progress
```

**On Computer (Sender):**
```bash
hayate send ./documents.pdf --peer 192.168.1.13:50002
```

---

## ✦ Installation

You can install Hayate instantly using the automated installation scripts:

### macOS, Linux, and Termux (bash)
Run the following command to download and install the latest binary to `/usr/local/bin` (or `$PREFIX/bin` in Termux):
```bash
curl -sSf https://raw.githubusercontent.com/ShiinaSaku/Hayate/refs/heads/master/scripts/install.sh | bash
```
*(Source code: [install.sh](file:///Users/saksham/Projects/Hayate/scripts/install.sh))*

### Windows (PowerShell)
Run the following command in PowerShell to download and install the Windows executable:
```powershell
irm https://raw.githubusercontent.com/ShiinaSaku/Hayate/refs/heads/master/scripts/install.ps1 | iex
```
*(Source code: [install.ps1](file:///Users/saksham/Projects/Hayate/scripts/install.ps1))*

### Manual Installation
Alternatively, you can manually download and configure precompiled binaries from the [Releases](https://github.com/ShiinaSaku/Hayate/releases) page.

---

## ✦ Building from Source

### Requirements
* **Rust compiler**: Stable (1.96+ edition 2024).
* **just** (optional, command runner).

### Build steps
```bash
# Clone the repository
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate

# Compile the release CLI binary
cargo build --release -p hayate-cli

# The optimized binary will be located at:
# target/release/hayate
```

Using `just` recipes:
```bash
just check     # Lints, formatting, and unit testing checks
just build     # Build release CLI target
just run -h    # Run built CLI help menu
```

---

## ✦ Acknowledgements & Special Thanks

Hayate stands on the shoulders of giants. Special thanks to the authors and maintainers of these incredible Rust crates that make this project possible:

* **[compio](https://github.com/compio-rs/compio)**: For providing the completion-based proactor async I/O runtime (`io_uring`/`IOCP`/`kqueue`).
* **[quinn-proto](https://github.com/quinn-rs/quinn)**: For the high-performance, protocol-correct QUIC state machine.
* **[rustls](https://github.com/rustls/rustls)**: For memory-safe, modern TLS 1.3 protocol support.
* **[ring](https://github.com/briansmith/ring)**: For robust and fast hardware-accelerated cryptographic primitives.
* **[dalek-cryptography](https://github.com/dalek-cryptography)** (`x25519-dalek`): For secure Curve25519 Diffie-Hellman exchanges.
* **[RustCrypto](https://github.com/RustCrypto)** (`chacha20poly1305`): For pure-Rust AEAD encryption/decryption primitives.
* **[clap](https://github.com/clap-rs/clap)**: For parsing command-line parameters elegantly.
* **[indicatif](https://github.com/console-rs/indicatif)**: For the smooth terminal progress indicators.
* **[zstd-rs](https://github.com/gyscos/zstd-rs)**: For the lossless compression algorithms.
* **[rcgen](https://github.com/est31/rcgen)**: For runtime self-signed X.509 certificate generation.

---

## ✦ Changelog

See [CHANGELOG.md](file:///Users/saksham/Projects/Hayate/CHANGELOG.md) for a list of notable changes in each version.

---

## ✦ License

MIT. See [LICENSE](LICENSE) for details.
