# How to Release

Hayate no longer uses release-plz or cargo-dist. Both were removed — a replacement release
pipeline is TBD. Until then, releasing is manual.

---

## Step by step

### 1. Bump the version

Both crates share one workspace version. Edit it by hand:

```toml
# Cargo.toml
[workspace.package]
version = "6.0.0"
```

`hayate` and `hayate-cli` both use `version.workspace = true`, so they stay in lockstep
automatically — one edit is enough.

### 2. Update the changelog

Add a dated entry to `CHANGELOG.md` under `## [X.Y.Z] - YYYY-MM-DD`, grouped by
`Added` / `Changed` / `Fixed` / `Security` as needed.

### 3. Commit, tag, push

```bash
git commit -am "chore: release vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin master --follow-tags
```

### 4. Publish the library to crates.io

Only `hayate` (the library) is published; `hayate-cli` has `publish = false`.

```bash
cargo publish -p hayate
```

### 5. Build and attach binaries (optional)

No automated multi-platform build exists right now. To ship a binary for this machine's
platform:

```bash
cargo build --release -p hayate-cli
```

The binary is at `target/release/hayate` (`hayate.exe` on Windows). Attach it manually to a
GitHub Release if you want to distribute it, or skip this until the new release tooling is in
place.

---

## What NOT to do

| Don't                          | Why                                                          |
| ------------------------------ | ------------------------------------------------------------- |
| Force-push or recreate a tag   | Same rule as before — tags should be append-only              |
| Publish `hayate-cli`           | It's `publish = false` on purpose; only the library ships     |
| Hand-edit `Cargo.lock` version | Run `cargo update -p hayate -p hayate-cli` after bumping instead |

---

## Version numbers

```toml
[workspace.package]
version = "6.0.0"
```

| Crate              | Published?             | Version source              |
| ------------------ | ---------------------- | ---------------------------- |
| `hayate` (lib)     | crates.io               | `workspace.package.version`  |
| `hayate-cli` (bin) | No (`publish = false`) | `workspace.package.version`  |
