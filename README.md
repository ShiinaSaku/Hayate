<div align="center">

<img src="./assets/logo.svg" alt="Hayate Logo" height="120" />

# Hayate

**A blazing-fast, completion-based CLI and engine for secure file and directory transfers across local networks.**

[![CI](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.96%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/ShiinaSaku/Hayate?include_prereleases&sort=semver)](https://github.com/ShiinaSaku/Hayate/releases)
[![Website](https://img.shields.io/badge/website-docs-black)](https://shiinasaku.github.io/Hayate/)

[Features](#features) • [Quick Start](#quick-start) • [Usage & Commands](#usage--commands) • [Installation](#installation) • [Security Model](#security-model) • [Architecture](#workspace-architecture)

</div>

---

Hayate is a command-line tool and Rust library for transferring files and folders over local area networks (LAN). Powered by QUIC and completion-based async I/O (compio/io_uring), it saturates available local bandwidth while protecting transfer integrity and privacy.

> [!NOTE]
> **Why Hayate?**
> Standard utilities like scp or rsync require SSH setup, while tools like Magic Wormhole rely on external rendezvous servers. Hayate is designed for high-performance direct LAN transfers without configuration, using UDP broadcast for zero-setup peer discovery.

<details>
<summary>Show ASCII Art</summary>

```text
    __  _______  _____  ____________
   / / / /   \ \/ /   |/_  __/ ____/
  / /_/ / /| |\  / /| | / / / __/
 / __  / ___ |/ / ___ |/ / / /___
/_/ /_/_/  |_/_/_/  |_/_/ /_____/
```

</details>

## Features

- **Extreme Throughput**: Leverages completion-based asynchronous I/O (compio runtime using io_uring/IOCP) combined with parallel multithreaded AEAD/Zstd encoding/decoding.
- **Direct UDP/QUIC Protocol**: Built on compio-quic and quinn-proto state machines to minimize latency and transport overhead.
- **Auto-Discovery**: Pair devices instantly using simple broadcast passphrases—no IP lookup or typing required.
- **Secure by Default**: Ephemeral X25519 key agreements, HKDF key derivation, and ChaCha20-Poly1305 / AES-GCM frame encryption.
- **Directory Support**: Zero-overhead tar streaming directly from/to disks with strict path-traversal prevention.
- **Smart Compression**: Automatic zstd chunk compression with safety checks that skip pre-compressed formats (archives, media) to optimize CPU time.

---

## Quick Start

### Pairing Mode (Auto-Discovery)

Share a phrase to let the sender and receiver find each other over the local subnet:

```bash
# Receiver
hayate receive --code "apple-bravo-charlie"

# Sender
hayate send ./photos.zip --code "apple-bravo-charlie"
```

### Direct Mode

Connect directly via IP and Port when UDP broadcast is restricted (e.g., corporate VPNs, mobile hotspots):

```bash
# Receiver
hayate receive --port 50001

# Sender
hayate send ./archive.tar 192.168.1.50:50001
```

---

## Usage & Commands

### `hayate receive`

Prepares the local node to receive incoming files/directories. Features interactive [y/N] confirmation prompts and visual transfer progress.

```text
Usage: hayate receive [OPTIONS]

Options:
  -b, --bind <BIND>      IP address to bind [default: 0.0.0.0]
  -p, --port <PORT>      Port to listen on [default: 50001]
  -o, --output <OUTPUT>  Default directory for received files [default: .]
      --auto-accept      Accept transfers without prompting
      --code <CODE>      Pairing code phrase
      --no-tui           Disable progress UI
```

### `hayate send`

Transfers a file or directory to a specific target peer.

```text
Usage: hayate send [OPTIONS] <PATH> [TARGET]

Options:
      --peer <PEER>      Receiver address (equivalent to TARGET)
      --code <CODE>      Pairing code phrase
  -z, --compress         Compress chunks before encryption (default: true)
      --hash <ALGO>      Integrity algorithm: blake3, rapidhash, sha256 [default: blake3]
      --no-tui           Disable progress UI
```

### `hayate discover`

Scan the local subnet CIDR for active receivers listening on the network.

```text
Usage: hayate discover [OPTIONS]

Options:
  -t, --timeout <TIMEOUT>  Scan timeout in seconds [default: 3]
      --cidr <CIDR>        Subnet range to scan (e.g. 192.168.1.0/24)
```

---

## Security Model

Hayate builds its trust topology at the application layer over transport-layer encrypted QUIC connections:

1. **Passphrase KDF**: When a --code phrase is used, it acts as a high-entropy salt for key derivation.
2. **Metadata Confidentiality**: The filename, size, and integrity hash algorithms are fully encrypted and validated before the receiver is prompted to accept.
3. **Payload Integrity**: Every frame is authenticated via AEAD before decompression, and payload integrity is validated against a global stream checksum (blake3, rapidhash, or sha256).
4. **Extraction Security**: Directory unpacking rejects absolute paths, parent directory traversals (..), symbolic links, and hard links to mitigate path traversal attacks.

> [!WARNING]
> Direct mode without a --code phrase relies solely on network locality. In public or untrusted LAN environments, always use a shared pairing phrase to ensure authenticated encryption.

---

## Mobile & Termux Support

Android OS enforces restrictions on UDP broadcast operations. When running Hayate inside Termux, utilize Direct Mode:

```bash
# Receiver (on Phone)
./hayate receive --port 50002

# Sender (on Computer)
hayate send ./document.pdf 192.168.1.13:50002
```

---

## Installation

### macOS & Linux

```bash
curl -sSf https://shiinasaku.github.io/Hayate/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://shiinasaku.github.io/Hayate/install.ps1 | iex
```

> [!TIP]
> Precompiled binaries for major platforms (x86_64, aarch64) are also downloadable from the GitHub Releases Page (https://github.com/ShiinaSaku/Hayate/releases).

---

## Build From Source

### Prerequisites

- Rust toolchain version 1.95 or later.

### Steps

1. Clone the repository:
   ```bash
   git clone https://github.com/ShiinaSaku/Hayate.git
   cd Hayate
   ```
2. Build the release binary:
   ```bash
   cargo build --release -p hayate-cli
   ```
3. (Optional) Run checks and tests:
   ```bash
   just check
   ```

---

## Workspace Architecture

Hayate is structured as a Cargo workspace:

- **[hayate](hayate)**: The core standalone engine library (`hayate`). Contains all implementations for handshakes, crypto, network binding, tar stream handling, and payload transfer pipelines.
- **[hayate-cli](hayate-cli)**: The command-line interface wrapper that provides CLI options, interactive TUI progress bars, and pairing console prompts.

---

## Acknowledgements

Hayate is built using several excellent open-source libraries, including:

- [compio](https://github.com/compio-rs/compio) & compio-quic for completion-based asynchronous runtimes.
- quinn-proto & rustls for pure-Rust QUIC/TLS network state management.
- ring, blake3, rapidhash for cryptography and payload integrity verification.
- clap, indicatif, dialoguer for terminal interaction.
