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
- The engine uses `x25519-dalek` 3, which shares `rand_core` 0.10 with the workspace's
  `rand = "0.10"`. System randomness now comes from `getrandom::SysRng` (wrapped via
  `rand_core::UnwrapErr`) — `rand_core::OsRng` was removed in 0.10. No dual-version `rand_core`
  pin is needed; keep both crates on the latest 0.10 line.

## Commands (run from workspace root; `just` orchestrates)

- `just fmt` / `just fmt-check` — `cargo +nightly fmt`. `rustfmt.toml` opts into nightly-only
  options (`unstable_features`, `imports_granularity`, `wrap_comments`, …), so formatting
  requires the nightly toolchain (`rustup component add rustfmt --toolchain nightly`); the CI
  `fmt` job installs nightly for this reason.
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

| Module          | Role                                                                   |
| --------------- | ---------------------------------------------------------------------- |
| `runner.rs`     | Public `HayateSender`/`HayateReceiver` builders — the real entrypoints |
| `transfer.rs`   | Handshake state machine + chunked send/receive pipeline                |
| `protocol.rs`   | Wire format: version negotiation, `Metadata`, frame encoding           |
| `crypto.rs`     | X25519 ECDH, HKDF-SHA256, AEAD seal/open, cipher negotiation           |
| `network.rs`    | QUIC endpoint setup, ephemeral TLS certs (`rcgen`)                     |
| `discovery.rs`  | mDNS + UDP-broadcast peer discovery                                    |
| `pool.rs`       | `BufferPool` (flume-backed) for hot-path buffer reuse                  |
| `tar.rs`        | Directory ⇄ tar streaming; extraction rejects abs paths/`..`/symlinks  |
| `local_addr.rs` | Interface/subnet detection (`if-addrs`)                                |

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

Tegami drives versioning and publishing. Binary releases are built and uploaded automatically by GitHub Actions.

- Write a pending changelog under `.tegami/` (see the Release workflow section below).
- Merge the Tegami version PR to bump versions and publish the `hayate` crate to crates.io.
- The `publish` workflow creates a GitHub release (titled `Hayate vX.Y.Z — <codename>`; codenames per major version live in `scripts/tegami.mts`); the `release-binaries` workflow then builds archives (and `.deb` packages) for Linux, macOS, Windows, and Android, and uploads them to the release.
- `hayate-cli` stays `publish = false`; only the library is published to crates.io.
- Both crates bump in lockstep: `scripts/tegami.mts` groups them with `syncBump` + `syncGitTag` (one version, one tag, one release). `patches/tegami@*.patch` (bun `patchedDependencies`) fixes tegami's cargo plugin double-bumping the shared `[workspace.package]` version — do not delete it, and re-apply when bumping tegami until fixed upstream.
- Publishing is **tokenless** on both registries (OIDC trusted publishing): crates.io via `rust-lang/crates-io-auth-action` in `publish.yml`, npm via `--provenance` + `id-token: write` in `release-binaries.yml`. No `CARGO_REGISTRY_TOKEN` / `NPM_TOKEN` secrets — the one-time dashboard config per registry is documented in `RELEASE.md`.
- Never force-push or recreate a tag.

## Binary builds

Cross-platform binaries are built with `bun run build.ts`:

- `bun run build` — native host target
- `bun run build:all` — every target this host can reach
- `bun run build:deb` — Linux targets plus `.deb` packages (requires `dpkg-deb` or GNU `ar`)
- `bun run build:android` — include Android (Termux) targets (requires `cargo-ndk` or the Android NDK)
- `bun run build:everything` — all of the above

Linux cross-compiles use `cargo-zigbuild`. Android uses `cargo-ndk` when available, otherwise falls back to the NDK linker scripts in `.cargo/config.toml`.

## npm distribution

The release pipeline also publishes `@shiinasaku/hayate` to npm. The package is a small ESM wrapper that downloads the correct native binary as an optional dependency.

```bash
npm install -g @shiinasaku/hayate
hayate --help
```

Platform packages:

- `@shiinasaku/hayate-darwin-x64` / `@shiinasaku/hayate-darwin-arm64`
- `@shiinasaku/hayate-linux-x64` / `@shiinasaku/hayate-linux-arm64`
- `@shiinasaku/hayate-win32-x64` / `@shiinasaku/hayate-win32-arm64`
- `@shiinasaku/hayate-android-x64` / `@shiinasaku/hayate-android-arm64`

The npm release script is `bun run npm:release`. The wrapper (`npm/hayate/`) is TypeScript
under `src/`, bundled with **tsdown** (`bun run npm:build`; requires Node ≥ 22.18 at build
time, output targets Node 18) into `dist/` — the published layout stays `index.js` +
`index.d.ts` + `bin/hayate.js`. The script downloads the GitHub release archives plus
`SHA256SUMS.txt`, verifies every archive's SHA-256 before repackaging (and rejects absolute /
`..` paths inside archives), then publishes the scoped packages — with `--provenance`
attestation in GitHub Actions, authenticated via OIDC trusted publishing (no token; the job
has `id-token: write` and installs Node 24 for tsdown + npm 11 — see the `npm` job in
`release-binaries.yml`).

## TypeScript release scripts

`build.ts`, `scripts/tegami.mts`, and the npm wrapper sources (`npm/hayate/src/`) are checked
with `bun run typecheck` (which runs `bun tsc --noEmit`); `npm/hayate/` has its own
`tsconfig.json` for the tsdown build.

# Release workflow

This repository uses [Tegami](https://tegami.fuma-nama.dev) for versioning and publishing.

## Write changelog files

Create pending changelog files under `.tegami/` as `YYYY-MM-DD-{hash}.md`.

See the [changelog format docs](https://tegami.fuma-nama.dev/changelog) for details.

### Example

```md
---
packages:
  "npm:@acme/ui": patch
---

### Fix button hover state

The hover color now matches the design system.
```

### Package references

Use package names, ids, or groups in frontmatter. For example:

- `"@acme/ui"` — package name
- `"npm:@acme/ui"` — package id
- `"group:acme"` — every package in a group

Rules:

- Include YAML frontmatter with `packages`
- Include at least one `#`, `##`, or `###` heading in the body
- Write user-facing release notes under each heading
- Do not edit the publish lock file (`.tegami/publish-lock.yaml`) or package `CHANGELOG.md` files directly
