//! Cryptographic primitives: X25519 ECDH key exchange, HKDF-SHA256 key
//! derivation, and AES-GCM / ChaCha20-Poly1305 AEAD.
//!
//! ## Design notes
//!
//! * All AEAD key expansion happens via [`AeadKey::new`]. Build it **once** per
//!   transfer; pass it to every frame operation with the `_with_key` variants.
//!   The bare one-shot variants have been intentionally removed to avoid
//!   accidentally paying key-expansion cost in the hot loop.
//! * Metadata uses a random 96-bit nonce from `ring`'s `SystemRandom`. Payload
//!   frames use a per-worker nonce prefix plus a 64-bit counter, so the hot
//!   loop never calls the CSPRNG and nonce reuse is structurally impossible
//!   within a session.
//! * A passphrase is stretched with memory-hard Argon2id before HKDF.
//! * HKDF is performed via `ring::hkdf` (ring already exposes it); the
//!   standalone `hkdf` crate has been dropped.
//! * Key derivation uses a random, public per-session salt sent on the wire and
//!   a transcript-bound HKDF info string. A passphrase, when provided, is
//!   Argon2id-stretched and mixed into the HKDF input keying material, not used
//!   as the salt.

use argon2::{Algorithm, Argon2, Params, Version};
use getrandom::SysRng;
use rand_core::{Rng, UnwrapErr};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use x25519_dalek::{EphemeralSecret, PublicKey};
use zeroize::Zeroize;

use crate::EngineError;

/// Cipher suite identifier for ChaCha20-Poly1305.
pub const CIPHER_CHACHA20: u8 = 0x00;

/// Cipher suite identifier for AES-256-GCM.
pub const CIPHER_AES256_GCM: u8 = 0x01;

/// Length of the HKDF salt sent on the wire in bytes.
pub const SALT_LEN: usize = 32;

/// Length of the X25519 public key in bytes.
pub const PUBLIC_KEY_LEN: usize = 32;

/// Length of the AEAD nonce in bytes (12 bytes).
pub const NONCE_LEN: usize = 12;

/// Length of the AEAD authentication tag in bytes (16 bytes).
pub const TAG_LEN: usize = 16;

/// Argon2id memory cost in KiB (19 MiB — the OWASP minimum).
const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
/// Argon2id iteration count (2 — the OWASP minimum).
const ARGON2_ITERATIONS: u32 = 2;
/// Argon2id lane count. One lane keeps the handshake from spawning a thread
/// pool just to stretch a four-word code.
const ARGON2_LANES: u32 = 1;

/// Dynamically detects if the host CPU possesses hardware AES acceleration.
///
/// Returns `false` when `HAYATE_FORCE_CHACHA20` is set, regardless of
/// hardware capabilities — useful for testing or comparing cipher performance.
#[inline]
#[must_use]
pub fn is_aes_hw_accelerated() -> bool {
    if std::env::var_os("HAYATE_FORCE_CHACHA20").is_some() {
        return false;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("aes")
    }
    #[cfg(target_arch = "aarch64")]
    {
        std::arch::is_aarch64_feature_detected!("aes")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    {
        false
    }
}

/// Prepared AEAD key for repeated frame operations.
///
/// Constructing a `ring::aead::LessSafeKey` expands and validates the
/// selected cipher key. Transfer workers process thousands of frames with
/// the same negotiated key — build this **once** at handshake time and reuse
/// it for the entire transfer via [`encrypt_frame_with_key`] /
/// [`decrypt_frame_into_with_key`].
pub struct AeadKey {
    inner: LessSafeKey,
}

impl AeadKey {
    /// Creates a reusable AEAD key from the negotiated 32-byte transfer key.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Crypto`] if `cipher_id` is unknown or if key
    /// expansion fails.
    pub fn new(key: &[u8; 32], cipher_id: u8) -> Result<Self, EngineError> {
        let algo = cipher_algorithm(cipher_id)?;
        let unbound = UnboundKey::new(algo, key)
            .map_err(|_| EngineError::Crypto("failed to create UnboundKey"))?;
        Ok(Self { inner: LessSafeKey::new(unbound) })
    }
}

/// Resolves the `ring` AEAD algorithm for the given cipher ID.
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] for unknown `cipher_id` values.
pub fn cipher_algorithm(cipher_id: u8) -> Result<&'static ring::aead::Algorithm, EngineError> {
    match cipher_id {
        CIPHER_CHACHA20 => Ok(&ring::aead::CHACHA20_POLY1305),
        CIPHER_AES256_GCM => Ok(&ring::aead::AES_256_GCM),
        _ => Err(EngineError::Crypto("unknown cipher suite")),
    }
}

/// Generates an ephemeral X25519 key pair.
///
/// Returns `(secret, public_key_bytes)`. The caller must keep the secret
/// alive until [`derive_key`] consumes it.
#[must_use]
pub fn generate_keypair() -> (EphemeralSecret, [u8; PUBLIC_KEY_LEN]) {
    let secret = EphemeralSecret::random_from_rng(&mut UnwrapErr(SysRng));
    let public = PublicKey::from(&secret);
    (secret, public.to_bytes())
}

/// Generates a random 32-byte HKDF salt.
///
/// Each transfer session gets a fresh salt sent on the wire in the clear.
/// The salt is public but unique per session, ensuring the HKDF extraction
/// step is never repeated across runs even when no passphrase is used.
#[must_use]
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    UnwrapErr(SysRng).fill_bytes(&mut salt);
    salt
}

/// Validates a received X25519 public key.
///
/// Rejects the all-zero key. X25519 clamps the public u-coordinate during the
/// ECDH step, but an all-zero key is guaranteed to produce an all-zero shared
/// secret, so it is rejected up front.
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] for an invalid public key.
pub fn validate_public_key(peer_pub: &[u8; PUBLIC_KEY_LEN]) -> Result<(), EngineError> {
    if peer_pub == &[0u8; PUBLIC_KEY_LEN] {
        return Err(EngineError::Crypto("peer public key is all-zero"));
    }
    Ok(())
}

/// Context for a single transfer's key derivation.
///
/// All fields are public because the caller assembles the handshake
/// transcript values directly; this struct simply keeps the parameter list
/// manageable.
pub struct KeyDerivationContext<'a> {
    /// Our ephemeral secret; consumed by the ECDH step.
    pub secret: EphemeralSecret,
    /// Peer public key bytes.
    pub peer_pub: &'a [u8; PUBLIC_KEY_LEN],
    /// Public per-session HKDF salt.
    pub salt: &'a [u8; SALT_LEN],
    /// Optional pairing passphrase (secret IKM material, never the salt).
    pub passphrase: Option<&'a str>,
    /// Sender X25519 public key bytes.
    pub sender_pub: &'a [u8; PUBLIC_KEY_LEN],
    /// Receiver X25519 public key bytes.
    pub receiver_pub: &'a [u8; PUBLIC_KEY_LEN],
    /// Sender's cipher capability byte.
    pub sender_cap: u8,
    /// Negotiated cipher suite byte.
    pub selected_cipher: u8,
}

/// Performs X25519 DH and derives a 32-byte symmetric key via HKDF-SHA256.
///
/// The `salt` is a random, public per-session value sent on the wire. The
/// `passphrase`, when provided, is secret material mixed into the HKDF input
/// keying material. The resulting key is bound to the transcript formed by the
/// protocol version, cipher capabilities, selected cipher, public keys, and
/// salt, providing session-specific forward secrecy and replay resistance.
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] if HKDF expansion fails or the public key is
/// rejected by [`validate_public_key`].
pub fn derive_key(ctx: KeyDerivationContext<'_>) -> Result<[u8; 32], EngineError> {
    validate_public_key(ctx.peer_pub)?;
    let peer = PublicKey::from(*ctx.peer_pub);
    let shared = ctx.secret.diffie_hellman(&peer);
    if shared.as_bytes() == &[0u8; PUBLIC_KEY_LEN] {
        return Err(EngineError::Crypto("X25519 shared secret is all-zero"));
    }

    let hkdf_salt = hkdf::Salt::new(hkdf::HKDF_SHA256, ctx.salt.as_slice());

    // Structure the IKM so the passphrase and no-passphrase cases are
    // unambiguous: fixed-length shared secret followed by a tagged secret,
    // or an explicit no-passphrase marker. A passphrase is stretched with
    // memory-hard Argon2id first, so an active on-path attacker who learns
    // both X25519 shared secrets cannot cheaply brute-force the four-word
    // code offline. The salt is per-session and public, which is exactly what
    // Argon2 needs.
    let mut ikm = Vec::with_capacity(PUBLIC_KEY_LEN + 1 + 32);
    ikm.extend_from_slice(shared.as_bytes());
    if let Some(phrase) = ctx.passphrase {
        let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_LANES, Some(32))
            .map_err(|_| EngineError::Crypto("invalid Argon2id parameters"))?;
        let mut stretched = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(phrase.as_bytes(), ctx.salt.as_slice(), &mut stretched)
            .map_err(|_| EngineError::Crypto("Argon2id passphrase stretch failed"))?;
        ikm.push(0x02);
        ikm.extend_from_slice(&stretched);
        stretched.zeroize();
    } else {
        ikm.push(0x00);
    }
    let prk = hkdf_salt.extract(&ikm);
    ikm.zeroize();

    let version_bytes = crate::protocol::PROTOCOL_VERSION.to_be_bytes();
    let info = [
        version_bytes.as_slice(),
        &[ctx.sender_cap],
        &[ctx.selected_cipher],
        ctx.sender_pub.as_slice(),
        ctx.receiver_pub.as_slice(),
        ctx.salt.as_slice(),
        b"hayate-v3",
    ];

    let okm = prk
        .expand(&info, hkdf::HKDF_SHA256)
        .map_err(|_| EngineError::Crypto("HKDF expand failed"))?;
    let mut key = [0u8; 32];
    okm.fill(&mut key).map_err(|_| EngineError::Crypto("HKDF fill failed"))?;
    Ok(key)
}

/// Encrypts `plaintext` with an already-prepared AEAD key.
///
/// Writes `nonce (12 B) || ciphertext || tag (16 B)` into `buf` and returns
/// a slice into the appended region. The buffer is **extended**, not cleared,
/// so callers can chain multiple frames.
///
/// Use [`decrypt_frame_into_with_key`] on the receiver side.
pub fn encrypt_frame_with_key<'buf>(
    key: &AeadKey,
    plaintext: &[u8],
    buf: &'buf mut Vec<u8>,
) -> Result<&'buf [u8], EngineError> {
    // ring's SystemRandom is already in the dep graph for AEAD; no extra crate.
    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce_bytes).map_err(|_| EngineError::Crypto("RNG failed"))?;
    encrypt_frame_with_nonce(key, &nonce_bytes, plaintext, buf)
}

/// Encrypts `plaintext` with a caller-supplied 96-bit nonce.
///
/// This is the hot-path variant for payload frames. The caller **must** use
/// each nonce at most once with a given key. Payload workers build a unique
/// nonce from their worker index and a monotonically increasing frame
/// counter, which removes a CSPRNG call (and the 96-bit random-nonce
/// birthday bound) from every 4 MiB frame.
///
/// Metadata still uses [`encrypt_frame_with_key`] because it is a one-shot
/// value and a random nonce is simpler there.
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] if AEAD sealing fails.
pub fn encrypt_frame_with_nonce<'buf>(
    key: &AeadKey,
    nonce_bytes: &[u8; NONCE_LEN],
    plaintext: &[u8],
    buf: &'buf mut Vec<u8>,
) -> Result<&'buf [u8], EngineError> {
    let nonce = Nonce::assume_unique_for_key(*nonce_bytes);

    let start = buf.len();
    buf.extend_from_slice(nonce_bytes);

    let plain_start = buf.len();
    buf.extend_from_slice(plaintext);
    let plain_end = buf.len();

    let tag = key
        .inner
        .seal_in_place_separate_tag(nonce, Aad::empty(), &mut buf[plain_start..plain_end])
        .map_err(|_| EngineError::Crypto("AEAD encrypt failed"))?;

    buf.extend_from_slice(tag.as_ref());
    Ok(&buf[start..])
}

/// Decrypts a frame produced by [`encrypt_frame_with_key`] into a reused
/// buffer.
///
/// This is the hot-path variant used by transfer workers that process
/// thousands of frames with the same negotiated key. The output buffer is
/// cleared and filled with the plaintext on success.
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] if the frame is too short, or if AEAD
/// authentication fails (wrong key, corrupted data, or tampered ciphertext).
pub fn decrypt_frame_into_with_key(
    key: &AeadKey,
    frame: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), EngineError> {
    if frame.len() < NONCE_LEN + TAG_LEN {
        return Err(EngineError::Crypto("frame too short"));
    }
    let (nonce_bytes, rest) = frame.split_at(NONCE_LEN);
    let mut nonce_array = [0u8; NONCE_LEN];
    nonce_array.copy_from_slice(nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_array);

    out.clear();
    out.extend_from_slice(rest);

    let plaintext_len = key
        .inner
        .open_in_place(nonce, Aad::empty(), out)
        .map_err(|_| EngineError::Crypto("AEAD decrypt failed"))?
        .len();
    out.truncate(plaintext_len);
    Ok(())
}

/// Encrypts a small metadata blob with a freshly-derived nonce.
///
/// Convenience wrapper over [`encrypt_frame_with_key`] for single-use
/// metadata frames (not the hot payload loop).
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] on AEAD or key errors.
pub fn encrypt_metadata(
    key: &[u8; 32],
    cipher_id: u8,
    plaintext: &[u8],
) -> Result<Vec<u8>, EngineError> {
    let aead = AeadKey::new(key, cipher_id)?;
    let mut out = Vec::with_capacity(NONCE_LEN + plaintext.len() + TAG_LEN);
    encrypt_frame_with_key(&aead, plaintext, &mut out)?;
    Ok(out)
}

/// Decrypts a metadata blob encrypted with [`encrypt_metadata`].
///
/// # Errors
///
/// Returns [`EngineError::Crypto`] if decryption or authentication fails.
pub fn decrypt_metadata(
    key: &[u8; 32],
    cipher_id: u8,
    data: &[u8],
) -> Result<Vec<u8>, EngineError> {
    let aead = AeadKey::new(key, cipher_id)?;
    let mut out = Vec::with_capacity(data.len());
    decrypt_frame_into_with_key(&aead, data, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_key_derivation_roundtrip() {
        let (sec1, pub1) = generate_keypair();
        let (sec2, pub2) = generate_keypair();
        let salt = generate_salt();
        let phrase = "apple-bravo-charlie";

        let key1 = derive_key(KeyDerivationContext {
            secret: sec1,
            peer_pub: &pub2,
            salt: &salt,
            passphrase: Some(phrase),
            sender_pub: &pub1,
            receiver_pub: &pub2,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        let key2 = derive_key(KeyDerivationContext {
            secret: sec2,
            peer_pub: &pub1,
            salt: &salt,
            passphrase: Some(phrase),
            sender_pub: &pub1,
            receiver_pub: &pub2,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        assert_eq!(key1, key2, "matching phrases and transcripts must yield the same key");

        // Mismatched phrases must produce different keys.
        let (sec3, pub3) = generate_keypair();
        let (sec4, pub4) = generate_keypair();
        let salt2 = generate_salt();
        let key3 = derive_key(KeyDerivationContext {
            secret: sec3,
            peer_pub: &pub4,
            salt: &salt2,
            passphrase: Some("apple-bravo-charlie"),
            sender_pub: &pub3,
            receiver_pub: &pub4,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        let key4 = derive_key(KeyDerivationContext {
            secret: sec4,
            peer_pub: &pub3,
            salt: &salt2,
            passphrase: Some("apple-bravo-delta"),
            sender_pub: &pub3,
            receiver_pub: &pub4,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        assert_ne!(key3, key4);

        // No-passphrase path.
        let (sec5, pub5) = generate_keypair();
        let (sec6, pub6) = generate_keypair();
        let salt3 = generate_salt();
        let key5 = derive_key(KeyDerivationContext {
            secret: sec5,
            peer_pub: &pub6,
            salt: &salt3,
            passphrase: None,
            sender_pub: &pub5,
            receiver_pub: &pub6,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        let key6 = derive_key(KeyDerivationContext {
            secret: sec6,
            peer_pub: &pub5,
            salt: &salt3,
            passphrase: None,
            sender_pub: &pub5,
            receiver_pub: &pub6,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        assert_eq!(key5, key6);
        assert_ne!(key1, key5, "passphrase-derived must differ from no-passphrase");
    }

    #[test]
    fn wrong_passphrase_fails_decryption() {
        let (sec_s, pub_s) = generate_keypair();
        let (sec_r, pub_r) = generate_keypair();
        let salt = generate_salt();
        let sender_key = derive_key(KeyDerivationContext {
            secret: sec_s,
            peer_pub: &pub_r,
            salt: &salt,
            passphrase: Some("correct"),
            sender_pub: &pub_s,
            receiver_pub: &pub_r,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();
        let receiver_key_wrong = derive_key(KeyDerivationContext {
            secret: sec_r,
            peer_pub: &pub_s,
            salt: &salt,
            passphrase: Some("wrong"),
            sender_pub: &pub_s,
            receiver_pub: &pub_r,
            sender_cap: CIPHER_CHACHA20,
            selected_cipher: CIPHER_CHACHA20,
        })
        .unwrap();

        let encrypted =
            encrypt_metadata(&sender_key, CIPHER_CHACHA20, b"metadata-payload").unwrap();
        assert!(
            decrypt_metadata(&receiver_key_wrong, CIPHER_CHACHA20, &encrypted).is_err(),
            "wrong key must fail AEAD authentication"
        );
    }

    #[test]
    fn all_ciphers_encrypt_decrypt() {
        let key = [42u8; 32];
        let plain = b"hello world from cipher negotiation";
        for &cipher_id in &[CIPHER_CHACHA20, CIPHER_AES256_GCM] {
            let enc = encrypt_metadata(&key, cipher_id, plain).unwrap();
            let dec = decrypt_metadata(&key, cipher_id, &enc).unwrap();
            assert_eq!(plain, dec.as_slice());
        }
    }

    #[test]
    fn caller_supplied_nonce_roundtrips_and_is_not_reused_implicitly() {
        let key = [7u8; 32];
        let aead = AeadKey::new(&key, CIPHER_CHACHA20).unwrap();
        let nonce_a = [1u8; NONCE_LEN];
        let nonce_b = [2u8; NONCE_LEN];

        let mut frame_a = Vec::new();
        crypto_frame(&aead, &nonce_a, b"same plaintext", &mut frame_a);
        let mut frame_b = Vec::new();
        crypto_frame(&aead, &nonce_b, b"same plaintext", &mut frame_b);
        assert_ne!(frame_a, frame_b, "different nonces must produce different frames");

        let mut out = Vec::new();
        decrypt_frame_into_with_key(&aead, &frame_a, &mut out).unwrap();
        assert_eq!(out, b"same plaintext");
    }

    fn crypto_frame(aead: &AeadKey, nonce: &[u8; NONCE_LEN], plain: &[u8], out: &mut Vec<u8>) {
        encrypt_frame_with_nonce(aead, nonce, plain, out).unwrap();
    }

    #[test]
    fn rejects_all_zero_public_key() {
        assert!(validate_public_key(&[0u8; PUBLIC_KEY_LEN]).is_err());
    }

    #[test]
    fn accepts_non_zero_public_key() {
        let (_sec, pub_bytes) = generate_keypair();
        assert!(validate_public_key(&pub_bytes).is_ok());
    }
}
