//! Binary wire protocol constants, framing, and metadata codec.
//!
//! Wire format (all multi-byte integers big-endian):
//!
//!  SENDER --> RECEIVER
//!  [2]  u16  protocol version  (= PROTOCOL_VERSION)
//!  [32] bytes sender X25519 public key
//!
//!  RECEIVER --> SENDER
//!  [32] bytes receiver X25519 public key
//!
//!  SENDER --> RECEIVER (encrypted metadata)
//!  [4]  u32  encrypted metadata length
//!  [N]  bytes  nonce + ciphertext + tag  (via crypto::encrypt_metadata)
//!       plaintext layout:
//!         [2]  u16  filename byte length
//!         [M]  bytes filename (UTF-8)
//!         [8]  u64  file/stream size (0 = unknown/directory)
//!         [1]  u8   transfer type (0x00 = file, 0x01 = directory)
//!
//!  RECEIVER --> SENDER  (consent)
//!  [1]  u8  0x01 = accept, 0x00 = reject
//!
//!  SENDER --> RECEIVER (data frames, repeated until EOF)
//!  [4]  u32  encrypted frame length
//!  [N]  bytes nonce + [1 flag byte | payload] + tag
//!       flag  0x00 = raw, 0x01 = zstd

pub const PROTOCOL_VERSION: u16 = 4;

pub const TRANSFER_FILE: u8 = 0x00;
pub const TRANSFER_DIR: u8 = 0x01;

pub const FRAME_RAW: u8 = 0x00;
pub const FRAME_ZSTD: u8 = 0x01;

/// Maximum allowed filename length in bytes.
pub const MAX_FILENAME_BYTES: usize = 4096;

/// Maximum encrypted metadata payload size (sanity cap).
pub const MAX_METADATA_ENCRYPTED: usize = 4 + MAX_FILENAME_BYTES + 8 + 1 + 12 + 16 + 16;

/// Chunk size for each data frame in bytes (4 MiB — optimal for LAN and mobile).
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Metadata that travels in the encrypted handshake.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub filename: String,
    /// Total bytes for a file transfer; 0 for directories (streaming, unknown).
    pub total_size: u64,
    pub transfer_type: u8,
}

impl Metadata {
    /// Serialises to the plaintext metadata blob.
    pub fn encode(&self) -> Vec<u8> {
        let name_bytes = self.filename.as_bytes();
        let mut buf = Vec::with_capacity(2 + name_bytes.len() + 8 + 1);
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&self.total_size.to_be_bytes());
        buf.push(self.transfer_type);
        buf
    }

    /// Deserialises from the plaintext metadata blob.
    pub fn decode(raw: &[u8]) -> Result<Self, crate::EngineError> {
        if raw.len() < 11 {
            return Err(crate::EngineError::InvalidFrame(
                "metadata too short".into(),
            ));
        }
        let name_len = u16::from_be_bytes([raw[0], raw[1]]) as usize;
        if name_len == 0 || name_len > MAX_FILENAME_BYTES {
            return Err(crate::EngineError::InvalidFrame(format!(
                "invalid filename length: {name_len}"
            )));
        }
        if raw.len() < 2 + name_len + 8 + 1 {
            return Err(crate::EngineError::InvalidFrame(
                "metadata truncated".into(),
            ));
        }
        let filename = std::str::from_utf8(&raw[2..2 + name_len])
            .map_err(|_| crate::EngineError::InvalidFrame("filename not UTF-8".into()))?
            .to_owned();
        let total_size = u64::from_be_bytes(
            raw[2 + name_len..2 + name_len + 8]
                .try_into()
                .expect("slice len == 8"),
        );
        let transfer_type = raw[2 + name_len + 8];
        Ok(Self {
            filename,
            total_size,
            transfer_type,
        })
    }
}
