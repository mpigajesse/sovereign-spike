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
use sha2::{Sha256, Digest};
use thiserror::Error;
use uuid::Uuid;

/// Nature de l'opération métier.
///
/// `Sale` / `StockAdjust` : opérations de stock (invariant fort anti-survente) — inchangées.
/// `*Upsert` / `*Delete`  : CRUD métier générique (produits, clients) — sans invariant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OpType {
    Sale,
    StockAdjust,
    ProduitUpsert,
    ProduitDelete,
    ClientUpsert,
    ClientDelete,
}

/// Contenu d'une opération de STOCK (item + quantité). Inchangé — l'anti-survente en dépend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub item_id:  String,
    pub quantity: i64,
}

/// Contenu d'une opération CRUD métier GÉNÉRIQUE.
///
/// Le moteur ne connaît pas la sémantique de `fields` (sku, nom, prix…) : il transporte
/// un simple dictionnaire de champs. Le nœud actif les interprète en les rangeant dans
/// le schéma `business`. C'est ce qui rend le moteur agnostique au domaine (framework).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessData {
    /// Identifiant de l'entité métier (UUID en texte) — clé de l'upsert/delete.
    pub entity_id: String,
    /// Champs métier (clé → valeur en texte). Vide pour un delete.
    pub fields: std::collections::BTreeMap<String, String>,
}

/// Une opération du journal (§0.1 Stack).
/// `seq` est imposé par le nœud actif — l'ordre unique, pièce maîtresse anti-survente.
/// `prev_hash` chaîne chaque entrée à la précédente — toute altération du passé est détectable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub seq:            u64,
    pub op_type:        OpType,
    pub op_id:          Uuid,
    pub ts:             i64,
    pub payload:        Payload,
    pub schema_version: u8,
    /// SHA-256 du CBOR de l'entrée précédente (hex). "genesis" pour la première entrée.
    pub prev_hash:      String,
    /// Données CRUD métier (produits, clients). `None` pour les opérations de stock.
    /// `skip_serializing_if` : une opération de stock se sérialise EXACTEMENT comme avant
    /// (aucun champ ajouté) → les blobs existants et la chaîne de hash restent valides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business:       Option<BusinessData>,
}

impl Operation {
    pub const CURRENT_SCHEMA: u8 = 1;
    pub const GENESIS_HASH: &'static str = "genesis";

    pub fn new(seq: u64, op_type: OpType, payload: Payload) -> Self {
        Self {
            seq,
            op_type,
            op_id: Uuid::new_v4(),
            ts: chrono::Utc::now().timestamp(),
            payload,
            schema_version: Self::CURRENT_SCHEMA,
            prev_hash: Self::GENESIS_HASH.to_string(),
            business: None,
        }
    }

    /// Construit une opération chaînée à l'entrée précédente.
    pub fn new_chained(seq: u64, op_type: OpType, payload: Payload, prev_cbor: &[u8]) -> Self {
        let hash = hex::encode(Sha256::digest(prev_cbor));
        Self {
            seq,
            op_type,
            op_id: Uuid::new_v4(),
            ts: chrono::Utc::now().timestamp(),
            payload,
            schema_version: Self::CURRENT_SCHEMA,
            prev_hash: hash,
            business: None,
        }
    }

    /// Construit une opération CRUD métier (produit/client), chaînée à la précédente.
    /// `payload` est neutre (item_id vide, quantity 0) — le contenu est dans `business`.
    pub fn new_business(seq: u64, op_type: OpType, business: BusinessData, prev_cbor: Option<&[u8]>) -> Self {
        let prev_hash = match prev_cbor {
            Some(cbor) => hex::encode(Sha256::digest(cbor)),
            None => Self::GENESIS_HASH.to_string(),
        };
        Self {
            seq,
            op_type,
            op_id: Uuid::new_v4(),
            ts: chrono::Utc::now().timestamp(),
            payload: Payload { item_id: String::new(), quantity: 0 },
            schema_version: Self::CURRENT_SCHEMA,
            prev_hash,
            business: Some(business),
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
    entries:     Vec<JournalEntry>,
    next_seq:    u64,
    last_cbor:   Option<Vec<u8>>, // CBOR de la dernière entrée → pour le hachage chaîné
}

impl MemoryJournal {
    pub fn new() -> Self {
        Self { entries: Vec::new(), next_seq: 1, last_cbor: None }
    }

    /// Ajoute une opération au journal. Vérifie la séquence et la chaîne de hachage.
    pub fn append(&mut self, op: &Operation, dek: &Dek) -> Result<(), JournalError> {
        if op.seq != self.next_seq {
            return Err(JournalError::BadSequence {
                expected: self.next_seq,
                got: op.seq,
            });
        }
        let cbor = encode_cbor(op)?;
        let entry = JournalEntry { blob: encrypt(&cbor, dek) };
        self.last_cbor = Some(cbor);
        self.entries.push(entry);
        self.next_seq += 1;
        Ok(())
    }

    /// CBOR de la dernière entrée — à passer à `Operation::new_chained` pour le hachage.
    pub fn last_cbor(&self) -> Option<&[u8]> {
        self.last_cbor.as_deref()
    }

    /// Rejoue tout le journal → retourne les opérations déchiffrées dans l'ordre.
    pub fn replay(&self, dek: &Dek) -> Result<Vec<Operation>, JournalError> {
        self.entries.iter().map(|e| e.open(dek)).collect()
    }

    /// Vérifie l'intégrité de la chaîne de hachage.
    /// Retourne `Ok(())` si chaque entrée référence correctement la précédente.
    pub fn verify_chain(&self, dek: &Dek) -> Result<(), JournalError> {
        let ops = self.replay(dek)?;
        let mut prev_cbor: Option<Vec<u8>> = None;

        for op in &ops {
            match &prev_cbor {
                None => {
                    // Première entrée : prev_hash doit être "genesis"
                    if op.prev_hash != Operation::GENESIS_HASH {
                        return Err(JournalError::BadSequence {
                            expected: 0,
                            got: op.seq,
                        });
                    }
                }
                Some(prev) => {
                    let expected_hash = hex::encode(Sha256::digest(prev));
                    if op.prev_hash != expected_hash {
                        return Err(JournalError::BadSequence {
                            expected: op.seq - 1,
                            got: op.seq,
                        });
                    }
                }
            }
            prev_cbor = Some(encode_cbor(op)?);
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for MemoryJournal {
    fn default() -> Self { Self::new() }
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
        for (i, op) in ops.iter().enumerate() {
            assert_eq!(op.seq, (i + 1) as u64);
        }
    }

    #[test]
    fn sequence_hors_ordre_rejetee() {
        let dek = setup();
        let mut journal = MemoryJournal::new();

        let op1 = Operation::new(1, OpType::Sale, Payload { item_id: "X".into(), quantity: 1 });
        journal.append(&op1, &dek).expect("seq 1 ok");

        let op3 = Operation::new(3, OpType::Sale, Payload { item_id: "X".into(), quantity: 1 });
        assert!(journal.append(&op3, &dek).is_err());
    }

    #[test]
    fn hash_chaine_verifie_integrite() {
        let dek = setup();
        let mut journal = MemoryJournal::new();

        // Première entrée : genesis
        let op1 = Operation::new(1, OpType::StockAdjust, Payload { item_id: "A".into(), quantity: 100 });
        journal.append(&op1, &dek).expect("append 1");

        // Entrées suivantes : chaînées
        for seq in 2u64..=4 {
            let prev = journal.last_cbor().unwrap().to_vec();
            let op = Operation::new_chained(seq, OpType::Sale, Payload { item_id: "A".into(), quantity: 1 }, &prev);
            journal.append(&op, &dek).expect("append chaîné");
        }

        // La chaîne doit être valide
        journal.verify_chain(&dek).expect("chaîne intègre");
    }

    #[test]
    fn hash_chaine_detecte_alteration() {
        let dek = setup();
        let mut journal = MemoryJournal::new();

        // Construire un journal avec 2 entrées chaînées correctement
        let op1 = Operation::new(1, OpType::StockAdjust, Payload { item_id: "B".into(), quantity: 50 });
        journal.append(&op1, &dek).expect("append 1");
        let prev = journal.last_cbor().unwrap().to_vec();
        let op2 = Operation::new_chained(2, OpType::Sale, Payload { item_id: "B".into(), quantity: 5 }, &prev);
        journal.append(&op2, &dek).expect("append 2");

        journal.verify_chain(&dek).expect("chaîne valide avant altération");

        // Simuler une altération : construire une 3e entrée avec un faux prev_hash
        let mut op3_falsifie = Operation::new(3, OpType::Sale, Payload { item_id: "B".into(), quantity: 999 });
        op3_falsifie.prev_hash = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        journal.append(&op3_falsifie, &dek).expect("append 3 falsifié");

        // La vérification doit détecter l'altération
        assert!(journal.verify_chain(&dek).is_err(), "altération doit être détectée");
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
    fn operation_business_crud_journalisee_et_chainee() {
        // LOT 2 : une opération CRUD métier est chiffrée, journalisée, et la chaîne de
        // hash reste valide même mélangée à des opérations de stock.
        let dek = setup();
        let mut journal = MemoryJournal::new();

        // 1. stock (genesis)
        let op1 = Operation::new(1, OpType::StockAdjust, Payload { item_id: "PROD-1".into(), quantity: 10 });
        journal.append(&op1, &dek).expect("append stock");

        // 2. création produit (CRUD métier, chaînée)
        let prev = journal.last_cbor().unwrap().to_vec();
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("sku".to_string(), "PROD-1".to_string());
        fields.insert("nom".to_string(), "Pantalon".to_string());
        fields.insert("prix_cents".to_string(), "1499".to_string());
        let op2 = Operation::new_business(2, OpType::ProduitUpsert,
            BusinessData { entity_id: "prod-uuid-1".into(), fields }, Some(&prev));
        journal.append(&op2, &dek).expect("append produit");

        // 3. vente (stock, chaînée)
        let prev = journal.last_cbor().unwrap().to_vec();
        let op3 = Operation::new_chained(3, OpType::Sale, Payload { item_id: "PROD-1".into(), quantity: 2 }, &prev);
        journal.append(&op3, &dek).expect("append vente");

        // La chaîne reste intègre malgré le mélange stock + CRUD
        journal.verify_chain(&dek).expect("chaîne intègre stock+CRUD");

        // Le contenu métier est bien récupérable au rejeu
        let ops = journal.replay(&dek).expect("replay");
        let produit = &ops[1];
        assert_eq!(produit.op_type, OpType::ProduitUpsert);
        let biz = produit.business.as_ref().expect("données business présentes");
        assert_eq!(biz.fields.get("nom").map(String::as_str), Some("Pantalon"));
        assert_eq!(biz.fields.get("prix_cents").map(String::as_str), Some("1499"));

        // Une opération de stock n'a PAS de données business
        assert!(ops[0].business.is_none(), "une op de stock ne porte pas de business");
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
