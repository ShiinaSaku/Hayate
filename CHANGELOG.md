# Changelog

All notable changes to this project will be documented in this file.

## [v2.0.0] - 2026-06-01

### ✦ Features
- feat: format ([`4949639`](https://github.com/ShiinaSaku/Hayate/commit/4949639))
- feat(ffi): build FFI library abstraction wrapper ([`d06d081`](https://github.com/ShiinaSaku/Hayate/commit/d06d081))
- feat(cli): implement async file receive orchestrator and auto-pairing client ([`ad6f9b7`](https://github.com/ShiinaSaku/Hayate/commit/ad6f9b7))
- feat(cli): implement async file send orchestrator and broadcaster ([`0e1ed5c`](https://github.com/ShiinaSaku/Hayate/commit/0e1ed5c))
- feat(cli): implement concurrent subnet port discovery scanner ([`697d4a8`](https://github.com/ShiinaSaku/Hayate/commit/697d4a8))
- feat(cli): define command structure, output utilities, and random phrase generator ([`6bf5c34`](https://github.com/ShiinaSaku/Hayate/commit/6bf5c34))
- feat(engine): implement zero-copy async send and receive payload pipelines ([`f7b1398`](https://github.com/ShiinaSaku/Hayate/commit/f7b1398))
- feat(engine): implement file tar extraction, protocol framing, and quic configuration ([`bf98018`](https://github.com/ShiinaSaku/Hayate/commit/bf98018))
- feat(engine): implement udp broadcast pairing and target channel filter ([`350b5c3`](https://github.com/ShiinaSaku/Hayate/commit/350b5c3))
- feat(engine): implement x25519 ecdh key exchange and chacha20poly1305 aead encryption ([`385190e`](https://github.com/ShiinaSaku/Hayate/commit/385190e))
- feat(engine): define workspace core types and engine library entry ([`d5b5b44`](https://github.com/ShiinaSaku/Hayate/commit/d5b5b44))
- feat(rust): initialize compio-based rust cargo workspace ([`326621e`](https://github.com/ShiinaSaku/Hayate/commit/326621e))
- feat: update action versions ([`2c0c6d0`](https://github.com/ShiinaSaku/Hayate/commit/2c0c6d0))
- feat: fix performance ,tui and add ci ([`0dfae05`](https://github.com/ShiinaSaku/Hayate/commit/0dfae05))

### ✦ Bug Fixes
- fix: remove hot-loop clones, graceful compression fallback, better panic info, update docs for Rust ([`94bc196`](https://github.com/ShiinaSaku/Hayate/commit/94bc196))
- Fix: Configure ALPN protocol 'hayate' for TLS handshake in server and client configurations ([`14fdc66`](https://github.com/ShiinaSaku/Hayate/commit/14fdc66))
- fix(engine): resolve pedantic and code-level clippy warnings ([`59ff150`](https://github.com/ShiinaSaku/Hayate/commit/59ff150))
- fix: installation script ([`86a0616`](https://github.com/ShiinaSaku/Hayate/commit/86a0616))

### ✦ Refactoring & Code Quality
- Refactor: update ASCII art logo to new styling, and scale/color according to terminal screen size ([`5ce6e6a`](https://github.com/ShiinaSaku/Hayate/commit/5ce6e6a))
- Refactor: Add --no-tui as alias to --no-progress, and support --peer option in send command for backward compatibility ([`324d883`](https://github.com/ShiinaSaku/Hayate/commit/324d883))
- Refactor: relocate Rust codebase to root, polish CLI banner & colors, fix Clippy, and configure Android Termux build ([`b12da1d`](https://github.com/ShiinaSaku/Hayate/commit/b12da1d))
- refactor: remove Go implementation in preparation for Rust rewrite ([`377af8a`](https://github.com/ShiinaSaku/Hayate/commit/377af8a))

### ✦ Maintenance & Chore
- chore(scripts): update installation scripts for Rust binary target and help subcommand ([`5f9127e`](https://github.com/ShiinaSaku/Hayate/commit/5f9127e))

### ✦ Miscellaneous
- Create FUNDING.yml ([`91ca918`](https://github.com/ShiinaSaku/Hayate/commit/91ca918))
- Update ci.yml ([`22a908f`](https://github.com/ShiinaSaku/Hayate/commit/22a908f))
- Merge pull request #1 from ZG089/master ([`b4380c9`](https://github.com/ShiinaSaku/Hayate/commit/b4380c9))
- Patch ([`4f6765a`](https://github.com/ShiinaSaku/Hayate/commit/4f6765a))
- Patch script ([`aac92c5`](https://github.com/ShiinaSaku/Hayate/commit/aac92c5))
- Update install.sh ([`0c8d0eb`](https://github.com/ShiinaSaku/Hayate/commit/0c8d0eb))
- Add windows install script ([`ab2c076`](https://github.com/ShiinaSaku/Hayate/commit/ab2c076))
- Create install.sh ([`ece0bf7`](https://github.com/ShiinaSaku/Hayate/commit/ece0bf7))


## [v1.0.0] - 2026-05-29

### ✦ Miscellaneous
- Delete .DS_Store ([`a640bcc`](https://github.com/ShiinaSaku/Hayate/commit/a640bcc))
- release v1.0.0 "Add compression mode and ASCII TUI ([`984d5cc`](https://github.com/ShiinaSaku/Hayate/commit/984d5cc))
- intial commit ([`0137bdd`](https://github.com/ShiinaSaku/Hayate/commit/0137bdd))
