# How to Release

Hayate releases are driven by [Tegami](https://tegami.fuma-nama.dev). Merging a Tegami version PR bumps the workspace version, publishes the `hayate` library to crates.io, creates a GitHub release (e.g. **Hayate v6.0.0 — Shinka (進化)** — codenames live in `scripts/tegami.mts`), and triggers a matrix workflow that builds and attaches binaries for Linux, macOS, Windows, and Android (Termux).

Both crates share one version and bump in lockstep: `scripts/tegami.mts` declares a `hayate` group with `syncBump` + `syncGitTag`, so one git tag (`hayate@<version>`) and one GitHub release cover the whole workspace.

---

## One-time setup: trusted publishing

Publishing is tokenless — no `CARGO_REGISTRY_TOKEN` or `NPM_TOKEN` secrets. Both registries authenticate via GitHub OIDC; configure this once:

### crates.io

1. Open the `hayate` crate on crates.io → **Settings → Trusted Publishing**.
2. Add a GitHub Actions publisher: owner `ShiinaSaku`, repo `Hayate`, workflow filename `publish.yml`, no environment.
3. The `Publish` workflow exchanges OIDC for a short-lived token via `rust-lang/crates-io-auth-action` — nothing else to do.

### npm

1. For **each** of the 9 packages (`@shiinasaku/hayate` plus the 8 platform packages), open npmjs.com → package **Settings → Publishing access → Trusted Publisher**.
2. Add GitHub Actions: org/user `ShiinaSaku`, repository `Hayate`, workflow filename `release-binaries.yml`, no environment.
3. The `npm` job installs Node 24 (which ships npm 11) and `npm publish --provenance` picks up OIDC automatically.

If a trusted-publisher exchange fails, the error message names the package and the missing config — fix the dashboard entry and re-run the workflow.

---

## Step by step

### 1. Write a changelog

Create a pending changelog file under `.tegami/` as `YYYY-MM-DD-{hash}.md`. The frontmatter must reference the `hayate` package (the library is the only published crate; `hayate-cli` is `publish = false`).

```md
---
packages:
  "hayate": patch
---

### Fix discovery on networks with link-local addresses

Pairing now works correctly when only link-local addresses are available.
```

See the [Tegami changelog format](https://tegami.fuma-nama.dev/changelog) for details.

### 2. Open the Tegami version PR

Tegami computes the new version from the changelogs and opens a PR. Review it, then merge.

### 3. Publish and release

Merging the version PR triggers the `Publish` workflow (`.github/workflows/publish.yml`). It:

- Publishes the `hayate` crate to crates.io via trusted publishing (OIDC, no stored token).
- Creates the GitHub release for `hayate@<version>` with the codename title from `scripts/tegami.mts`.

### 4. Binary release

The `release-binaries` workflow (`.github/workflows/release-binaries.yml`) runs on the release event and builds:

| Target | Archive | Extras |
| ------ | ------- | ------ |
| `x86_64-unknown-linux-gnu` | `.tar.gz` | `.deb` |
| `aarch64-unknown-linux-gnu` | `.tar.gz` | `.deb` |
| `x86_64-apple-darwin` | `.tar.gz` | completions |
| `aarch64-apple-darwin` | `.tar.gz` | completions |
| `x86_64-pc-windows-msvc` | `.zip` | completions |
| `aarch64-pc-windows-msvc` | `.zip` | completions |
| `aarch64-linux-android` | `.tar.gz` | completions |
| `x86_64-linux-android` | `.tar.gz` | completions |

A final job aggregates all uploaded artifacts into `SHA256SUMS.txt` and re-uploads it.

### 5. npm distribution

The `npm` job in the same workflow bundles the TypeScript wrapper with tsdown (Node 24 in CI; tsdown needs ≥ 22.18, output still targets Node 18), downloads the release archives plus `SHA256SUMS.txt`, verifies every archive's SHA-256, rejects unsafe archive paths, repackages the native binary for each platform into a scoped npm package, and publishes with `--provenance` attestation via trusted publishing (OIDC, no stored token):

- `@shiinasaku/hayate` — the main CLI wrapper that installs the correct native binary as an optional dependency.
- `@shiinasaku/hayate-darwin-x64` / `...arm64`
- `@shiinasaku/hayate-linux-x64` / `...arm64`
- `@shiinasaku/hayate-win32-x64` / `...arm64`
- `@shiinasaku/hayate-android-x64` / `...arm64`

After the release is published, users can install the CLI with:

```bash
npm install -g @shiinasaku/hayate
```


---

## Local release tooling

Cross-platform binaries are built with `bun run build.ts`:

- `bun run build` — native host target
- `bun run build:all` — every target this host can reach
- `bun run build:deb` — Linux targets plus `.deb` packages
- `bun run build:android` — include Android (Termux) targets
- `bun run build:everything` — all of the above

Linux cross-compiles use `cargo-zigbuild`. Android builds prefer `cargo-ndk`; otherwise they fall back to the NDK linker scripts in `.cargo/config.toml`.

Type-check the release scripts with `bun run typecheck`.

---

## What NOT to do

| Don't                          | Why                                                          |
| ------------------------------ | ------------------------------------------------------------- |
| Force-push or recreate a tag   | Tags should be append-only.                                   |
| Publish `hayate-cli`           | It's `publish = false` on purpose; only the library ships.  |
| Hand-edit `Cargo.lock` version | Run `cargo update -p hayate -p hayate-cli` after bumping.    |
| Edit `.tegami/publish-lock.yaml` or package changelogs | Tegami owns these files.                                    |
| Re-add registry token secrets  | Publishing is OIDC trusted publishing on both registries.    |

---

## Version numbers

```toml
[workspace.package]
version = "6.0.0"
```

| Crate              | Published?             | Version source              |
| ------------------ | ---------------------- | ---------------------------- |
| `hayate` (lib)     | crates.io              | `workspace.package.version`  |
| `hayate-cli` (bin) | No (`publish = false`) | `workspace.package.version`  |
