//! Journal d'opérations append-only chiffré (§4.3 + §0.1 Phase 0).
//!
//! Chaque opération est :
//!   1. Sérialisée en CBOR  (ciborium)
//!   2. Chiffrée avec la DEK (XChaCha20-Poly1305)
//!   3. Écrite en fin de fichier (append-only — jamais de réécriture du passé)
//!
//! Le rejeu ordonné reconstruit l'état courant (stock + numérotation).

use crate::crypto::{decrypt, encrypt, CryptoError, Dek, EncryptedBlob};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Nature de l'opération métier (minimal pour le spike).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OpType {
    Sale,
    StockAdjust,
}

/// Contenu métier minimal d'une opération (spike).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub item_id:  String,
    pub quantity: i64,
}

/// Une opération du journal (§0.1 Stack).
/// `seq` est imposé par le nœud actif — l'ordre unique, pièce maîtresse anti-survente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub seq:            u64,
    pub op_type:        OpType,
    pub op_id:          Uuid,
    pub ts:             i64,
    pub payload:        Payload,
    pub schema_version: u8,
}

impl Operation {
    pub const CURRENT_SCHEMA: u8 = 1;

    pub fn new(seq: u64, op_type: OpType, payload: Payload) -> Self {
        Self {
            seq,
            op_type,
            op_id: Uuid::new_v4(),
            ts: chrono::Utc::now().timestamp(),
            payload,
            schema_version: Self::CURRENT_SCHEMA,
        }
    }
}

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("erreur crypto : {0}")]
    Crypto(#[from] CryptoError),
    #[error("erreur CBOR sérialisation : {0}")]
    CborEncode(String),
    #[error("erreur CBOR désérialisation : {0}")]
    CborDecode(String),
    #[error("séquence invalide : attendu {expected}, reçu {got}")]
    BadSequence { expected: u64, got: u64 },
    #[error("erreur IO : {0}")]
    Io(#[from] std::io::Error),
}

// ── Sérialisation CBOR ───────────────────────────────────────────────────────

/// Sérialise une opération en CBOR.
pub fn encode_cbor(op: &Operation) -> Result<Vec<u8>, JournalError> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(op, &mut buf)
        .map_err(|e| JournalError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Désérialise une opération depuis CBOR.
pub fn decode_cbor(bytes: &[u8]) -> Result<Operation, JournalError> {
    ciborium::de::from_reader(bytes)
        .map_err(|e| JournalError::CborDecode(e.to_string()))
}

// ── Entrée de journal chiffrée ────────────────────────────────────────────────

/// Format d'une entrée persistée sur disque.
/// nonce (24 octets) || longueur ciphertext (8 octets LE) || ciphertext
#[derive(Debug)]
pub struct JournalEntry {
    pub blob: EncryptedBlob,
}

impl JournalEntry {
    /// Chiffre une opération avec la DEK → entrée prête à écrire sur disque.
    pub fn seal(op: &Operation, dek: &Dek) -> Result<Self, JournalError> {
        let cbor = encode_cbor(op)?;
        Ok(Self { blob: encrypt(&cbor, dek) })
    }

    /// Déchiffre et désérialise une entrée → opération en clair.
    pub fn open(&self, dek: &Dek) -> Result<Operation, JournalError> {
        let cbor = decrypt(&self.blob, dek)?;
        decode_cbor(&cbor)
    }

    /// Sérialise en octets pour écriture disque (nonce || len || ciphertext).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.blob.nonce);
        let len = self.blob.ciphertext.len() as u64;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.blob.ciphertext);
        out
    }

    /// Désérialise depuis un slice d'octets. Retourne (entrée, octets_consommés).
    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        const HEADER: usize = 24 + 8; // nonce + longueur
        if bytes.len() < HEADER {
            return None;
        }
        let nonce: [u8; 24] = bytes[..24].try_into().ok()?;
        let len = u64::from_le_bytes(bytes[24..32].try_into().ok()?) as usize;
        if bytes.len() < HEADER + len {
            return None;
        }
        let ciphertext = bytes[HEADER..HEADER + len].to_vec();
        Some((
            Self { blob: EncryptedBlob { nonce, ciphertext } },
            HEADER + len,
        ))
    }
}

// ── Journal en mémoire (spike) ────────────────────────────────────────────────

/// Journal append-only en mémoire pour le spike.
/// En production, les entrées seraient écrites sur disque (fichier ou PostgreSQL WAL).
pub struct MemoryJournal {
    entries: Vec<JournalEntry>,
    next_seq: u64,
}

impl MemoryJournal {
    pub fn new() -> Self {
        Self { entries: Vec::new(), next_seq: 1 }
    }

    /// Ajoute une opération au journal. Vérifie la séquence.
    pub fn append(&mut self, op: &Operation, dek: &Dek) -> Result<(), JournalError> {
        if op.seq != self.next_seq {
            return Err(JournalError::BadSequence {
                expected: self.next_seq,
                got: op.seq,
            });
        }
        let entry = JournalEntry::seal(op, dek)?;
        self.entries.push(entry);
        self.next_seq += 1;
        Ok(())
    }

    /// Rejoue tout le journal → retourne les opérations déchiffrées dans l'ordre.
    pub fn replay(&self, dek: &Dek) -> Result<Vec<Operation>, JournalError> {
        self.entries.iter().map(|e| e.open(dek)).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for MemoryJournal {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::init;

    fn setup() -> Dek {
        init().expect("sodiumoxide init");
        Dek::generate()
    }

    #[test]
    fn operation_cbor_roundtrip() {
        let dek = setup();
        let op = Operation::new(
            1,
            OpType::Sale,
            Payload { item_id: "ITEM-001".into(), quantity: 3 },
        );
        let entry = JournalEntry::seal(&op, &dek).expect("seal");
        let op2 = entry.open(&dek).expect("open");
        assert_eq!(op.seq, op2.seq);
        assert_eq!(op.op_id, op2.op_id);
        assert_eq!(op.payload.item_id, op2.payload.item_id);
    }

    #[test]
    fn journal_append_et_replay() {
        let dek = setup();
        let mut journal = MemoryJournal::new();

        for i in 1..=5 {
            let op = Operation::new(
                i,
                OpType::Sale,
                Payload { item_id: "ITEM-A".into(), quantity: 1 },
            );
            journal.append(&op, &dek).expect("append");
        }

        let ops = journal.replay(&dek).expect("replay");
        assert_eq!(ops.len(), 5);
        // Vérifie que l'ordre est préservé
        for (i, op) in ops.iter().enumerate() {
            assert_eq!(op.seq, (i + 1) as u64);
        }
    }

    #[test]
    fn sequence_hors_ordre_rejetee() {
        let dek = setup();
        let mut journal = MemoryJournal::new();

        let op_seq1 = Operation::new(1, OpType::Sale, Payload { item_id: "X".into(), quantity: 1 });
        journal.append(&op_seq1, &dek).expect("seq 1 ok");

        // seq=3 alors qu'on attend seq=2 → rejeté
        let op_seq3 = Operation::new(3, OpType::Sale, Payload { item_id: "X".into(), quantity: 1 });
        assert!(journal.append(&op_seq3, &dek).is_err());
    }

    #[test]
    fn entree_serialisee_est_opaque() {
        let dek = setup();
        let op = Operation::new(
            1,
            OpType::Sale,
            Payload { item_id: "CONFIDENTIEL".into(), quantity: 99 },
        );
        let entry = JournalEntry::seal(&op, &dek).expect("seal");
        let bytes = entry.to_bytes();

        // Le contenu disque ne contient pas "CONFIDENTIEL" en clair
        let bytes_as_str = String::from_utf8_lossy(&bytes);
        assert!(
            !bytes_as_str.contains("CONFIDENTIEL"),
            "le journal sur disque ne doit contenir aucun clair"
        );
    }

    #[test]
    fn mauvaise_dek_ne_rejoue_pas() {
        let dek = setup();
        let dek_mauvaise = Dek::generate();
        let mut journal = MemoryJournal::new();

        let op = Operation::new(1, OpType::StockAdjust, Payload { item_id: "Y".into(), quantity: 10 });
        journal.append(&op, &dek).expect("append");

        assert!(journal.replay(&dek_mauvaise).is_err());
    }

    #[test]
    fn serialisation_entree_roundtrip() {
        let dek = setup();
        let op = Operation::new(1, OpType::Sale, Payload { item_id: "Z".into(), quantity: 2 });
        let entry = JournalEntry::seal(&op, &dek).expect("seal");
        let bytes = entry.to_bytes();

        let (entry2, consumed) = JournalEntry::from_bytes(&bytes).expect("from_bytes");
        assert_eq!(consumed, bytes.len());

        let op2 = entry2.open(&dek).expect("open");
        assert_eq!(op.seq, op2.seq);
        assert_eq!(op.payload.quantity, op2.payload.quantity);
    }
}
