//! Benchmarks for hash algorithm throughput comparison.
//!
//! Hayate uses blake3 and SHA-256 for transfer integrity. This file
//! benchmarks both at representative payload sizes to track throughput and
//! inform algorithm selection.
//!
//! SHA-256 is measured via `ring::digest`, which is already a production
//! dependency — no extra `sha2` crate required.

use divan::Bencher;

fn main() {
    divan::main();
}

/// Payload sizes from a small fragment up to a large multi-MiB block.
const SIZES: &[usize] = &[1024, 64 * 1024, 1024 * 1024, 16 * 1024 * 1024];

#[divan::bench(args = SIZES)]
fn hash_blake3(bencher: Bencher, size: usize) {
    let data = vec![0u8; size];
    bencher.bench(|| divan::black_box(blake3::hash(divan::black_box(&data))));
}

#[divan::bench(args = SIZES)]
fn hash_sha256(bencher: Bencher, size: usize) {
    let data = vec![0u8; size];
    bencher.bench(|| {
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        ctx.update(divan::black_box(&data));
        divan::black_box(ctx.finish())
    });
}
