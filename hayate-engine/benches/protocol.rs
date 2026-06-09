//! CodSpeed benchmarks for Hayate's wire protocol metadata codec.
//!
//! Metadata is encoded and decoded once per transfer during the encrypted
//! handshake. The codec is small but on the critical path before any payload
//! bytes flow, so regressions here delay every transfer's start.

use divan::Bencher;
use hayate::protocol::{Metadata, TRANSFER_FILE};

fn main() {
    divan::main();
}

fn sample_metadata() -> Metadata {
    Metadata {
        filename: "2026-projects-archive.tar.zst".to_owned(),
        total_size: 1 << 30,
        transfer_type: TRANSFER_FILE,
    }
}

#[divan::bench]
fn encode(bencher: Bencher) {
    let meta = sample_metadata();
    bencher.bench(|| divan::black_box(divan::black_box(&meta).encode()));
}

#[divan::bench]
fn decode(bencher: Bencher) {
    let encoded = sample_metadata().encode();
    bencher.bench(|| divan::black_box(Metadata::decode(divan::black_box(&encoded)).unwrap()));
}

#[divan::bench]
fn validate(bencher: Bencher) {
    let meta = sample_metadata();
    bencher.bench(|| divan::black_box(divan::black_box(&meta).validate()));
}
