# Changelog

All notable changes to Hayate are documented in this file.

---

## [5.1.1](https://github.com/ShiinaSaku/Hayate/compare/v5.1.0...v5.1.1) - 2026-06-28

### Other

- rename release workflows and add CLAUDE.md project guide

## [5.1.0](https://github.com/ShiinaSaku/Hayate/compare/v5.0.0...v5.1.0) - 2026-06-27

### Added

- **winget package manifests** for Windows Package Manager (`winget install ShiinaSaku.Hayate`)
- **cargo-dist integration** — release artifacts (8 platform binaries, shell/powershell installers, updaters, checksums) now built automatically by `release.yml`

### Changed

- Release pipeline: custom `release-assets.yml` replaced by dist-generated `release.yml`. Tag pushes trigger dist build matrix.
- README rewritten — comparison table, library example, updated flags, winget instructions

### Fixed

- `BroadcasterGuard` cancel channel bug — UDP broadcast now properly stops on guard drop
- Release-plz no longer calls a separate build job; tag push autonomously triggers dist
- CI: pnpm version, rustsec permissions, ghost-tracked public files

### API

- `#[non_exhaustive]` on `Metadata`, `DiscoveredPeer`, `PayloadSource`, `PayloadSink`
- `BroadcasterGuard::new()` made `pub(crate)` — use `start_broadcaster_hybrid()`
- `Metadata::new()` constructor added
- `Metadata` and `DiscoveredPeer` re-exported at crate root
- `Debug` impl added for `BroadcasterGuard`
- `EngineError::TimedOut` and `EngineError::Cancelled` variants added

## [5.0.0](https://github.com/ShiinaSaku/Hayate/compare/v4.0.0...v5.0.0) - 2026-06-27

### Added

- **mDNS + UDP hybrid discovery** — peers found via RFC 6762 mDNS (`_hayate._udp.local.`) with automatic UDP broadcast fallback. Works on Android, iOS, macOS, Linux, Windows without admin privileges.
- **Hybrid broadcaster** (`start_broadcaster_hybrid`) — registers mDNS service with TXT records (channel ID, OS, port) simultaneously with UDP broadcast.
- **Cross-platform terminal rendering** — Unicode box-drawing glyphs fall back to ASCII on Windows consoles and non-TTY. Progress bars auto-hide when stdout is piped.
- `mdns-sd` 0.20 dependency for cross-platform mDNS discovery.

### Changed

- **Discovery timeouts**: receiver pairing 30s → 60s; `discover` scan 3s → 15s.
- **QUIC idle timeout**: 60s → 300s to prevent spurious disconnects on large files.
- **Keep-alive interval**: 5s → 3s for faster dead-peer detection.
- **UDP broadcast interval**: 1s → 800ms.
- **Progress bar UX**: premium spinner + gradient bar + right-aligned byte counts + colored speed tiers + compact ETA.

### Fixed

- **Sender hang after transfer**: `recv_stream.read()` time-bounded at 10s.
- **Receiver shutdown**: `send_stream.finish()` + 200ms grace before `conn.close()`.
- **Ctrl+C exit**: shared `Arc<AtomicBool>` cancellation flag, 1.5s grace period.
- **Windows socket buffers**: tuned to 8MB on IOCP to avoid non-paged pool exhaustion.
- **clippy**: zero warnings across the workspace.

### Removed

- Legacy UDP-only discovery code (replaced by mDNS + UDP hybrid).
- `cliff.toml` (conflicted with release-plz).

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
