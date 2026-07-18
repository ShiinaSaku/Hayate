## hayate@6.0.0

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
