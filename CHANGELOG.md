# Changelog

All notable changes to Hayate are documented in this file.

---

## [5.1.0](https://github.com/ShiinaSaku/Hayate/compare/v5.0.0...v5.1.0) - 2026-06-27

### Added

- format

## [5.0.0](https://github.com/ShiinaSaku/Hayate/compare/v4.0.0...v5.0.0) - 2026-06-27

### Added

- mDNS discovery, stability fixes, docs overhaul, CI upgrade

## [4.0.0] — 2026-06

### Added
- **mDNS + UDP hybrid discovery** — peers are found via RFC 6762 mDNS (`_hayate._udp.local.`) with automatic UDP broadcast fallback. mDNS works on Android, iOS, macOS, Linux, and Windows without admin privileges.
- **Hybrid broadcaster** (`start_broadcaster_hybrid`) — sender registers an mDNS service with TXT records (channel ID, OS, port) and simultaneously broadcasts UDP packets.
- **Cross-platform terminal rendering** — Unicode box-drawing glyphs fall back to ASCII on Windows consoles and non-TTY environments. Progress bars auto-hide when stdout is piped.

### Changed
- **Discovery timeout** — receiver pairing extended from 30s to 60s; `discover` scan default from 3s to 15s.
- **QUIC idle timeout** — raised from 60s to 300s to prevent spurious disconnects on slow links with very large files.
- **Keep-alive interval** — reduced from 5s to 3s for faster dead-peer detection.
- **UDP broadcast interval** — reduced from 1s to 800ms.
- **Progress bar UX** — premium styling with spinner animation, right-aligned byte counts, colored speed tiers, compact ETA.

### Fixed
- **Sender hang after transfer** — final `recv_stream.read()` now time-bounded at 10s, preventing indefinite stall when the receiver disappears.
- **Receiver shutdown** — `send_stream.finish()` + 200ms grace period before `conn.close()` ensures the sender sees the stream close before the transport tears down.
- **Ctrl+C exit** — replaced bare `process::exit` with a shared `Arc<AtomicBool>` cancellation flag propagated to all subcommands; 1.5s grace period for cleanup before force exit.
- **Windows socket buffers** — tuned to 8MB on IOCP to avoid non-paged pool exhaustion.

### Removed
- Legacy UDP-only discovery code; replaced by mDNS + UDP hybrid.

---

## [3.0.0] — 2026-05

### Changed
- Upgraded wire protocol to v5 with frame-level integrity checks.
- Directory extraction rejects symlinks, hard links, and path traversal.
- Improved metadata validation in handshake (`validate` before encode).

### Fixed
- Buffer pool memory leak when channel sender disconnects mid-transfer.
- Transfer size mismatch detection on directory streams.

---

## [2.0.0] — 2026-04

### Added
- `discover` subcommand with 128-concurrent QUIC probes and real-time RTT measurement.
- Pairing-code mode for sender/receiver LAN discovery without manual IP entry.
- Zstd compression toggle (`-z`/`--compress`).
- Hash algorithm selection (`--hash blake3|rapidhash|sha256`).

### Changed
- Migrated runtime from tokio to compio (io_uring/IOCP/kqueue).
- Transport from raw TCP+TLS to QUIC over compio-quic.
- Buffer pool from `crossbeam` channels to `flume` for async compatibility.

---

## [1.0.0] — 2026-03

### Added
- Initial release with encrypted LAN file transfer.
- X25519 + HKDF-SHA256 key exchange.
- ChaCha20-Poly1305 and AES-256-GCM AEAD.
- Tar-based directory transfer with path-traversal protection.
- Direct IP transfer mode.
- Progress bar with speed and ETA display.
