//! **ucf-node** — the ed25519 key a node is known by.
//!
//! Two principals in this workspace hold one of these. The **fleet** — the captain's own
//! machine — signs the ship's autonomy lease with its key (`ucf_world::lease::issue`), and
//! the **ship** is minted one of its own at commissioning so it has a cryptographic identity
//! the fleet can name in its provisioning record. The ship verifies the lease against the
//! issuer's public identity, written into its store during the ceremony — no file outside
//! the ship's own store is ever read.
//!
//! The **node id** is a short fingerprint of the public key (the first 8 bytes of
//! `SHA-256(pubkey)`, hex) — stable, self-certifying (anyone can recompute it from the
//! pubkey), and human-legible when paired with a hostname label.
//!
//! ## The one place dependency-minimalism is relaxed
//!
//! The rest of this workspace is serde-only; a small, legible trust surface is part of the
//! promise that this thing cannot be turned against the people it serves. Signing is not
//! something to hand-roll, so this crate — and only this crate — takes on a crypto floor:
//! `ed25519-dalek`, `sha2`, `getrandom`. The concession is named here so it stays visible
//! rather than hidden. What is deliberately absent is everything above the key: group
//! certificates, brief federation, async transport, push. A ship federates with nobody.
//!
//! The file layout is fixed, because a live fleet's stores already hold these files: the
//! private key in `mesh/node_key` (0600), the public record in `mesh/node.json`.

#![forbid(unsafe_code)]

mod node;

pub use node::{fingerprint, NodeIdentity, NodeKey, NODE_FILE, NODE_KEY_FILE};

use sha2::{Digest, Sha256};
use std::fmt;

/// Content address of a byte slice: lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

/// The node error type. Kept deliberately coarse — callers log the rationale and refuse;
/// there is no fine-grained recovery beyond "do not trust this".
#[derive(Debug)]
pub enum Error {
    /// Filesystem / serialization trouble reading or writing the key state.
    Io(std::io::Error),
    /// A malformed or wrong-length key / signature / hex payload.
    Malformed(String),
    /// A signature did not verify. The most security-relevant variant: a peer failed to
    /// prove it holds the private half of the key it claims.
    Untrusted(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "node io: {e}"),
            Error::Malformed(s) => write!(f, "node malformed: {s}"),
            Error::Untrusted(s) => write!(f, "node untrusted: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Malformed(format!("json: {e}"))
    }
}

/// Result alias for node operations.
pub type Result<T> = std::result::Result<T, Error>;

// ---- small hex helpers (kept dependency-free) --------------------------------------

/// Lower-case hex encoding of a byte slice.
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

/// Decode a lower/upper-case hex string into bytes. Rejects odd length / non-hex.
pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err(Error::Malformed("hex: odd length".into()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char)
            .to_digit(16)
            .ok_or_else(|| Error::Malformed("hex: non-hex digit".into()))?;
        let lo = (bytes[i + 1] as char)
            .to_digit(16)
            .ok_or_else(|| Error::Malformed("hex: non-hex digit".into()))?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Ok(out)
}

/// Fill a fixed-size buffer with OS randomness (for minting keys).
pub(crate) fn os_random<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::getrandom(&mut buf).map_err(|e| Error::Malformed(format!("getrandom: {e}")))?;
    Ok(buf)
}

/// Exactly-32-byte view of a slice, or a `Malformed` error.
pub(crate) fn exactly_32(bytes: &[u8], what: &str) -> Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| Error::Malformed(format!("{what}: expected 32 bytes, got {}", bytes.len())))
}

/// Write a JSON document a peer is meant to read: pretty, world-readable, parents created.
pub(crate) fn write_json_public<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let data = [0x00u8, 0x0f, 0xf0, 0xab, 0xff];
        let s = hex_encode(&data);
        assert_eq!(s, "000ff0abff");
        assert_eq!(hex_decode(&s).unwrap(), data);
        assert!(hex_decode("abc").is_err()); // odd length
        assert!(hex_decode("zz").is_err()); // non-hex
    }

    #[test]
    fn os_random_is_nonzero_and_varies() {
        let a: [u8; 32] = os_random().unwrap();
        let b: [u8; 32] = os_random().unwrap();
        assert_ne!(a, [0u8; 32]);
        assert_ne!(a, b, "two draws should differ");
    }

    /// The content address is the usual SHA-256 hex, so a hash written by one side still
    /// matches when read by the other.
    #[test]
    fn sha256_hex_is_the_usual_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
