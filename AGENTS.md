# AGENTS.md

Hayate is an encrypted, compressed LAN file transfer tool: QUIC transport, application-layer
X25519 + AEAD crypto, mDNS/UDP peer discovery, 4-word code-phrase pairing. One Cargo workspace,
two crates:

- `hayate/` — library (the transfer engine), published to crates.io
- `hayate-cli/` — binary (`hayate`), `publish = false`, thin clap wrapper over the library

Both crates share one `[workspace.package] version` in the root `Cargo.toml` — bump it by hand
before a release (see Release below).

## Fast facts an agent can easily get wrong

- Async runtime is **compio** (io_uring/IOCP/kqueue) — **not tokio**. Don't reach for tokio
  types/macros; the CLI entrypoint builds its own `compio::runtime::Runtime` and `block_on`s it.
- MSRV **1.96**, edition **2024** (`rust-toolchain.toml` pins stable + this MSRV; the installed
  toolchain here already matches).
- `HayateSender`/`HayateReceiver` public methods (`runner.rs`) never take/return compio types —
  only `SocketAddr`/`String`/`Path`/`EngineError`. Callers still must execute inside a compio
  runtime (`#[compio::main]` or manual `Runtime::block_on`), since the futures await compio I/O
  internally.
- Tests are plain `#[test]` (sync), even where the code under test is async — this repo has no
  `#[compio::test]` usage. Tests live inline in `#[cfg(test)] mod tests` blocks; there are no
  `tests/` directories.
- `hayate/Cargo.toml` pins `rand_core = "0.6.4"` directly (see the comment there): `x25519-dalek`
  2.x needs the old `CryptoRng` trait from 0.6, while the workspace's `rand = "0.10"` pulls
  `rand_core` 0.10. Both versions coexist in `Cargo.lock` on purpose — don't try to unify them.

## Commands (run from workspace root; `just` orchestrates)

- `just fmt` / `just fmt-check` — `cargo fmt` (`rustfmt.toml`: `style_edition = "2024"`)
- `just clippy` — `cargo clippy --workspace --all-targets -- -D warnings` (also compiles
  `hayate/examples/*`)
- `just test` — `cargo test --workspace` — **not** `--all-targets`, so examples aren't
  compile-checked here; `just clippy` is what catches example breakage from API changes
- `just check` — fmt-check + clippy + test; run this before considering work done
- Single test: `cargo test -p hayate <name>` or `cargo test -p hayate-cli <name>`
  (add `-- --nocapture` for stdout)
- Doctests run automatically inside `cargo test --workspace` (several in `lib.rs`, `runner.rs`,
  `local_addr.rs`)
- Benchmarks (`hayate/benches/`) are feature-gated: `cargo bench --features benchmarks` — a bare
  `cargo bench` silently builds nothing (`required-features` skips every bench target)
- Extra aliases in `.cargo/config.toml`: `check-all`/`clippy-all`/`test-all`/`bench-all`
  (`--workspace --all-targets` shortcuts), `android-aarch64`/`android-x86_64`/`android-all`

## CI (`.github/workflows/ci.yml`) — independent jobs, all required

`fmt` · `clippy` (`-D warnings`) · `msrv` (`cargo check --workspace --all-targets` pinned to
**exactly 1.96**, not just `stable`) · `audit` (RustSec advisories via `rustsec/audit-check`) ·
`test` matrix (ubuntu/macos/windows/windows-11-arm, `cargo test --workspace --all-targets`).
`missing_docs = "warn"` in `Cargo.toml` is a plain rustc lint, but clippy's `-D warnings`
promotes it to a hard failure — every new public item needs a doc comment or CI's `clippy` job
fails.

## Architecture (`hayate/src/`)

| Module         | Role                                                                    |
| -------------- | ------------------------------------------------------------------------ |
| `runner.rs`    | Public `HayateSender`/`HayateReceiver` builders — the real entrypoints |
| `transfer.rs`  | Handshake state machine + chunked send/receive pipeline                |
| `protocol.rs`  | Wire format: version negotiation, `Metadata`, frame encoding           |
| `crypto.rs`    | X25519 ECDH, HKDF-SHA256, AEAD seal/open, cipher negotiation            |
| `network.rs`   | QUIC endpoint setup, ephemeral TLS certs (`rcgen`)                     |
| `discovery.rs` | mDNS + UDP-broadcast peer discovery                                     |
| `pool.rs`      | `BufferPool` (flume-backed) for hot-path buffer reuse                  |
| `tar.rs`       | Directory ⇄ tar streaming; extraction rejects abs paths/`..`/symlinks  |
| `local_addr.rs`| Interface/subnet detection (`if-addrs`)                                |

`hayate-cli/` has no protocol logic of its own — it's clap parsing → library builder calls →
`indicatif`/`console` progress UI.

### Protocol/threading notes (read before touching `transfer.rs` or `crypto.rs`)

- Handshake: QUIC connects with an ephemeral self-signed cert (trust-on-first-use) → app-level
  X25519 exchange → receiver picks cipher (AES-256-GCM if HW-accelerated, else
  ChaCha20-Poly1305) → encrypted `Metadata` → accept/reject byte.
- Payload: 4 MiB frames, 8-deep async read-ahead, AEAD-encrypted, zstd-compressed unless the
  file extension is a known pre-compressed format.
- AEAD/zstd work runs on dedicated `std::thread` workers connected via `flume` channels,
  deliberately kept off the compio event-loop threads — don't add blocking crypto/compression
  calls directly on the async path.
- Receiver reorders frames in a `BTreeMap` before writing, to guarantee sequential disk I/O.
- Discovery channel ID = `derive_channel_id(phrase)` = hex(SHA-256(phrase)[..4]). Tests pin the
  output to exactly 8 lowercase hex chars — a dropped leading zero would silently break pairing
  between mismatched versions.

## Conventions

- Builder pattern for public API (`HayateSender::new().code(..).compress(..).send(path)`)
- `EngineError` (thiserror) in the library; `anyhow::Result` in the CLI — don't cross the two
- Every source file starts with a `//!` module doc comment

## Release (see `RELEASE.md` for the full walkthrough)

release-plz and cargo-dist have been removed — a replacement pipeline is TBD. Releasing is
manual for now: bump `[workspace.package] version` by hand, update `CHANGELOG.md`, tag, push,
`cargo publish -p hayate`. `hayate-cli` stays `publish = false`. Never force-push/recreate a tag.

## Docs site

`docs/` is a separate pnpm/Rspress project, not part of the Cargo workspace — see
`docs/AGENTS.md` for its own commands and conventions.
