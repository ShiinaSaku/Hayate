<p align="center">
  <img src="../assets/logo.svg" width="140" height="140" alt="Hayate Logo">
</p>

<h1 align="center">Hayate Engine</h1>

<p align="center">
  <strong>Encrypted, resumable LAN file transfer as a library — the engine powering the Hayate CLI.</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/hayate"><img src="https://img.shields.io/crates/v/hayate.svg" alt="Crates.io"></a>
  <a href="https://docs.rs/hayate"><img src="https://docs.rs/hayate/badge.svg" alt="Documentation"></a>
  <a href="https://shiinasaku.github.io/Hayate/"><img src="https://img.shields.io/badge/website-docs-black" alt="Website"></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

---

`hayate` is a completion-based transfer engine for moving files and directories across a local network — encrypted, compressed, and resumable. It gives you two builders, [`HayateSender`] and [`HayateReceiver`], and handles QUIC transport, X25519 key agreement, AEAD framing, zstd compression, integrity hashing, and safe tar streaming behind them.

Driven by [`compio`](https://github.com/compio-rs/compio) (io_uring / IOCP / kqueue) and `compio-quic`, with AEAD + zstd on dedicated worker threads so the async executor never blocks.

## Highlights

- **Resumable** — interrupted single-file transfers continue from the last 4 MiB frame; already-complete files are hash-verified without resending a byte.
- **Throttleable** — `bandwidth_limit(bytes_per_sec)` caps sustained throughput for shared LANs.
- **Encrypted end to end** — ephemeral X25519 + Argon2id phrase stretch + HKDF-SHA256 + per-frame AEAD (AES-256-GCM with hardware acceleration, ChaCha20-Poly1305 elsewhere).
- **Integrity-checked** — every payload is hashed (`blake3` or `sha256`) on both sides; sizes are verified exactly.
- **Staged API** — observe connect → handshake → offer → transfer → finish via `TransferStage` for rich UIs.
- **Stable surface** — the semver guarantee covers `runner` and crate-root re-exports; `#[doc(hidden)]` internals are free to evolve.

## Installation

```toml
[dependencies]
hayate = "7.0"
compio = { version = "0.19", features = ["macros", "runtime", "fs", "net", "time"] }
```

## Send

```rust
use std::net::SocketAddr;
use hayate::HayateSender;

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let target: SocketAddr = "192.168.1.50:50001".parse().unwrap();

    let checksum = HayateSender::new()
        .target(target)
        .compress(true)
        .hash_algo("blake3".to_string())
        .bandwidth_limit(50 * 1024 * 1024) // optional: cap at 50 MiB/s
        .send("photos", |bytes| {
            println!("sent {bytes} bytes");
            Ok(())
        })
        .await?;

    println!("checksum {checksum}");
    Ok(())
}
```

## Receive (with resume)

```rust
use std::net::SocketAddr;
use hayate::HayateReceiver;

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let bind_addr: SocketAddr = "0.0.0.0:50001".parse().unwrap();

    let (checksum, path) = HayateReceiver::new()
        .bind(bind_addr)
        .resume(true) // continue interrupted transfers from partial files
        .receive(
            "./downloads",
            |meta| {
                println!("incoming {} ({} bytes)", meta.filename, meta.total_size);
                true
            },
            |bytes| {
                println!("received {bytes} bytes");
                Ok(())
            },
        )
        .await?;

    println!("saved to {} — checksum {checksum}", path.display());
    Ok(())
}
```

## Pairing Mode

Discover peers across the LAN via code-phrase broadcasts. The phrase is also mixed into key derivation, so strangers on the LAN cannot decrypt your metadata.

```rust
use hayate::{HayateReceiver, HayateSender};

# async fn sender() -> Result<(), hayate::EngineError> {
HayateSender::new()
    .code("alpha-bravo-charlie".to_owned())
    .send("report.pdf", |_| Ok(()))
    .await?;
# Ok(())
# }

# async fn receiver() -> Result<(), hayate::EngineError> {
let (_checksum, _path) = HayateReceiver::new()
    .code("alpha-bravo-charlie".to_owned())
    .auto_accept(true)
    .receive("./downloads", |_| true, |_| Ok(()))
    .await?;
# Ok(())
# }
```

> [!NOTE]
> Android devices and VPNs often block broadcasts. Fall back to direct IP mode when necessary.

## API Stability

| Surface | Semver-guaranteed? |
| ------- | ------------------ |
| `runner` (`HayateSender`, `HayateReceiver`, `ListeningReceiver`, `TransferStage`, outcomes) | ✅ stable |
| Crate-root re-exports (`EngineError`, `Metadata`, `TransferKind`, …) | ✅ stable |
| `crypto`, `network`, `discovery`, `local_addr`, `protocol` | ✅ stable |
| `transfer`, `tar`, `pool` (`#[doc(hidden)]`) | ⚠️ unstable — may change in any minor release |

CI runs `cargo-semver-checks` on every push, so accidental breaks in the stable surface fail the build before they reach crates.io.

## Runtime Notes

`compio` uses completion-based I/O: buffers are owned and passed by value to I/O operations, returning via `compio::BufResult`.

Hayate keeps blocking work off the executor: hashing, `tar` extraction, and zstd/AEAD run on dedicated `std::thread` workers connected by bounded `flume` channels. Payload frames use per-worker counter nonces, so the hot loop never calls the CSPRNG. The receiver reorders frames in a `BTreeMap` to guarantee sequential disk writes.

## Safety Guarantees

Hayate treats network data as fundamentally hostile:

- **Encrypted metadata**: filename, size, and hash choice are authenticated before any consent prompt.
- **Strict framing**: frames are length-capped and AEAD-verified before decompression.
- **Size verification**: completed files must match the announced byte count exactly.
- **Path sanitization**: directory extraction rejects absolute paths, `..` traversal, and symlinks; hard links are replayed only after their in-archive target exists.
- **Strict versioning**: the wire protocol is v8; incompatible peers fail fast with a clear
  `ProtocolMismatch` error instead of deriving mismatched keys.

## License

MIT — see [LICENSE](../LICENSE).
