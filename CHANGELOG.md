# Changelog

All notable changes to this project will be documented in this file.

## [4.0.0](https://github.com/ShiinaSaku/Hayate/compare/v3.0.0...v4.0.0) - 2026-06-27

### Other

- deep engine audit — hot-path optimisations, protocol safety, discovery overhaul

## [3.0.0](https://github.com/ShiinaSaku/Hayate/compare/v2.1.1...v3.0.0) - 2026-06-18

### Added

- multithreaded receiver and dynamic hashing
- add release-plz and codebase improvements

### Changed

- structure,improve speeds and prepare v3

### Other

- bump crate versions ([#9](https://github.com/ShiinaSaku/Hayate/pull/9))
- Add CodSpeed performance benchmarks and CI workflow ([#5](https://github.com/ShiinaSaku/Hayate/pull/5))

## [2.0.0] - 2026-06-02

### Other

- Create install.sh ([ece0bf7cdd8d8c0f5cb9da94778d23201e19efbe](https://github.com/ShiinaSaku/Hayate/commit/ece0bf7cdd8d8c0f5cb9da94778d23201e19efbe)) by 椎名 朔
- Add windows install script ([ab2c0764532ac7fd167cfb54ce8dfb055e619fc5](https://github.com/ShiinaSaku/Hayate/commit/ab2c0764532ac7fd167cfb54ce8dfb055e619fc5)) by 椎名 朔
- Update install.sh ([0c8d0eb9bbd51a3094f1736908bdbb848eaa2b30](https://github.com/ShiinaSaku/Hayate/commit/0c8d0eb9bbd51a3094f1736908bdbb848eaa2b30)) by 椎名 朔
- Patch script ([aac92c5f5eaef337b2110c12533e61abcb783baa](https://github.com/ShiinaSaku/Hayate/commit/aac92c5f5eaef337b2110c12533e61abcb783baa)) by 椎名 朔
- Patch ([4f6765a26ef8bb45977d3429de949e021a352521](https://github.com/ShiinaSaku/Hayate/commit/4f6765a26ef8bb45977d3429de949e021a352521)) by 椎名 朔
- Merge pull request #1 from ZG089/master

scripts: let curl fail on http 4xx-5xx errors ([b4380c9a3b947ebd1df14ec22634f66bf200285b](https://github.com/ShiinaSaku/Hayate/commit/b4380c9a3b947ebd1df14ec22634f66bf200285b)) by ZGX089

- Update ci.yml ([22a908fab1db6f46c3211d58d8e9e81fe4bbdbb7](https://github.com/ShiinaSaku/Hayate/commit/22a908fab1db6f46c3211d58d8e9e81fe4bbdbb7)) by 椎名 朔
- Create FUNDING.yml ([cf223ab3a7180d3ec473694703969077f7d1fcc6](https://github.com/ShiinaSaku/Hayate/commit/cf223ab3a7180d3ec473694703969077f7d1fcc6)) by 椎名 朔
- Rewrite hayate with rust (#3)

* refactor: remove Go implementation in preparation for Rust rewrite

* feat(rust): initialize compio-based rust cargo workspace

* feat(engine): define workspace core types and engine library entry

* feat(engine): implement x25519 ecdh key exchange and chacha20poly1305 aead encryption

* feat(engine): implement udp broadcast pairing and target channel filter

* feat(engine): implement file tar extraction, protocol framing, and quic configuration

* feat(engine): implement zero-copy async send and receive payload pipelines

* feat(cli): define command structure, output utilities, and random phrase generator

* feat(cli): implement concurrent subnet port discovery scanner

* feat(cli): implement async file send orchestrator and broadcaster

* feat(cli): implement async file receive orchestrator and auto-pairing client

* feat(ffi): build FFI library abstraction wrapper

* chore(scripts): update installation scripts for Rust binary target and help subcommand

* fix(engine): resolve pedantic and code-level clippy warnings

* Refactor: relocate Rust codebase to root, polish CLI banner & colors, fix Clippy, and configure Android Termux build

* Fix: Configure ALPN protocol 'hayate' for TLS handshake in server and client configurations

* Refactor: Add --no-tui as alias to --no-progress, and support --peer option in send command for backward compatibility

* Refactor: update ASCII art logo to new styling, and scale/color according to terminal screen size

* feat: format

* Create FUNDING.yml

* fix: remove hot-loop clones, graceful compression fallback, better panic info, update docs for Rust

* feat:Bump compio and refactor transfer pipeline

* feat: Add CHANGELOG and changelog generator

Add scripts/generate-changelog.sh to auto-generate CHANGELOG.md, add a
justfile 'changelog' target, and update README to reference the
changelog

---

Co-authored-by: copilot-swe-agent[bot] <198982749+Copilot@users.noreply.github.com> ([4636c1d9ecab6cacfece44d17e2c0c1bd3154e51](https://github.com/ShiinaSaku/Hayate/commit/4636c1d9ecab6cacfece44d17e2c0c1bd3154e51)) by 椎名 朔

- Readme to reflect new cli changes ([493870945be43b0b38f67bf2c5a6d13638cc1774](https://github.com/ShiinaSaku/Hayate/commit/493870945be43b0b38f67bf2c5a6d13638cc1774)) by Saku Shiina

### ✦ Bug Fixes

- Installation script ([86a06162224302349d9d873f968fc9c39fffcd07](https://github.com/ShiinaSaku/Hayate/commit/86a06162224302349d9d873f968fc9c39fffcd07)) by 椎名 朔
- _(CLI)_ List local IPs when bound to 0.0.0.0 ([e526d5b0487ae21c3413e3867fb178a149627538](https://github.com/ShiinaSaku/Hayate/commit/e526d5b0487ae21c3413e3867fb178a149627538)) by 椎名 朔
- Format ([1eedf4158999909ab413142616904675c833fa61](https://github.com/ShiinaSaku/Hayate/commit/1eedf4158999909ab413142616904675c833fa61)) by Saku Shiina

### ✦ Features

- Fix performance ,tui and add ci ([0dfae05d4a824316dbabe0f84c307a9f1de0de40](https://github.com/ShiinaSaku/Hayate/commit/0dfae05d4a824316dbabe0f84c307a9f1de0de40)) by Saku Shiina
- Update action versions ([2c0c6d0b610702f7b4a7e17009f8e0872b00df7f](https://github.com/ShiinaSaku/Hayate/commit/2c0c6d0b610702f7b4a7e17009f8e0872b00df7f)) by 椎名 朔
- Feat(cli) : Improve user experience and add alias ([a357a22e433b3583011767eb6fefff573b50d379](https://github.com/ShiinaSaku/Hayate/commit/a357a22e433b3583011767eb6fefff573b50d379)) by Saku Shiina
- _(engine)_ Implement dynamic cipher negotiation, buffer pool, and transport tuning ([0c920d8ac077798f34332806b48ba9b5d423b62a](https://github.com/ShiinaSaku/Hayate/commit/0c920d8ac077798f34332806b48ba9b5d423b62a)) by 椎名 朔
- _(cli)_ Enhance transfer UI, progress indicators, and stage logging ([52b411834d7a6841a861aad453bf229ddad565a6](https://github.com/ShiinaSaku/Hayate/commit/52b411834d7a6841a861aad453bf229ddad565a6)) by 椎名 朔

### ✦ Maintenance & CI/CD

- _(release)_ Configure git-cliff, SLSA attestations, and private package settings ([4767a7fc893d0bd396b9bbfe19b66aa1f68c0e84](https://github.com/ShiinaSaku/Hayate/commit/4767a7fc893d0bd396b9bbfe19b66aa1f68c0e84)) by 椎名 朔

### ✦ Refactoring & Code Quality

- _(ffi)_ Adapt imports for renamed engine crate and set publish = false ([237c7194cd034204cd1892b8c254963d889ae7ca](https://github.com/ShiinaSaku/Hayate/commit/237c7194cd034204cd1892b8c254963d889ae7ca)) by 椎名 朔

## [1.0.0] - 2026-05-29

### Other

- Intial commit ([0137bddedf4cf46fa4e1330ac60c4b30533dc1fe](https://github.com/ShiinaSaku/Hayate/commit/0137bddedf4cf46fa4e1330ac60c4b30533dc1fe)) by 椎名 朔
- Release v2.0.0 "Add compression mode and ASCII TUI

Switch CLI to pflag and add --compress (auto|always|never). Implement
compression heuristics (ShouldCompress, NormalizeCompressionMode) and
thread through compressMode into the transfer pipeline. Bump protocol/
tool Version to 2.0.0 and add unit tests for compression and filename
sanitization. Update TUI to use ASCII borders and include local address
info. Add isAndroid detection for QUIC PMTU handling. Add release
script,
update .gitignore, remove bundled binaries, and add termenv/pflag deps." ([984d5ccaca263155ab451f208b704670942f0cac](https://github.com/ShiinaSaku/Hayate/commit/984d5ccaca263155ab451f208b704670942f0cac)) by 椎名 朔

- Delete .DS_Store ([a640bccecc7741527da3b7482d795d0e59dda839](https://github.com/ShiinaSaku/Hayate/commit/a640bccecc7741527da3b7482d795d0e59dda839)) by 椎名 朔
<!-- generated by git-cliff -->
