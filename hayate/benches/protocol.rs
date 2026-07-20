//! Benchmarks for Hayate's wire protocol metadata codec.
//!
//! Metadata is encoded and decoded once per transfer during the encrypted
//! handshake. The codec is small but on the critical path before any payload
//! bytes flow, so regressions here delay every transfer's start.

use divan::Bencher;
use hayate::protocol::{Metadata, TransferKind};

fn main() {
    divan::main();
}

fn sample_metadata() -> Metadata {
    Metadata::new(
        "2026-projects-archive.tar.zst".to_owned(),
        1 << 30,
        TransferKind::File,
        "blake3".to_owned(),
    )
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

#[divan::bench]
fn encode_decode_roundtrip(bencher: Bencher) {
    let meta = sample_metadata();
    bencher.bench(|| {
        let encoded = divan::black_box(&meta).encode();
        divan::black_box(Metadata::decode(&encoded).unwrap())
    });
}

#[divan::bench]
fn validate_long_filename(bencher: Bencher) {
    let meta = Metadata::new("a".repeat(4000), 0, TransferKind::File, "blake3".to_owned());
    bencher.bench(|| divan::black_box(divan::black_box(&meta).validate()));
}

#[divan::bench]
fn encode_directory_metadata(bencher: Bencher) {
    let meta = Metadata::new(
        "my-project-backup".to_owned(),
        0,
        TransferKind::Directory,
        "blake3".to_owned(),
    );
    bencher.bench(|| divan::black_box(divan::black_box(&meta).encode()));
}

#[divan::bench]
fn decode_large_payload(bencher: Bencher) {
    let meta = Metadata::new("x".repeat(200), 1 << 40, TransferKind::File, "blake3".to_owned());
    let encoded = meta.encode();
    bencher.bench(|| divan::black_box(Metadata::decode(divan::black_box(&encoded)).unwrap()));
}
