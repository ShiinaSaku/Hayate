# Releasing Hayate

Releases use two community tools and one tag:

| Tool | Owns |
| --- | --- |
| [release-plz](https://release-plz.dev) | version PR, `CHANGELOG.md`, crates.io publish, `vX.Y.Z` tag |
| [cargo-dist](https://axodotdev.github.io/cargo-dist) | archives, shell/PowerShell installers, npm package, `SHA256SUMS`, GitHub Release, attestations |

## Flow

1. Land conventional-commit PRs on `master`.
2. The **Release PR** job opens/updates `chore: release` with the version bump and changelog.
3. Merge it. The **Publish** job then:
   - authenticates to crates.io with OIDC trusted publishing (no stored token),
   - publishes `hayate`, and pushes `vX.Y.Z`.
4. The tag starts the **Release** workflow (cargo-dist), which builds every target, publishes the
   npm installer, and creates the GitHub Release with attestations.

## One-time setup

- crates.io → `hayate` → Settings → Trusted Publishing: repository `ShiinaSaku/Hayate`, workflow
  `release-plz.yml` (no environment).
- GitHub secret `NPM_TOKEN` for the `@shiinasaku/hayate` npm package.
- GitHub secret `RELEASE_PLZ_TOKEN` (GitHub App or PAT with `contents: write`) so the tag pushed
  by release-plz triggers `release.yml`. Without it the crate is still published and tagged, but
  run `gh workflow run release.yml --ref vX.Y.Z` to build the binaries.

## Local checks

```bash
mise run check     # fmt, clippy, tests, dashboard type-check
mise run web       # rebuild web/build if web/ changed, then commit it
dist plan          # dry-run the release plan
```

## Notes

- `hayate-cli` is `publish = false`; only the library is on crates.io. cargo-dist releases it
  because of `[package.metadata.dist] dist = true`.
- Android/Termux binaries are not in the release matrix; build locally with `mise run android`.
- Never force-push or recreate a tag.
- The generated `.github/workflows/release.yml` must be updated with `dist generate`, not by hand.
