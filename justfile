set shell := ["sh", "-cu"]

default:
    @just --list

fmt:
    cargo fmt

fmt-check:
    cargo fmt -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

check: fmt-check clippy test

build target="hayate":
    cargo build --release -p hayate-cli

run *args:
    cargo run -p hayate-cli -- {{args}}

receive port="50001" output=".":
    cargo run -p hayate-cli -- receive --port "{{port}}" --output "{{output}}"

send file peer:
    cargo run -p hayate-cli -- send "{{file}}" --peer "{{peer}}"

discover timeout="5":
    cargo run -p hayate-cli -- discover --timeout "{{timeout}}"

clean:
    cargo clean

changelog:
    ./scripts/generate-changelog.sh
