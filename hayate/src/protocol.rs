//! Binary wire protocol constants, framing, and metadata codec.
//!
//! Wire format, with all multi-byte integers encoded big-endian:
//!
//! | Direction | Bytes | Field |
//! | --- | ---: | --- |
//! | Sender to receiver | 2 | Protocol version, equal to [`PROTOCOL_VERSION`] |
//! | Sender to receiver | 1 | Sender's preferred cipher capability |
//! | Sender to receiver | 32 | Sender X25519 public key |
//! | Sender to receiver | 32 | HKDF salt (random per session, sent in the clear) |
//! | Receiver to sender | 32 | Receiver X25519 public key |
//! | Receiver to sender | 1 | Selected cipher suite |
//! | Sender to receiver | 4 | Encrypted metadata length |
//! | Sender to receiver | N | Metadata frame: nonce, ciphertext, authentication tag |
//! | Receiver to sender | 1 | Consent byte: `0x01` accept, `0x00` reject |
//! | Sender to receiver | 4 | Encrypted payload frame length |
//! | Sender to receiver | N | Payload frame: nonce, encrypted flag/payload, authentication tag |
//!
//! Plaintext metadata layout:
//!
//! | Bytes | Field |
//! | ---: | --- |
//! | 2 | Filename byte length |
//! | M | UTF-8 filename |
//! | 8 | File size, or `0` for directory streams |
//! | 1 | Transfer type: [`TransferKind::File`] or [`TransferKind::Directory`] |
//! | 1 | Hash algorithm name length |
//! | H | Hash algorithm name |
//!
//! Decrypted payload frames begin with one flag byte: [`FRAME_RAW`] for
//! uncompressed payloads, [`FRAME_ZSTD`] for zstd-compressed payloads.

/// Current binary wire protocol version.
pub(crate) const PROTOCOL_VERSION: u16 = 6;

/// Metadata transfer type for a single file.
pub(crate) const TRANSFER_FILE: u8 = 0x00;
/// Metadata transfer type for a directory encoded as a tar stream.
pub(crate) const TRANSFER_DIR: u8 = 0x01;

/// Transfer kind: a single file or a directory encoded as a tar stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransferKind {
    /// A single file transfer.
    File = 0x00,
    /// A directory transfer, streamed as a tar archive.
    Directory = 0x01,
}

impl TransferKind {
    /// Converts a wire transfer-type byte into a [`TransferKind`].
    ///
    /// Returns `None` for unknown values.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            TRANSFER_FILE => Some(Self::File),
            TRANSFER_DIR => Some(Self::Directory),
            _ => None,
        }
    }

    /// Returns the wire transfer-type byte for this kind.
    #[must_use]
    #[allow(clippy::as_conversions)]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Payload frame flag for bytes that were sent without compression.
pub(crate) const FRAME_RAW: u8 = 0x00;
/// Payload frame flag for bytes compressed with zstd.
pub(crate) const FRAME_ZSTD: u8 = 0x01;

/// Maximum allowed filename length in bytes.
pub(crate) const MAX_FILENAME_BYTES: usize = 4096;

/// Maximum encrypted metadata payload size.
///
/// The cap includes the plaintext metadata fields plus nonce/tag overhead and
/// a small margin, preventing malicious peers from forcing large allocations
/// during the handshake.
pub(crate) const MAX_METADATA_ENCRYPTED: usize =
    4 + MAX_FILENAME_BYTES + 8 + 1 + 1 + 256 + 12 + 16 + 16;

/// Chunk size for each data frame in bytes.
///
/// 4 MiB amortises per-frame AEAD overhead (12-byte nonce + 16-byte tag) over
/// a much larger payload. On Gigabit+ LAN this reduces frame-count by 4× vs
/// the old 1 MiB size, cutting syscall and scheduler overhead proportionally.
/// The pipeline pool and QUIC flow windows are sized to keep 8 frames in flight
/// simultaneously, so peak buffer usage is ~256 MiB across sender + receiver.
pub(crate) const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// Metadata that travels in the encrypted handshake.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Metadata {
    /// Display name sent to the receiver and used as the output filename.
    pub filename: String,
    /// Total bytes for a file transfer; 0 for directories (streaming, unknown).
    pub total_size: u64,
    /// Transfer kind, either [`TransferKind::File`] or
    /// [`TransferKind::Directory`].
    pub transfer_type: TransferKind,
    /// Hash algorithm used for payload integrity (e.g., "blake3", "sha256").
    pub hash_algo: String,
}

impl Metadata {
    /// Creates a new [`Metadata`] instance.
    #[must_use]
    pub fn new(
        filename: String,
        total_size: u64,
        transfer_type: TransferKind,
        hash_algo: String,
    ) -> Self {
        Self { filename, total_size, transfer_type, hash_algo }
    }

    /// Validates metadata fields before they are encoded or used to route a
    /// payload.
    ///
    /// This rejects empty or oversized names and any hash algorithm the engine
    /// does not know how to compute and verify.
    #[inline]
    pub fn validate(&self) -> Result<(), crate::EngineError> {
        let name_len = self.filename.len();
        if name_len == 0 || name_len > MAX_FILENAME_BYTES {
            return Err(crate::EngineError::InvalidFrame(format!(
                "invalid filename length: {name_len}"
            )));
        }
        let algo = self.hash_algo.as_str();
        if !is_known_hash_algo(algo) {
            return Err(crate::EngineError::InvalidFrame(format!(
                "unsupported hash algorithm: {algo}"
            )));
        }
        Ok(())
    }
}

/// Hash algorithms the engine can compute and verify payload integrity.
pub const KNOWN_HASH_ALGOS: &[&str] = &["blake3", "sha256"];

/// Returns `true` if `algo` is a hash algorithm the engine supports.
#[must_use]
#[inline]
pub fn is_known_hash_algo(algo: &str) -> bool {
    KNOWN_HASH_ALGOS.contains(&algo)
}

impl Metadata {
    /// Serialises to the plaintext metadata blob.
    ///
    /// Callers should validate metadata before encoding it. Metadata decoded
    /// from the wire is validated by [`Self::decode`].
    pub fn encode(&self) -> Vec<u8> {
        let name_bytes = self.filename.as_bytes();
        let algo_bytes = self.hash_algo.as_bytes();
        let mut buf = Vec::with_capacity(2 + name_bytes.len() + 8 + 1 + 1 + algo_bytes.len());
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&self.total_size.to_be_bytes());
        buf.push(self.transfer_type.as_u8());
        buf.push(algo_bytes.len() as u8);
        buf.extend_from_slice(algo_bytes);
        buf
    }

    /// Deserialises from the plaintext metadata blob.
    pub fn decode(raw: &[u8]) -> Result<Self, crate::EngineError> {
        if raw.len() < 12 {
            return Err(crate::EngineError::InvalidFrame("metadata too short".into()));
        }
        let name_len = u16::from_be_bytes([raw[0], raw[1]]) as usize;
        if name_len == 0 || name_len > MAX_FILENAME_BYTES {
            return Err(crate::EngineError::InvalidFrame(format!(
                "invalid filename length: {name_len}"
            )));
        }
        if raw.len() < 2 + name_len + 8 + 1 + 1 {
            return Err(crate::EngineError::InvalidFrame(
                "metadata truncated before hash algorithm".into(),
            ));
        }
        let filename = std::str::from_utf8(&raw[2..2 + name_len])
            .map_err(|_| crate::EngineError::InvalidFrame("filename not UTF-8".into()))?
            .to_owned();
        let total_size = u64::from_be_bytes(
            raw[2 + name_len..2 + name_len + 8]
                .try_into()
                .map_err(|_| crate::EngineError::InvalidFrame("metadata truncated".into()))?,
        );
        let transfer_type = TransferKind::from_u8(raw[2 + name_len + 8]).ok_or_else(|| {
            crate::EngineError::InvalidFrame(format!(
                "invalid transfer type: 0x{:02x}",
                raw[2 + name_len + 8]
            ))
        })?;
        let algo_len = raw[2 + name_len + 9] as usize;
        if raw.len() < 2 + name_len + 10 + algo_len {
            return Err(crate::EngineError::InvalidFrame(
                "metadata truncated for hash algorithm name".into(),
            ));
        }
        let hash_algo = std::str::from_utf8(&raw[2 + name_len + 10..2 + name_len + 10 + algo_len])
            .map_err(|_| crate::EngineError::InvalidFrame("hash algorithm not UTF-8".into()))?
            .to_owned();
        if !is_known_hash_algo(&hash_algo) {
            return Err(crate::EngineError::InvalidFrame(format!(
                "unsupported hash algorithm: {hash_algo}"
            )));
        }

        Ok(Self::new(filename, total_size, transfer_type, hash_algo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_unknown_transfer_type() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&4u16.to_be_bytes());
        raw.extend_from_slice(b"name");
        raw.extend_from_slice(&123u64.to_be_bytes());
        raw.push(0xff);

        let err = Metadata::decode(&raw).unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidFrame(_)));
    }

    #[test]
    fn validate_rejects_empty_filename() {
        let meta = Metadata::new(String::new(), 0, TransferKind::File, "blake3".to_owned());

        let err = meta.validate().unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidFrame(_)));
    }

    #[test]
    fn validate_rejects_unknown_hash_algo() {
        let meta = Metadata::new("file.bin".to_owned(), 0, TransferKind::File, "md5".to_owned());

        let err = meta.validate().unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidFrame(_)));
    }

    #[test]
    fn validate_accepts_known_hash_algos() {
        for algo in KNOWN_HASH_ALGOS {
            let meta =
                Metadata::new("file.bin".to_owned(), 0, TransferKind::File, (*algo).to_owned());
            assert!(meta.validate().is_ok(), "expected {algo} to validate");
        }
    }
}
