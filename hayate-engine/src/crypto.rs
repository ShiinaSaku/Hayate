//! Cryptographic primitives: X25519 ECDH key exchange, HKDF-SHA256 key
//! derivation, and ChaCha20-Poly1305 AEAD encryption/decryption.
//!
//! All functions are purely synchronous CPU work — no async needed here.

use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{AeadInPlace, KeyInit, OsRng},
};
use ring::hkdf;
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::EngineError;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// Generates an ephemeral X25519 key pair.
/// Returns `(secret, public_key_bytes)`.
#[must_use]
pub fn generate_keypair() -> (EphemeralSecret, [u8; 32]) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (secret, public.to_bytes())
}

/// Performs X25519 DH and derives a 32-byte symmetric key via HKDF-SHA256.
pub fn derive_key(
    secret: EphemeralSecret,
    peer_pub: &[u8; 32],
    passphrase: Option<&str>,
) -> Result<[u8; 32], EngineError> {
    let peer = PublicKey::from(*peer_pub);
    let shared = secret.diffie_hellman(&peer);
    let salt_bytes = match passphrase {
        Some(phrase) => phrase.as_bytes(),
        None => b"hayate-v2-default-salt",
    };
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt_bytes);
    let prk = salt.extract(shared.as_bytes());
    let info: &[&[u8]] = &[b"hayate-v2-key"];
    let okm = prk
        .expand(info, hkdf::HKDF_SHA256)
        .map_err(|_| EngineError::Crypto("HKDF expand failed".into()))?;
    let mut key = [0u8; 32];
    okm.fill(&mut key)
        .map_err(|_| EngineError::Crypto("HKDF expand failed".into()))?;
    Ok(key)
}

/// Encrypts `plaintext` in-place inside `buf`, writing nonce + ciphertext + tag.
/// `buf` must have capacity >= `nonce_len` + `plaintext.len()` + `tag_len`.
/// Returns the byte slice `buf[..nonce_len + plaintext.len() + tag_len]`.
pub fn encrypt_frame<'buf>(
    key: &[u8; 32],
    plaintext: &[u8],
    buf: &'buf mut Vec<u8>,
) -> Result<&'buf [u8], EngineError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);

    let start = buf.len();
    buf.extend_from_slice(&nonce_bytes);
    buf.extend_from_slice(plaintext);
    // The tag will be appended.
    buf.extend_from_slice(&[0u8; TAG_LEN]);

    let ciphertext_and_tag = &mut buf[start + NONCE_LEN..];
    let plaintext_len = plaintext.len();
    cipher
        .encrypt_in_place_detached(&nonce, b"", &mut ciphertext_and_tag[..plaintext_len])
        .map(|tag| {
            ciphertext_and_tag[plaintext_len..].copy_from_slice(tag.as_slice());
        })
        .map_err(|_| EngineError::Crypto("AEAD encrypt failed".into()))?;

    Ok(&buf[start..])
}

/// Decrypts a frame produced by `encrypt_frame`.
/// Input: nonce (12 bytes) + ciphertext + tag (16 bytes).
/// Returns plaintext as a freshly allocated `Vec<u8>`.
pub fn decrypt_frame(key: &[u8; 32], frame: &[u8]) -> Result<Vec<u8>, EngineError> {
    if frame.len() < NONCE_LEN + TAG_LEN {
        return Err(EngineError::Crypto("frame too short".into()));
    }
    let (nonce_bytes, rest) = frame.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let ciphertext_len = rest.len() - TAG_LEN;
    let (ciphertext, tag_bytes) = rest.split_at(ciphertext_len);

    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut plaintext = ciphertext.to_vec();
    cipher
        .decrypt_in_place_detached(
            nonce,
            b"",
            &mut plaintext,
            chacha20poly1305::Tag::from_slice(tag_bytes),
        )
        .map_err(|_| EngineError::Crypto("AEAD decrypt failed".into()))?;
    Ok(plaintext)
}

/// Decrypts a frame produced by `encrypt_frame` into a reused buffer.
/// Input: nonce (12 bytes) + ciphertext + tag (16 bytes).
/// Writes the decrypted plaintext to `out`.
pub fn decrypt_frame_into(
    key: &[u8; 32],
    frame: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), EngineError> {
    if frame.len() < NONCE_LEN + TAG_LEN {
        return Err(EngineError::Crypto("frame too short".into()));
    }
    let (nonce_bytes, rest) = frame.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let ciphertext_len = rest.len() - TAG_LEN;
    let (ciphertext, tag_bytes) = rest.split_at(ciphertext_len);

    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    out.clear();
    out.extend_from_slice(ciphertext);
    cipher
        .decrypt_in_place_detached(
            nonce,
            b"",
            out,
            chacha20poly1305::Tag::from_slice(tag_bytes),
        )
        .map_err(|_| EngineError::Crypto("AEAD decrypt failed".into()))?;
    Ok(())
}

/// Encrypts a small metadata blob with a freshly-derived nonce.
/// Same layout as `encrypt_frame`.
pub fn encrypt_metadata(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, EngineError> {
    let mut out = Vec::with_capacity(NONCE_LEN + plaintext.len() + TAG_LEN);
    encrypt_frame(key, plaintext, &mut out)?;
    Ok(out)
}

/// Decrypts a metadata blob encrypted with `encrypt_metadata`.
pub fn decrypt_metadata(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, EngineError> {
    decrypt_frame(key, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phrase_key_derivation() {
        let (sec1, pub1) = generate_keypair();
        let (sec2, pub2) = generate_keypair();

        let phrase = "apple-bravo-charlie";

        // Correct phrase on both sides
        let key1 = derive_key(sec1, &pub2, Some(phrase)).unwrap();
        let key2 = derive_key(sec2, &pub1, Some(phrase)).unwrap();
        assert_eq!(key1, key2);

        // Different phrases
        let (sec3, pub3) = generate_keypair();
        let (sec4, pub4) = generate_keypair();
        let key3 = derive_key(sec3, &pub4, Some("apple-bravo-charlie")).unwrap();
        let key4 = derive_key(sec4, &pub3, Some("apple-bravo-delta")).unwrap();
        assert_ne!(key3, key4);

        // No phrase (default salt)
        let (sec5, pub5) = generate_keypair();
        let (sec6, pub6) = generate_keypair();
        let key5 = derive_key(sec5, &pub6, None).unwrap();
        let key6 = derive_key(sec6, &pub5, None).unwrap();
        assert_eq!(key5, key6);
        assert_ne!(key1, key5); // should differ from phrase-derived keys
    }

    #[test]
    fn test_metadata_encryption_and_decryption_mismatch() {
        let (sec_sender, pub_sender) = generate_keypair();
        let (sec_receiver, pub_receiver) = generate_keypair();

        let correct_phrase = "my-secret-phrase";
        let wrong_phrase = "wrong-secret-phrase";

        let sender_key = derive_key(sec_sender, &pub_receiver, Some(correct_phrase)).unwrap();
        let receiver_key_wrong = derive_key(sec_receiver, &pub_sender, Some(wrong_phrase)).unwrap();

        let plain_meta = b"metadata-payload";
        let encrypted = encrypt_metadata(&sender_key, plain_meta).unwrap();

        // Decrypting with wrong key must fail
        let decrypt_res = decrypt_metadata(&receiver_key_wrong, &encrypted);
        assert!(decrypt_res.is_err());
    }
}
