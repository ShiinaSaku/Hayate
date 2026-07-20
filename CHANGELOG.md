## hayate

### Passphrases work on direct transfers

`HayateSender::passphrase()` shipped with no way for the receiver to supply the
matching secret — every passphrase-protected direct transfer failed at metadata
decryption. `HayateReceiver::passphrase()` now mirrors the sender side, and the
secret threads through one-shot receives and multi-accept `ListeningReceiver`s.
Roundtrip and wrong-passphrase tests pin the behavior.

### Discovery can no longer deadlock

Peer scanning used a bounded channel between worker threads and the collector: a
busy network could fill it past the scan deadline, leaving a worker blocked in
`send` while the main thread waited forever in `join()`. The channel is now
unbounded (peer volume is rate-limited by the network anyway), read-timeout
failures propagate instead of silently disabling the receive deadline, and a
failed mDNS browse shuts its daemon down instead of leaking the thread.

### Unknown hash algorithms are rejected at the handshake

`Metadata::decode` documented that it validated the payload hash algorithm but
didn't — a peer offering an unknown algorithm passed the consent prompt and only
failed once payload frames arrived. `decode` now rejects unknown algorithms up
front, before anyone is asked to approve a transfer.

### CLI: honest timings, no ghost spinners

- Transfer summaries (and the JSON `speed_bps` field) no longer count the
  pairing wait or the time spent at the accept prompt — the clock starts when
  payload bytes actually flow.
- Error paths clear the progress bar and spinner before exiting, so failed
  transfers leave the terminal clean.
- A failed accept prompt (e.g. piped stdin) propagates as an error instead of
  silently reporting "rejected" and exiting 0.
- Cancelling a listen-mode receive exits 7 like pairing mode, and a closed
  endpoint stops the accept loop instead of respawning a spinner every 500 ms.
- `--compress=false` parses again, and `--no-compress` now conflicts with `-z`
  instead of combining silently.
- JSON output no longer reports `rtt_ms: null` for sub-millisecond peers, and
  the `ESC / q` hint stays out of the NDJSON stream.

### Release pipeline: verified, pinned, attested

- The npm publisher downloads `SHA256SUMS.txt` from each GitHub release and
  verifies every archive before repackaging — a corrupted or tampered artifact
  fails the release instead of shipping to npm.
- Archive extraction rejects absolute paths and `..` traversal entries before
  writing a single byte, and downloads are atomic (temp file + rename), so an
  interrupted run can't leave a truncated archive for a later run to trust.
- npm publishes carry `--provenance` attestation from GitHub Actions over OIDC
  trusted publishing, and the workflow pins bun, cargo-zigbuild, and cargo-ndk
  to exact versions.

### Staged transfer API and library polish

- Add staged hooks on `HayateSender` / `HayateReceiver` (`send_with`, `receive_with`, `ListeningReceiver`) so callers can observe connect, handshake, progress, and completion without owning the engine.
- Harden transfer, crypto, discovery, and protocol paths used by the public API.
- Author and project identity: Saku Shiina `<saku@shiina.xyz>`, site `https://hayate.shiina.xyz`.

### CLI: docs, exit codes, and install surface

- Replace man-page generation with `hayate docs` (terminal handbook + web docs pointer).
- Map `EngineError` to stable CLI exit codes for scripting.
- Ship shell completions install helpers; publish the binary crate to crates.io as `hayate-cli`.

### Release tooling

- Shared release target matrix for native binaries and npm platform packages.
- Tegami-driven versioning for both workspace crates; local npm publish without GitHub.

# Changelog

All notable changes to Hayate are documented in this file.

---

## [6.0.0] - 2026-06-30

### Removed

- `handshake_sender` and `handshake_receiver` (combined read+write stream handshake).
  Use `handshake_sender_split` / `handshake_receiver_split` instead. The split-stream
  variants match QUIC's unidirectional stream model and were already the only path
  used by the library and CLI internally. Combined-stream functions were thin
  duplicates and have been removed.
- `start_broadcaster` (UDP-only legacy broadcaster). Use `start_broadcaster_hybrid`
  which runs mDNS + UDP simultaneously. The UDP-only variant was marked legacy and
  never called internally.
- `listen_for_broadcast_udp` (legacy UDP-only listener). Use `listen_for_broadcast`
  which browses mDNS and listens on UDP concurrently.
- v1 UDP discovery packet format backward compatibility. All broadcasters have used
  the `HAYATE_PEER:v2:...` format since 5.0.0. The version-less parse path is removed.
- `crypto::features` submodule. The single function `is_aes_hw_accelerated()` now
  lives directly in `crypto`. Replace `crypto::features::is_aes_hw_accelerated()`
  with `crypto::is_aes_hw_accelerated()`.

### Changed

- `write_tar_sync` return type changed from `Result<u64, io::Error>` to
  `Result<(), io::Error>`. The `u64` was always `0` and never used by callers.
- Protocol validation error constructors inlined — the separate `#[cold]` functions
  that existed solely for benchmark attribution have been removed. Error construction
  is identical; stack traces and messages are unchanged.
- `SkipCertVerification::supported_verify_schemes()` advertises only ECDSA + Ed25519
  (dropped RSA-PKCS1-SHA256). All signatures are still accepted by the verifier;
  this only narrows the schemes the peer is offered during TLS negotiation.

### Added

- `HayateSender::build_metadata()`, `send_file()`, and `send_directory()` are now
  public. Callers who manage their own QUIC connection and handshake can use these
  to get metadata and stream payloads without duplicating the library's internal
  logic.

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
