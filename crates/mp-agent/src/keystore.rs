//! Persistent storage for a member's secret note `(k, r)`.
//!
//! The note is the only secret a member must keep (`docs/DESIGN.md` §6.2);
//! losing it costs the ability to propose, never funds (mirror-pool is
//! non-custodial). Stored as hex of the little-endian field encoding.

use anyhow::{Context, Result};
use mp_crypto::{field, Note};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// On-disk representation of a membership note.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Keystore {
    /// Nullifier secret `k`, hex of 32 little-endian bytes.
    pub k: String,
    /// Commitment randomness `r`, hex of 32 little-endian bytes.
    pub r: String,
}

fn decode_field(hexstr: &str) -> Result<mp_crypto::F> {
    let bytes = hex::decode(hexstr).context("invalid hex")?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))?;
    field::from_bytes_le(&arr).context("non-canonical field element")
}

impl Keystore {
    /// Serialize a note.
    pub fn from_note(note: &Note) -> Self {
        Self {
            k: hex::encode(field::to_bytes_le(&note.k)),
            r: hex::encode(field::to_bytes_le(&note.r)),
        }
    }

    /// Recover the note.
    pub fn to_note(&self) -> Result<Note> {
        Ok(Note::new(decode_field(&self.k)?, decode_field(&self.r)?))
    }

    /// Write the keystore as pretty JSON.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).context("writing keystore")
    }

    /// Read a keystore from JSON.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let json = std::fs::read_to_string(path).context("reading keystore")?;
        serde_json::from_str(&json).context("parsing keystore")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_crypto::field::from_u64;

    #[test]
    fn note_roundtrips_through_keystore() {
        let note = Note::new(from_u64(123_456), from_u64(789));
        let recovered = Keystore::from_note(&note).to_note().unwrap();
        assert_eq!(note, recovered);
        assert_eq!(note.commitment(), recovered.commitment());
    }

    #[test]
    fn save_and_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.json");

        let note = Note::new(from_u64(1), from_u64(2));
        Keystore::from_note(&note).save(&path).unwrap();

        let loaded = Keystore::load(&path).unwrap().to_note().unwrap();
        assert_eq!(note, loaded);
    }

    #[test]
    fn rejects_bad_hex_length() {
        let ks = Keystore {
            k: "abcd".to_string(),
            r: hex::encode(field::to_bytes_le(&from_u64(1))),
        };
        assert!(ks.to_note().is_err());
    }
}
