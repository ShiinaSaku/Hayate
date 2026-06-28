<p align="center">
  <img src="../assets/logo.svg" width="140" height="140" alt="Hayate Logo">
</p>

<h1 align="center">Hayate Engine</h1>

<p align="center">
  <strong>Standalone high-performance completion-based transfer engine powering the Hayate CLI.</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/hayate"><img src="https://img.shields.io/crates/v/hayate.svg" alt="Crates.io"></a>
  <a href="https://docs.rs/hayate"><img src="https://docs.rs/hayate/badge.svg" alt="Documentation"></a>
  <a href="https://shiinasaku.github.io/Hayate/"><img src="https://img.shields.io/badge/website-docs-black" alt="Website"></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

---

`hayate` is the high-performance transfer engine powering the Hayate CLI. It provides high-level `HayateSender` and `HayateReceiver` builders for encrypted file and directory transfers over QUIC, plus low-level modules for apps needing custom protocol control.

Driven by `compio` (an `io_uring`/IOCP runtime) and `compio-quic`, Hayate achieves blazing fast throughput utilizing a multi-threaded AEAD and Zstd pipeline, ephemeral X25519 key agreements, dynamic payload hashing (`blake3`, `rapidhash`, `sha256`), and safe `tar` streaming.

## Installation

```toml
[dependencies]
hayate = "5.1.1"
compio = { version = "0.19", features = ["macros", "runtime", "fs", "net", "time"] }
```

## Receive API

```rust
use std::net::SocketAddr;
use hayate::HayateReceiver;

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let bind_addr: SocketAddr = "0.0.0.0:50001".parse().unwrap();

    let receiver = HayateReceiver::new().bind(bind_addr);
    let (checksum, path) = receiver.receive(
        "./downloads",
        |meta| {
            println!("Incoming: {} ({} bytes)", meta.filename, meta.total_size);
            true // Accept the transfer
        },
        |bytes| println!("Received {bytes} bytes"),
    ).await?;

    // Saved payload info
    println!("Saved to {}", path.display());
    println!("Checksum: {checksum}");
    Ok(())
}
```

## Send API

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
        .send("photos", |bytes| println!("Sent {bytes} bytes"))
        .await?;

    println!("Checksum: {checksum}");
    Ok(())
}
```

## Pairing Mode

Discover peers across the LAN via UDP broadcasts. The code phrase doubles as the HKDF salt.

```rust
use hayate::{HayateReceiver, HayateSender};

# async fn sender() -> Result<(), hayate::EngineError> {
HayateSender::new()
    .code("alpha-bravo-charlie".to_owned())
    .send("report.pdf", |_| {})
    .await?;
# Ok(())
# }

# async fn receiver() -> Result<(), hayate::EngineError> {
let (_checksum, _path) = HayateReceiver::new()
    .code("alpha-bravo-charlie".to_owned())
    .auto_accept(true)
    .receive("./downloads", |_| true, |_| {})
    .await?;
# Ok(())
# }
```

> [!NOTE]
> Android devices and VPNs often block broadcasts. Fallback to direct IP mode when necessary.

## Module Architecture

| Module       | Purpose                                                                      |
| ------------ | ---------------------------------------------------------------------------- |
| `runner`     | Builder-style APIs: `HayateSender` and `HayateReceiver`.                     |
| `transfer`   | Pipeline orchestration: handshake, consent, multithreaded receive/send pool. |
| `protocol`   | Wire constants, frame flags, metadata structure, and limits.                 |
| `crypto`     | Ephemeral X25519, HKDF, AEAD wrappers, and cipher selection.                 |
| `network`    | QUIC bindings and ephemeral `rustls` configurations.                         |
| `discovery`  | Pairing-code UDP broadcast and listener logic.                               |
| `tar`        | Streaming directory packing and extraction.                                  |
| `local_addr` | Network interface and subnet utilities.                                      |
| `error`      | Defines `EngineError`.                                                       |

## Runtime Notes

`compio` uses completion-based I/O. Buffers must be owned and passed by value to I/O operations (returning via `compio::BufResult`).

Hayate isolates blocking logic (`tar` extraction) and heavy CPU tasks (`zstd` / AEAD) off the executor using dedicated worker threads (`std::thread::spawn`) and `flume` channels. A `BTreeMap` handles chunk reordering on the receiver to guarantee sequential disk writes.

## Safety Guarantees

Hayate treats network data as fundamentally hostile:

- **Encrypted Metadata**: Filename, size, and hashing choices are authenticated before prompting the user.
- **Strict Framing**: Frames are length-capped and verified via AEAD before decompression.
- **Size Verification**: Completed file sizes must match the exact announced byte count.
- **Path Sanitization**: Directory unpacking rigorously prevents absolute paths, parent directory traversals (`..`), symlinks, and hard links.
