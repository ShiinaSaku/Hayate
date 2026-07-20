//! Benchmarks for Hayate's cryptographic hot path.
//!
//! These cover the per-frame AEAD sealing/opening that every transfer worker
//! runs thousands of times, plus the one-time X25519 + HKDF key agreement that
//! gates the start of each transfer.

use divan::Bencher;
use hayate::crypto::{self, AeadKey, CIPHER_AES256_GCM, CIPHER_CHACHA20};

fn main() {
    divan::main();
}

/// Payload sizes from a small control frame up to the 1 MiB transfer chunk.
const SIZES: &[usize] = &[256, 4 * 1024, 64 * 1024, 1024 * 1024];

/// X25519 Diffie-Hellman followed by HKDF-SHA256 key derivation.
#[divan::bench]
fn derive_key(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (secret, sender_pub) = crypto::generate_keypair();
            let (_, peer_pub) = crypto::generate_keypair();
            let salt = crypto::generate_salt();
            (secret, sender_pub, peer_pub, salt)
        })
        .bench_values(|(secret, sender_pub, peer_pub, salt)| {
            divan::black_box(
                crypto::derive_key(crypto::KeyDerivationContext {
                    secret,
                    peer_pub: &peer_pub,
                    salt: &salt,
                    passphrase: Some("apple-bravo-charlie"),
                    sender_pub: &sender_pub,
                    receiver_pub: &peer_pub,
                    sender_cap: CIPHER_CHACHA20,
                    selected_cipher: CIPHER_CHACHA20,
                })
                .unwrap(),
            )
        });
}

#[divan::bench(args = SIZES)]
fn encrypt_chacha20(bencher: Bencher, size: usize) {
    bench_encrypt(bencher, size, CIPHER_CHACHA20);
}

#[divan::bench(args = SIZES)]
fn encrypt_aes256_gcm(bencher: Bencher, size: usize) {
    bench_encrypt(bencher, size, CIPHER_AES256_GCM);
}

#[divan::bench(args = SIZES)]
fn decrypt_chacha20(bencher: Bencher, size: usize) {
    bench_decrypt(bencher, size, CIPHER_CHACHA20);
}

#[divan::bench(args = SIZES)]
fn decrypt_aes256_gcm(bencher: Bencher, size: usize) {
    bench_decrypt(bencher, size, CIPHER_AES256_GCM);
}

/// Seals a payload of `size` bytes into a reused output buffer.
fn bench_encrypt(bencher: Bencher, size: usize, cipher: u8) {
    let key = [0x42u8; 32];
    let aead = AeadKey::new(&key, cipher).unwrap();
    let plaintext = vec![0xABu8; size];

    bencher.with_inputs(|| Vec::<u8>::with_capacity(size + 64)).bench_refs(|buf| {
        buf.clear();
        let frame = crypto::encrypt_frame_with_key(&aead, &plaintext, buf).unwrap();
        divan::black_box(frame.len())
    });
}

/// Opens a pre-sealed frame of `size` bytes into a reused output buffer.
fn bench_decrypt(bencher: Bencher, size: usize, cipher: u8) {
    let key = [0x42u8; 32];
    let aead = AeadKey::new(&key, cipher).unwrap();
    let plaintext = vec![0xABu8; size];

    let mut frame = Vec::with_capacity(size + 64);
    crypto::encrypt_frame_with_key(&aead, &plaintext, &mut frame).unwrap();

    bencher.with_inputs(|| Vec::<u8>::with_capacity(size)).bench_refs(|out| {
        crypto::decrypt_frame_into_with_key(&aead, &frame, out).unwrap();
        divan::black_box(out.len())
    });
}
