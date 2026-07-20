<div align="center">

<img src="./assets/logo.svg" alt="Hayate" width="128" height="128" />

# Hayate

**Encrypted, compressed LAN file transfer. One binary, one command — no cloud, no accounts, no SSH.**

QUIC transport · X25519 + AEAD · 4-word pairing phrases · macOS / Linux / Windows / Android

[![Website](https://img.shields.io/badge/website-hayate.shiina.xyz-6ea8fe?style=flat-square)](https://hayate.shiina.xyz)
[![CI](https://img.shields.io/github/actions/workflow/status/ShiinaSaku/Hayate/ci.yml?style=flat-square&label=CI)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/hayate?style=flat-square&color=e37602&label=crates.io)](https://crates.io/crates/hayate)
[![npm](https://npmx.dev/api/registry/badge/version/@shiinasaku/hayate)](https://npmx.dev/package/@shiinasaku/hayate)
[![docs.rs](https://img.shields.io/docsrs/hayate?style=flat-square&color=3fb950&label=docs.rs)](https://docs.rs/hayate)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-dea584?style=flat-square&logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)

**English** · [日本語](./README.ja.md)

</div>

```console
$ hayate send ./photos.zip

   ●  Pairing code
   forest-river-mango-silver-orbit

   Receiver runs:  hayate receive --code "forest-river-mango-silver-orbit"
```

One command on each machine. Files move — encrypted, compressed, and
integrity-verified end to end.

---

## Overview

Moving a file between two machines in the same room should not require a cloud
round-trip, an SSH daemon, or a chat app's attachment limit. Hayate is a single
static binary that turns a local network into the fastest transfer medium you
own. It pairs devices with a human-readable code phrase, discovers peers over
mDNS and UDP broadcast, and pushes data over QUIC with application-layer
encryption — no server, no configuration, no trusted third party.

## Features

|                        |                                                                                                                                                     |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Performance**        | QUIC with 4 MiB frames, 8-deep asynchronous read-ahead, and compression + AEAD on dedicated worker threads.                                         |
| **Encryption**         | Ephemeral X25519 key agreement, HKDF-SHA256, per-frame AEAD (AES-256-GCM or ChaCha20-Poly1305), all via [ring](https://github.com/briansmith/ring). |
| **Zero configuration** | 4-word pairing phrases locate and authenticate peers over mDNS + UDP broadcast. Direct `ip:port` addressing works too.                              |
| **Portability**        | One self-contained binary for macOS, Linux, Windows, and Android (Termux) — x64 and arm64.                                                          |
| **Scriptability**      | NDJSON event stream, stable documented exit codes, `--quiet` / `--verbose`, machine-readable `--format json`.                                       |

Under the hood, Hayate runs on [compio](https://github.com/compio-rs/compio), a
completion-based async runtime (io_uring / IOCP / kqueue) — the same class of
kernel primitives used by modern high-throughput servers.

---

## Installation

### npm (recommended)

```bash
npm install -g @shiinasaku/hayate
hayate --help
```

The installer resolves the correct prebuilt binary for your platform as an
optional dependency: macOS, Linux, Windows, and Android/Termux, each in x64 and
arm64.

### GitHub Releases

Prebuilt archives (`.tar.gz` / `.zip`) and `.deb` packages, published with
`SHA256SUMS.txt` and npm provenance attestation:
[latest release](https://github.com/ShiinaSaku/Hayate/releases).

### Cargo (library)

```bash
cargo add hayate
```

The transfer engine ships as a [library crate](https://docs.rs/hayate) — build
your own interface on top of it.

### From source

```bash
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate
cargo build --release -p hayate-cli
./target/release/hayate --help
```

Requires **Rust 1.96** (edition 2024).

---

## Usage

### Pairing mode — no IP addresses

```bash
# Sender — prints a one-time code phrase and waits
hayate send ./photos.zip

# Receiver — joins with the same phrase
hayate receive --code "forest-river-mango-silver-orbit"
```

The phrase authenticates the session: it seeds key derivation, so a wrong
phrase fails decryption and the transfer aborts.

### Direct mode — you know the address

```bash
hayate receive --port 50001 --output ./downloads
hayate send ./archive.tar.gz 192.168.1.50:50001
```

Pass `--code <phrase>` on **both** sides to authenticate a direct transfer as
well; the phrase then acts as an out-of-band passphrase.

### Directories

Directories stream as a tar archive, compressed and encrypted on the fly.
Extraction rejects absolute paths, `..` components, and symlink escapes.

```bash
hayate send ./my-project
hayate receive --code "harbor-lantern-cedar-quartz-drift" --output ./downloads/
```

### Peer discovery

```bash
hayate discover
hayate discover --timeout 5 --cidr 192.168.1.0/24
```

---

## Command reference

### `hayate send <PATH> [TARGET]`

| Flag                      | Description                                  | Default                      |
| ------------------------- | -------------------------------------------- | ---------------------------- |
| `PATH`                    | File or directory to send                    | required                     |
| `TARGET`                  | Receiver `ip:port` (omit for pairing)        | —                            |
| `--code <phrase>`         | Pairing phrase (or passphrase with a target) | auto-generated when unpaired |
| `-z, --compress[=<bool>]` | Zstd compression                             | on                           |
| `--no-compress`           | Disable compression (conflicts with `-z`)    | off                          |
| `--hash <algo>`           | Integrity algorithm: `blake3` or `sha256`    | `blake3`                     |
| `--no-progress`           | Hide the progress bar                        | off                          |

### `hayate receive`

| Flag                 | Description            | Default                   |
| -------------------- | ---------------------- | ------------------------- |
| `-b, --bind <addr>`  | Bind address           | `0.0.0.0` (`HAYATE_BIND`) |
| `-p, --port <port>`  | Listen port            | `50001` (`HAYATE_PORT`)   |
| `-o, --output <dir>` | Save directory         | `.`                       |
| `--code <phrase>`    | Join a pairing session | none                      |
| `--auto-accept`      | Skip the accept prompt | off                       |
| `--no-progress`      | Hide the progress bar  | off                       |

### `hayate discover`

| Flag                   | Description                   | Default |
| ---------------------- | ----------------------------- | ------- |
| `-t, --timeout <secs>` | Scan timeout                  | `15`    |
| `--cidr <cidr>`        | Subnet, e.g. `192.168.1.0/24` | auto    |

### Global flags

| Flag                             | Description                            |
| -------------------------------- | -------------------------------------- |
| `--color <auto\|always\|never>`  | Color policy                           |
| `--format <pretty\|plain\|json>` | Human UI, plain text, or NDJSON events |
| `-q, --quiet`                    | Less output (repeatable)               |
| `-v, --verbose`                  | More detail (repeatable)               |

The full reference lives inside the binary: `hayate docs`.

---

## Scripting and automation

`--format json` emits one NDJSON event per line on stdout — stages, progress,
peers, summaries — safe to consume from any language.

```bash
hayate send ./build.tar.zst --format json | jq -r 'select(.type=="summary") | .speed_bps'
```

Exit codes are stable and documented (`hayate docs exit`):

| Code | Meaning                          |
| ---: | -------------------------------- |
|    0 | Success                          |
|    1 | General runtime / transfer error |
|    2 | Usage / argument error           |
|    3 | Receiver rejected the transfer   |
|    4 | Protocol version mismatch        |
|    5 | Invalid pairing passphrase       |
|    6 | Timed out                        |
|    7 | Cancelled by the user            |
|  130 | Interrupted (Ctrl+C / Esc)       |

---

## Shell completions

```bash
hayate completions bash --install   # ~/.bash_completion.d/hayate
hayate completions zsh --install    # ~/.zsh/completions/_hayate
hayate completions fish --install   # ~/.config/fish/completions/hayate.fish
hayate completions powershell       # print to stdout
```

After `--install`, Hayate prints the exact lines to add to your shell's rc
file. Fish loads completions automatically on the next session.

---

## Performance

```text
Disk → [8 async reads × 4 MiB] → [worker threads: zstd + AEAD] → [QUIC window] → Wire
```

- **Read-ahead** — concurrent `read_at` keeps the disk busy while cryptography
  runs.
- **Worker threads** — compression and AEAD execute on dedicated threads
  connected by channels, never blocking the async event loop.
- **Cipher negotiation** — AES-256-GCM with hardware AES, ChaCha20-Poly1305
  otherwise (override with `HAYATE_FORCE_CHACHA20`).
- **Selective compression** — zstd level 1, skipped for already-compressed
  extensions (`.zip`, `.mp4`, …).
- **Ordered disk writes** — the receiver reorders frames before writing, so
  disk I/O stays sequential.

### Linux socket buffers

If transfers stall under load, raise the kernel UDP buffer caps:

```bash
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
```

macOS and Windows size UDP buffers automatically.

---

## Security model

| Layer            | Primitive                                                    |
| ---------------- | ------------------------------------------------------------ |
| Key agreement    | Ephemeral X25519 ECDH                                        |
| Key derivation   | HKDF-SHA256, transcript-bound; passphrase mixed into the IKM |
| Frame encryption | AES-256-GCM or ChaCha20-Poly1305, fresh nonce per frame      |
| Integrity        | BLAKE3 or SHA-256 over plaintext                             |
| Metadata         | Filename and size encrypted before use                       |
| Directory safety | Rejects `..`, absolute paths, symlink escapes                |

In pairing mode, only a peer that knows the phrase can derive the session key —
a wrong phrase fails metadata AEAD and the transfer aborts. Discovery itself is
unauthenticated by design; **the handshake is what authenticates the transfer**.
Direct mode relies on network locality unless you pass `--code` on both sides
as a passphrase.

All cryptography is provided by [ring](https://github.com/briansmith/ring) —
no hand-rolled primitives.

---

## Comparison

| Tool           | Transport    | Discovery         | Encryption        | Server needed      | Android    |
| -------------- | ------------ | ----------------- | ----------------- | ------------------ | ---------- |
| scp / rsync    | TCP / SSH    | manual IP         | SSH               | sshd               | limited    |
| Magic Wormhole | TCP / TLS    | rendezvous server | PAKE              | yes (public relay) | via Python |
| LocalSend      | HTTP / HTTPS | mDNS              | TLS               | no                 | yes        |
| croc           | TCP relay    | code phrase       | PAKE              | optional relay     | via Go     |
| **Hayate**     | **QUIC**     | **mDNS + UDP**    | **X25519 + AEAD** | **no**             | **native** |

---

## Library usage

```toml
[dependencies]
hayate = "6"
compio = { version = "0.19", features = ["macros", "runtime", "fs", "net", "time"] }
```

```rust
use hayate::HayateSender;

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let checksum = HayateSender::new()
        .code("forest-river-mango-silver-orbit".to_owned())
        .compress(true)
        .send("./file.txt", |bytes| {
            println!("sent {bytes} bytes");
            Ok(())
        })
        .await?;

    println!("done: {checksum}");
    Ok(())
}
```

Public APIs never expose compio types, but the returned futures must run on a
**compio** runtime. For custom interfaces, the staged API reports every
lifecycle step: `HayateSender::send_with`, `HayateReceiver::receive_with`,
`ListeningReceiver`, and `TransferStage`. Documentation:
[docs.rs/hayate](https://docs.rs/hayate).

---

## Development

```bash
just check          # fmt (nightly rustfmt) + clippy + tests
just fmt            # cargo +nightly fmt
just clippy         # clippy --workspace --all-targets -D warnings
just test           # cargo test --workspace
```

| Task            | Command                 |
| --------------- | ----------------------- |
| Release binary  | `bun run build`         |
| Cross-compile   | `bun run build:all`     |
| Debian packages | `bun run build:deb`     |
| Android         | `bun run build:android` |

Cross-builds are driven by `build.ts` (`cargo-zigbuild` for Linux, `cargo-ndk`
for Android). Releases are versioned with
[Tegami](https://tegami.fuma-nama.dev) and published to crates.io and npm by
GitHub Actions over OIDC trusted publishing, with SHA-256 checksums and npm
provenance attestation.

---

## Acknowledgements

Built with [compio](https://github.com/compio-rs/compio),
[quinn-proto](https://github.com/quinn-rs/quinn),
[rustls](https://github.com/rustls/rustls),
[ring](https://github.com/briansmith/ring),
[BLAKE3](https://github.com/BLAKE3-team/BLAKE3), and
[zstd](https://github.com/facebook/zstd).

---

<div align="center">

**MIT licensed** · [Issues](https://github.com/ShiinaSaku/Hayate/issues) · [Releases](https://github.com/ShiinaSaku/Hayate/releases) · [Website](https://hayate.shiina.xyz)

</div>
