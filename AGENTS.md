# AGENTS.md

Hayate is an encrypted, compressed LAN file transfer tool: QUIC transport, application-layer
X25519 + AEAD crypto, mDNS/UDP peer discovery, 4-word code-phrase pairing. One Cargo workspace,
two crates:

- `hayate/` — library (the transfer engine), published to crates.io
- `hayate-cli/` — binary (`hayate`), `publish = false`, thin clap wrapper over the library

Both crates share one `[workspace.package] version` in the root `Cargo.toml` — release-plz bumps
it in lockstep (see Release below). Never edit published `CHANGELOG.md` history by hand.

## Fast facts an agent can easily get wrong

- Async runtime is **compio** (io_uring/IOCP/kqueue) — **not tokio**. Don't reach for tokio
  types/macros; the CLI entrypoint builds its own `compio::runtime::Runtime` and `block_on`s it.
- MSRV **1.96**, edition **2024** (`rust-toolchain.toml` pins stable + this MSRV; the installed
  toolchain here already matches).
- `HayateSender`/`HayateReceiver` public methods (`runner.rs`) never take/return compio types —
  only `SocketAddr`/`String`/`Path`/`EngineError`. Callers still must execute inside a compio
  runtime (`#[compio::main]` or manual `Runtime::block_on`), since the futures await compio I/O
  internally.
- Semver-guaranteed API = `runner` + crate-root re-exports + `crypto`/`network`/`discovery`/
  `local_addr`/`protocol`. `transfer`, `tar`, `pool` are `pub` but `#[doc(hidden)]` — unstable,
  change freely; CI's `semver` job (cargo-semver-checks) guards the stable surface.
- Tests are plain `#[test]` (sync), even where the code under test is async — this repo has no
  `#[compio::test]` usage. Tests live inline in `#[cfg(test)] mod tests` blocks; there are no
  `tests/` directories.
- The engine uses `x25519-dalek` 3, which shares `rand_core` 0.10 with the workspace's
  `rand = "0.10"`. System randomness now comes from `getrandom::SysRng` (wrapped via
  `rand_core::UnwrapErr`) — `rand_core::OsRng` was removed in 0.10. No dual-version `rand_core`
  pin is needed; keep both crates on the latest 0.10 line.

## Commands (run from workspace root; `mise` orchestrates)

- `mise run fmt` / `mise run fmt-check` — `cargo +nightly fmt`. `rustfmt.toml` opts into
  nightly-only options (`unstable_features`, `imports_granularity`, `wrap_comments`, …), so
  formatting requires the nightly toolchain (`rustup component add rustfmt --toolchain nightly`);
  the CI `fmt` job installs nightly for this reason.
- `mise run clippy` — `cargo clippy --workspace --all-targets -- -D warnings` (also compiles
  `hayate/examples/*`)
- `mise run test` — `cargo test --workspace --all-targets`
- `mise run check` — fmt-check + clippy + test + dashboard type-check; run before considering
  work done
- `mise run web` — `bun install --frozen-lockfile && bun run build` inside `web/`; regenerates
  `web/build`, which is **embedded at compile time and committed** (see Dashboard below)
- `mise run web-dev` — Vite dev server for the dashboard
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
`semver` (cargo-semver-checks) · `test` matrix (ubuntu/macos/windows/windows-11-arm) ·
`web` (Bun install + `svelte-check` + build, then fails if `web/build` differs from the
committed bundle — regenerate with `mise run web` and commit it).
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
| `discovery.rs`  | mDNS announce/browse + UDP query/respond + pairing broadcast            |
| `pool.rs`       | `BufferPool` (flume-backed) for hot-path buffer reuse                  |
| `tar.rs`        | Directory ⇄ tar streaming; extraction rejects abs paths/`..`/symlinks  |
| `local_addr.rs` | Interface/subnet detection (`if-addrs`)                                |

`hayate-cli/` has minimal protocol logic of its own — mostly clap parsing → library builder
calls → `indicatif`/`console` progress UI — plus two local-only stores: `peers.rs`
(named peers JSON in the user config dir, backs `hayate peers` / `send --to`) and
`history.rs` (transfer log as JSONL in the user data dir, backs `hayate history`).
`subcmd/man.rs` renders the roff man page from the clap definitions (hidden `hayate man`).

## Dashboard (`hayate web`)

`web/` is a **SvelteKit + Tailwind + shadcn-svelte** SPA (Bun + Vite, Tabler icons — `components.json` sets `iconLibrary: "tabler"`, never import Lucide). `hayate-cli/src/web/` is
the Rust host: a hand-rolled HTTP/1.1 server on compio exposing `/api/{status,peers,history}`
and an SSE stream, plus static file serving from the embedded bundle.

- The UI is built into `web/build` and embedded with `include_dir!`; that directory is
  **committed** so `cargo build`/`test` need no JavaScript toolchain. After changing anything
  under `web/src`, run `mise run web` and commit the resulting `web/build` (CI enforces this).
- Transfers: `POST /api/send` spools the uploaded body to a temp directory and hands it to
  `HayateSender`; `POST /api/receive/{start,stop}` runs a listener on its own thread/runtime,
  auto-accepting into the configured folder (default `~/Downloads/Hayate`). Progress and history
  are recorded through the same modules as the CLI.
- Security model: loopback bind by default (`--allow-lan` overrides), per-launch token in a
  `HttpOnly; SameSite=Strict` cookie, `X-Hayate-Request: 1` on POSTs (CSRF), `Host` check, strict
  CSP, 16 GiB upload cap, basename-only filenames, and a 64-connection cap. `script-src
  'unsafe-inline'` is required by SvelteKit's inline bootstrap; the page renders no untrusted
  HTML.
- `web/src/routes/+layout.ts` sets `ssr = false`; the adapter is `@sveltejs/adapter-static`
  with `fallback: 'index.html'` (SPA mode).

## Conventions

- Builder pattern for public API (`HayateSender::new().code(..).compress(..).send(path)`)
- `EngineError` (thiserror) in the library; `anyhow::Result` in the CLI — don't cross the two
- Every source file starts with a `//!` module doc comment

## Release (see `RELEASE.md`)

Two community tools, one tag:

- **release-plz** (`.github/workflows/release-plz.yml`) opens the version/changelog PR on every
  push to `master`, then publishes `hayate` to crates.io via **OIDC trusted publishing**
  (`id-token: write`, no `CARGO_REGISTRY_TOKEN`) and pushes the `vX.Y.Z` tag. Configuration lives
  in `release-plz.toml` (`git_release_enable = false` — cargo-dist owns the GitHub Release).
- **cargo-dist** (`.github/workflows/release.yml`, generated — do not hand-edit) builds on the
  tag: archives, shell/PowerShell installers, an npm package, `SHA256SUMS`, and GitHub
  attestations. Config: `dist-workspace.toml`; the generated file is refreshed with
  `dist generate`.
- `hayate-cli` stays `publish = false`; only the library goes to crates.io. Its
  `[package.metadata.dist] dist = true` is what lets cargo-dist release the binary.
- Both crates share `[workspace.package] version`; release-plz bumps it in lockstep.
- Secrets: `NPM_TOKEN` (cargo-dist npm installer) and — for full automation — `RELEASE_PLZ_TOKEN`
  (App/PAT so the release-plz tag push triggers `release.yml`; GitHub does not start workflows
  for tags pushed with the default `GITHUB_TOKEN`). Without it, publish the crate then run
  `gh workflow run release.yml --ref vX.Y.Z`.
- Android/Termux builds are no longer in the release matrix (cargo-dist has no NDK cross
  target): build locally with `mise run android`.
- Never force-push or recreate a tag.
