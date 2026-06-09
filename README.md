# Hayate (はやて)

[![CI](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![Builds](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/builds.yml)
[![Rust](https://img.shields.io/badge/rust-1.95%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/ShiinaSaku/Hayate?include_prereleases&sort=semver)](https://github.com/ShiinaSaku/Hayate/releases)
[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/ShiinaSaku/Hayate?utm_source=badge)

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

Hayate is a zero-config, high-performance CLI for sending files and directories between machines on a local network. Built on **QUIC** via `compio-quic` and `quinn-proto`, with completion-based asynchronous I/O (`io_uring` on Linux/Android, `IOCP` on Windows, `kqueue` on macOS), Hayate is designed to keep fast Wi-Fi and Ethernet links saturated without sacrificing safety.

---

## ✦ Features

* **Authenticated Encryption**: Ephemeral `X25519` key exchange (DH) and `ChaCha20-Poly1305` AEAD payload encryption.
* **Proactor Async Engine**: Driven by `compio` thread-per-core runtime for zero-cost async I/O.
* **Terminal-Safe Progress**: Enabled by default, with transfer rate, ETA, elapsed time, and connection/pairing spinners that behave consistently across desktop terminals and Termux.
* **Zero-Config Pairing**: Secure LAN pairing utilizing random code-phrase broadcasters—no manual IP swapping required.
* **Hardware Hashing**: Hashing utilizing `ring::digest` hardware-accelerated SHA-256 for integrity verification.
* **Smart Compression**: Concurrent `zstd` level 1 compression that automatically skips pre-compressed file extensions (e.g., `.zip`, `.mp4`, `.png`).
* **Direct Mode**: Support for direct peer connections via IPv4 and IPv6 (including bracketed IPv6 syntax like `[fd00::1]:50001`).
* **Cross-Platform**: Binary packages for macOS, Linux, Windows, and Android (Termux).

---

## ✦ CLI Experience

Hayate keeps normal commands focused on the active transfer. The banner is shown only for help-oriented entry points:

```bash
hayate
hayate --help
hayate help
hayate receive --help
```

During transfers, Hayate shows connection and pairing spinners followed by a terminal-safe progress bar:

```text
   ⠋ [00:00:04] [==============================>---------] 1.15 GiB/1.46 GiB (78%) 14.2 MiB/s ETA 22s
```

* **Consistent Output**: `send`, `receive`, and `discover` start directly with useful status lines instead of repeating the banner.
* **Steady Tick**: Spinners animate at a steady cadence while connecting, pairing, or transferring.
* **Headless Friendly**: Suppress visual indicators with `--no-progress` or the `--no-tui` alias.
* **Receiver Lifecycle**: `hayate receive` exits after one successful transfer, but keeps listening after failed connections, failed handshakes, rejected transfers, or dropped transfers.

---

## ✦ Quick Start

### 1. Pairing Mode

Use code-phrase pairing when you do not want to exchange IP addresses manually.

**On the Receiver:**
```bash
hayate receive --code "apple-bravo-charlie" --output ~/Downloads
```

**On the Sender:**
```bash
hayate send ./holiday_photos.zip --code "apple-bravo-charlie"
```
Hayate scans the local network, pairs the nodes with the shared code phrase, performs the key exchange, and transfers the payload.

### 2. Direct Mode

Specify the receiver address directly to skip pairing broadcasts.

**On the Receiver:**
```bash
hayate receive --port 50001
```

**On the Sender:**
```bash
hayate send ./large_archive.tar --peer 192.168.1.50:50001
```

You can also pass the target positionally:

```bash
hayate send ./large_archive.tar 192.168.1.50:50001
```

Use either positional `TARGET` or `--peer`, not both.

---

## ✦ CLI Command Reference

Run `hayate` or `hayate --help` for the top-level help menu. Normal commands do not print the banner; help commands do.

### `hayate receive`
Starts a receiver and waits for one incoming file or directory. After a successful receive it exits. If a connection attempt fails, the receiver reports the error and keeps listening.

Aliases: `recv`, `rx`

```text
Usage: hayate receive [OPTIONS]

Options:
  -b, --bind <BIND>      IP address to bind the QUIC listener [env: HAYATE_BIND=] [default: 0.0.0.0]
  -p, --port <PORT>      Port to listen on [env: HAYATE_PORT=] [default: 50001]
  -o, --output <OUTPUT>  Directory to save received files into [default: .]
      --auto-accept      Auto-accept all incoming transfers without prompting
      --no-progress      Suppress the progress bar and spinner output
      --code <CODE>      Cryptographic code-phrase for pairing
  -h, --help             Print help
```

### `hayate send`
Sends a file or directory to a receiver.

Alias: `tx`

```text
Usage: hayate send [OPTIONS] <PATH> [TARGET]

Arguments:
  <PATH>    Path to the file or directory to send
  [TARGET]  Receiver address in the form ip:port or hostname:port

Options:
      --peer <PEER>  Receiver address in the form ip:port or hostname:port (compat option)
      --code <CODE>  Cryptographic code-phrase for pairing
  -z, --compress     Compress chunks with zstd level 1 before encrypting
      --no-progress  Suppress the progress bar and spinner output
  -h, --help         Print help
```

`TARGET` and `--peer` are mutually exclusive. If neither is supplied, Hayate generates a pairing code and waits for a receiver using `hayate receive --code "<code>"`.

### `hayate discover`
Scans the local network subnet for active receivers.

Alias: `scan`

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

### Protocol and File Safety

Hayate treats the wire protocol as untrusted input, even on a LAN:

* Metadata is encrypted and authenticated before the receiver sees filenames, transfer sizes, or transfer type.
* Metadata transfer type is validated strictly: only file and directory payloads are accepted.
* File transfers must finish with exactly the announced byte count.
* Directory transfers are streamed as tar archives into the resolved output directory, and extraction creates that directory before writing entries.
* Archive entries with absolute paths, `..` traversal, symlinks, or hard links are rejected instead of being unpacked.
* Payload frames are length-capped and AEAD-authenticated before decompression or filesystem writes.

---

## ✦ Termux (Android) Usage

Android can limit multicast and broadcast discovery depending on device and network policy. Direct IP connections are the most reliable option:

**On Phone (Receiver):**
```bash
./hayate receive --port 50002 --auto-accept --no-progress
```

**On Computer (Sender):**
```bash
hayate send ./documents.pdf --peer 192.168.1.13:50002
```

`--no-progress` is recommended for scripted Termux runs or terminals that do not redraw progress output reliably.

---

## ✦ Installation

You can install Hayate instantly using the automated installation scripts:

### macOS, Linux, and Termux (bash)
Run the following command to download and install the latest binary to `/usr/local/bin` (or `$PREFIX/bin` in Termux):
```bash
curl -sSf https://raw.githubusercontent.com/ShiinaSaku/Hayate/refs/heads/master/scripts/install.sh | bash
```
Source code: [`scripts/install.sh`](scripts/install.sh)

### Windows (PowerShell)
Run the following command in PowerShell to download and install the Windows executable:
```powershell
irm https://raw.githubusercontent.com/ShiinaSaku/Hayate/refs/heads/master/scripts/install.ps1 | iex
```
Source code: [`scripts/install.ps1`](scripts/install.ps1)

### Manual Installation
Alternatively, you can manually download and configure precompiled binaries from the [Releases](https://github.com/ShiinaSaku/Hayate/releases) page.

---

## ✦ Building from Source

### Requirements
* **Rust compiler**: Stable 1.95+.
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
just run -- --help  # Run built CLI help menu
```

For direct cargo workflows:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets
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

See [CHANGELOG.md](CHANGELOG.md) for a list of notable changes in each version.

---

## ✦ License

MIT. See [LICENSE](LICENSE) for details.
