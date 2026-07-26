## hayate-cli@6.2.0

### Resumable transfers, rate limiting, and a richer CLI

- **Resume interrupted transfers**: `hayate receive --resume` continues a partial
  single-file transfer from the last complete 4 MiB frame instead of restarting.
  An already-complete file is hash-verified without re-sending payload. (Wire
  protocol v7: the receiver now sends an 8-byte resume offset after accepting;
  v6 and v7 peers refuse to pair rather than mis-transfer.)
- **Bandwidth cap**: `hayate send --bandwidth-limit 10MiB` throttles sustained
  send throughput (also available as `HayateSender::bandwidth_limit`).
- **Named peers**: `hayate peers add/list/remove` saves receiver addresses;
  `hayate send <path> --to NAME` dials by name. Direct sends auto-remember the
  peer by IP.
- **Interactive send**: omit the path for a file prompt, or use `--pick` to scan
  the LAN and choose a receiver from a list.
- **Transfer history**: every completed transfer is logged locally;
  `hayate history` prints it (`--clear` to wipe, `--format json` for JSONL).
- **Integrity report**: receivers print an explicit "integrity verified" line
  and emit a matching JSON event after each transfer.
- **`receive --once`**: exit after the first completed transfer. Without it,
  direct-mode receive now keeps listening for more transfers.
- **JSON schema tag**: every `--format json` event now carries
  `"schema": "hayate/1"` for version-locked parsing.
- **Man pages**: release archives and `.deb` packages now ship a `hayate(1)`
  man page generated from the CLI definitions (`man hayate`).

### Fixes

- Windows binary builds no longer fail with `tar: Cannot connect to D:` —
  GNU tar on Git for Windows treated the drive-letter colon as a remote host;
  the build script now passes `--force-local`.
- The publish workflow triggers release binaries via the GitHub REST API
  instead of the `gh` CLI.

### Polish and hardening

- **API stability**: the semver-guaranteed surface is now explicitly `runner`
  + crate-root re-exports; `transfer`/`tar`/`pool` internals are
  `#[doc(hidden)]` and unstable. CI runs `cargo-semver-checks`.
- **Version tolerance**: v7 receivers gracefully accept v6 senders (resume is
  simply disabled); truly ancient/future versions fail fast with
  `ProtocolMismatch` instead of hanging.
- **UI correctness**: all progress bars/spinners are owned by a single
  `TransferUi` — no more leaked bars on error paths, and resumed transfers
  seed the bar at the resume offset so speed/ETA math is honest. Bar creation
  is gated uniformly by the output policy (JSON/plain/quiet never draw bars).
  The ESC/q listener now backs off while interactive prompts own the terminal.
- **Refactor**: send/receive share stage handling, consent/progress closures,
  success reporting, and a single `PathCompleter`; history recording is one
  `record_transfer` call instead of triplicated struct literals.
- **Tests**: 84 total — golden wire-format fixtures, protocol version gate
  (ancient + future), resume edge matrix (sub-frame partial, verify-only
  complete file), and assert_cmd-driven CLI integration tests (peers, history,
  docs, man, error paths) with isolated HOME.

## hayate-cli@6.0.0

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
