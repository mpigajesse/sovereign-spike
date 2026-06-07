//! Couche cryptographique du spike souverain.
//!
//! Hiérarchie de clés (§5.4 Phase 0) :
//!   DEK (symétrique, unique par entreprise)
//!     └── emballée via sealed box vers chaque paire X25519 d'appareil
//!
//! Primitives : libsodium via sodiumoxide
//!   - Chiffrement données/journal : XChaCha20-Poly1305 (AEAD)
//!   - Identité appareil            : X25519 (box_)
//!   - Dérivation depuis passphrase : Argon2id
//!   - Enrôlement                   : sealed box

use sodiumoxide::crypto::{
    aead::xchacha20poly1305_ietf::{self, Key as AeadKey, Nonce, KEYBYTES, NONCEBYTES},
    box_::{self, PublicKey, SecretKey},
    pwhash::argon2id13::{self, Salt, SALTBYTES, MEMLIMIT_INTERACTIVE, OPSLIMIT_INTERACTIVE},
    sealedbox,
};
use thiserror::Error;

/// Clé de chiffrement des données (Data Encryption Key).
/// Symétrique, unique par entreprise. Chiffre tout le contenu métier et le journal.
#[derive(Clone)]
pub struct Dek(pub(crate) AeadKey);

/// Paire de clés X25519 d'un appareil.
pub struct DeviceKeypair {
    pub public:  PublicKey,
    pub private: SecretKey,
}

/// Blob chiffré avec la DEK (XChaCha20-Poly1305).
/// Contient le nonce + le tag d'authentification + le ciphertext.
#[derive(Debug, Clone)]
pub struct EncryptedBlob {
    pub nonce:      [u8; NONCEBYTES],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("déchiffrement échoué : données corrompues ou mauvaise clé")]
    DecryptFailed,
    #[error("ouverture sealed box échouée : mauvaise clé privée ou blob corrompu")]
    SealedBoxFailed,
    #[error("dérivation de clé échouée")]
    KdfFailed,
    #[error("sodiumoxide non initialisé")]
    InitFailed,
}

/// Initialise libsodium. À appeler une fois au démarrage.
pub fn init() -> Result<(), CryptoError> {
    sodiumoxide::init().map_err(|_| CryptoError::InitFailed)
}

// ── DEK ─────────────────────────────────────────────────────────────────────

impl Dek {
    /// Génère une DEK aléatoire fraîche.
    pub fn generate() -> Self {
        Self(xchacha20poly1305_ietf::gen_key())
    }

    /// Reconstruit une DEK depuis des octets bruts (après unwrap d'une sealed box).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        AeadKey::from_slice(bytes).map(Self)
    }

    /// Expose les octets bruts de la DEK (pour les emballer dans une sealed box).
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

// ── Chiffrement symétrique ───────────────────────────────────────────────────

/// Chiffre `plaintext` avec la DEK. Retourne un blob opaque (nonce + ciphertext + tag).
pub fn encrypt(plaintext: &[u8], dek: &Dek) -> EncryptedBlob {
    let nonce = xchacha20poly1305_ietf::gen_nonce();
    let ciphertext = xchacha20poly1305_ietf::seal(plaintext, None, &nonce, &dek.0);
    EncryptedBlob {
        nonce: nonce.0,
        ciphertext,
    }
}

/// Déchiffre et vérifie l'intégrité d'un blob. Échoue si la DEK ou les données sont mauvaises.
pub fn decrypt(blob: &EncryptedBlob, dek: &Dek) -> Result<Vec<u8>, CryptoError> {
    let nonce = Nonce(blob.nonce);
    xchacha20poly1305_ietf::open(&blob.ciphertext, None, &nonce, &dek.0)
        .map_err(|_| CryptoError::DecryptFailed)
}

// ── Clés d'appareil (X25519) ─────────────────────────────────────────────────

impl DeviceKeypair {
    /// Génère une nouvelle paire de clés X25519 pour un appareil.
    pub fn generate() -> Self {
        let (public, private) = box_::gen_keypair();
        Self { public, private }
    }
}

// ── Enrôlement : sealed box ──────────────────────────────────────────────────

/// Emballe la DEK pour un appareil cible (sealed box vers sa clé publique).
/// Le relais éditeur ne peut pas ouvrir ce blob — il ne détient pas la clé privée.
pub fn wrap_dek(dek: &Dek, device_pubkey: &PublicKey) -> Vec<u8> {
    sealedbox::seal(dek.as_bytes(), device_pubkey)
}

/// Ouvre une sealed box avec la clé privée de l'appareil → récupère la DEK.
pub fn unwrap_dek(
    sealed_blob: &[u8],
    device_pubkey: &PublicKey,
    device_privkey: &SecretKey,
) -> Result<Dek, CryptoError> {
    let dek_bytes = sealedbox::open(sealed_blob, device_pubkey, device_privkey)
        .map_err(|_| CryptoError::SealedBoxFailed)?;
    Dek::from_bytes(&dek_bytes).ok_or(CryptoError::SealedBoxFailed)
}

// ── Code de récupération (Argon2id) ──────────────────────────────────────────

/// Dérive une clé de chiffrement depuis une passphrase (code de récupération).
/// Argon2id : volontairement lente et gourmande en mémoire → anti-bruteforce.
pub fn derive_key_from_passphrase(passphrase: &[u8], salt: &[u8; SALTBYTES]) -> Result<Dek, CryptoError> {
    let argon_salt = Salt(*salt);
    let mut key_bytes = [0u8; KEYBYTES];
    argon2id13::derive_key(
        &mut key_bytes,
        passphrase,
        &argon_salt,
        OPSLIMIT_INTERACTIVE,
        MEMLIMIT_INTERACTIVE,
    )
    .map_err(|_| CryptoError::KdfFailed)?;
    AeadKey::from_slice(&key_bytes)
        .map(Dek)
        .ok_or(CryptoError::KdfFailed)
}

/// Génère un sel aléatoire pour Argon2id (à stocker en clair avec la DEK chiffrée).
pub fn gen_kdf_salt() -> [u8; SALTBYTES] {
    let s = argon2id13::gen_salt();
    s.0
}

// ── Helpers hex pour les couches HTTP / UI (nœud actif, Tauri) ────────────────
//
// Le nœud actif et l'app Tauri ne manipulent que des chaînes hex et des `Dek` :
// ces helpers encapsulent les types sodiumoxide (PublicKey/SecretKey) pour éviter
// de les exposer hors du cœur.

impl DeviceKeypair {
    /// Clé publique de l'appareil en hex (le « QR » à présenter à l'enrôlement).
    pub fn public_hex(&self) -> String {
        hex::encode(self.public.as_ref())
    }

    /// Ouvre une sealed box hex reçue de l'appareil enrôleur → DEK en hex.
    /// Preuve du critère #8 : l'appareil récupère la clé avec sa seule clé privée.
    pub fn unwrap_dek_hex(&self, sealed_hex: &str) -> Result<String, CryptoError> {
        let sealed = hex::decode(sealed_hex.trim()).map_err(|_| CryptoError::SealedBoxFailed)?;
        let dek = unwrap_dek(&sealed, &self.public, &self.private)?;
        Ok(hex::encode(dek.as_bytes()))
    }
}

/// Parse une clé publique X25519 depuis du hex (32 octets).
pub fn public_key_from_hex(s: &str) -> Option<PublicKey> {
    PublicKey::from_slice(&hex::decode(s.trim()).ok()?)
}

/// Emballe la DEK pour une clé publique d'appareil fournie en hex → sealed box hex.
/// Utilisé à l'enrôlement et à chaque rotation (re-scellement pour les appareils restants).
pub fn wrap_dek_hex(dek: &Dek, device_pubkey_hex: &str) -> Option<String> {
    let pk = public_key_from_hex(device_pubkey_hex)?;
    Some(hex::encode(wrap_dek(dek, &pk)))
}

/// Tente de déchiffrer un blob (nonce + ciphertext hex) avec une DEK hex.
/// Renvoie `true` si la DEK est la bonne. Sert à PROUVER le critère #9 :
/// l'ancienne DEK d'un appareil dé-enrôlé échoue sur un blob écrit après rotation.
pub fn try_decrypt_hex(dek_hex: &str, nonce_hex: &str, ct_hex: &str) -> bool {
    let Some(dek) = hex::decode(dek_hex.trim()).ok().and_then(|b| Dek::from_bytes(&b)) else {
        return false;
    };
    let Ok(nonce_v) = hex::decode(nonce_hex.trim()) else { return false };
    if nonce_v.len() != NONCEBYTES {
        return false;
    }
    let mut nonce = [0u8; NONCEBYTES];
    nonce.copy_from_slice(&nonce_v);
    let Ok(ciphertext) = hex::decode(ct_hex.trim()) else { return false };
    decrypt(&EncryptedBlob { nonce, ciphertext }, &dek).is_ok()
}

// ── Code de récupération autoporté (salt + nonce + ciphertext) ────────────────

/// DEK emballée sous une passphrase (Argon2id) — tout le nécessaire pour restaurer.
pub struct RecoveryBlob {
    pub salt:       Vec<u8>,
    pub nonce:      Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Emballe la DEK sous une passphrase de récupération (à imprimer « au coffre »).
pub fn recovery_wrap(dek: &Dek, passphrase: &[u8]) -> Result<RecoveryBlob, CryptoError> {
    let salt = gen_kdf_salt();
    let kek = derive_key_from_passphrase(passphrase, &salt)?;
    let blob = encrypt(dek.as_bytes(), &kek);
    Ok(RecoveryBlob {
        salt:       salt.to_vec(),
        nonce:      blob.nonce.to_vec(),
        ciphertext: blob.ciphertext,
    })
}

/// Restaure la DEK depuis la passphrase et le blob de récupération (critère #11).
pub fn recovery_unwrap(blob: &RecoveryBlob, passphrase: &[u8]) -> Result<Dek, CryptoError> {
    if blob.salt.len() != SALTBYTES || blob.nonce.len() != NONCEBYTES {
        return Err(CryptoError::KdfFailed);
    }
    let mut salt = [0u8; SALTBYTES];
    salt.copy_from_slice(&blob.salt);
    let kek = derive_key_from_passphrase(passphrase, &salt)?;
    let mut nonce = [0u8; NONCEBYTES];
    nonce.copy_from_slice(&blob.nonce);
    let dek_bytes = decrypt(
        &EncryptedBlob { nonce, ciphertext: blob.ciphertext.clone() },
        &kek,
    )
    .map_err(|_| CryptoError::DecryptFailed)?;
    Dek::from_bytes(&dek_bytes).ok_or(CryptoError::DecryptFailed)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        init().expect("sodiumoxide init");
    }

    #[test]
    fn chiffrement_dechiffrement_roundtrip() {
        setup();
        let dek = Dek::generate();
        let message = b"stock=42, facture=0001";
        let blob = encrypt(message, &dek);
        let plaintext = decrypt(&blob, &dek).expect("dechiffrement");
        assert_eq!(plaintext, message);
    }

    #[test]
    fn mauvaise_dek_ne_dechiffre_pas() {
        setup();
        let dek1 = Dek::generate();
        let dek2 = Dek::generate();
        let blob = encrypt(b"donnee sensible", &dek1);
        assert!(decrypt(&blob, &dek2).is_err());
    }

    #[test]
    fn blob_altere_detecte() {
        setup();
        let dek = Dek::generate();
        let mut blob = encrypt(b"donnee integre", &dek);
        blob.ciphertext[0] ^= 0xFF; // corruption
        assert!(decrypt(&blob, &dek).is_err());
    }

    #[test]
    fn enrolement_wrap_unwrap_dek() {
        setup();
        // Appareil 1 génère la DEK (nœud actif initial)
        let dek_original = Dek::generate();

        // Appareil 2 génère sa paire X25519 et présente sa clé publique (QR)
        let appareil2 = DeviceKeypair::generate();

        // Appareil 1 emballe la DEK pour l'appareil 2
        let sealed = wrap_dek(&dek_original, &appareil2.public);

        // Appareil 2 ouvre la sealed box avec sa clé privée
        let dek_recue = unwrap_dek(&sealed, &appareil2.public, &appareil2.private)
            .expect("appareil2 doit recuperer la DEK");

        // Vérifie que l'appareil 2 peut déchiffrer une donnée chiffrée par l'appareil 1
        let message = b"invariant: stock ne peut pas etre negatif";
        let blob = encrypt(message, &dek_original);
        let plaintext = decrypt(&blob, &dek_recue).expect("dechiffrement avec DEK recue");
        assert_eq!(plaintext, message);
    }

    #[test]
    fn relais_ne_peut_pas_dechiffrer() {
        setup();
        let dek = Dek::generate();
        let blob = encrypt(b"donnee confidentielle PME", &dek);

        // Le relais reçoit uniquement des octets opaques — il n'a pas la DEK
        // Simuler une tentative avec une DEK aléatoire (comme si le relais essayait)
        let dek_relais = Dek::generate();
        assert!(decrypt(&blob, &dek_relais).is_err(), "relais aveugle : doit echouer");
    }

    #[test]
    fn code_de_recuperation_argon2id() {
        use sodiumoxide::crypto::pwhash::argon2id13::SALTBYTES;
        setup();
        let passphrase = b"phrase-secrete-gerant-coffre-fort";
        let salt: [u8; SALTBYTES] = gen_kdf_salt();

        let dek1 = derive_key_from_passphrase(passphrase, &salt).expect("derivation");
        let dek2 = derive_key_from_passphrase(passphrase, &salt).expect("derivation reproducible");

        // Même passphrase + même sel → même clé (déterministe)
        assert_eq!(dek1.as_bytes(), dek2.as_bytes());

        // On peut chiffrer/déchiffrer avec la clé dérivée
        let msg = b"archive comptable 2025";
        let blob = encrypt(msg, &dek1);
        let plain = decrypt(&blob, &dek2).expect("dechiffrement avec cle derivee");
        assert_eq!(plain, msg);
    }

    // ── Helpers runtime (#8/#9/#11) ──────────────────────────────────────────

    #[test]
    fn wrap_unwrap_dek_hex_roundtrip() {
        // #8 : l'appareil récupère la DEK via sa clé privée, à partir de hex seulement.
        setup();
        let dek = Dek::generate();
        let appareil = DeviceKeypair::generate();

        let sealed_hex = wrap_dek_hex(&dek, &appareil.public_hex()).expect("wrap hex");
        let dek_hex = appareil.unwrap_dek_hex(&sealed_hex).expect("unwrap hex");
        assert_eq!(dek_hex, hex::encode(dek.as_bytes()));
    }

    #[test]
    fn wrap_dek_hex_rejette_cle_invalide() {
        setup();
        let dek = Dek::generate();
        assert!(wrap_dek_hex(&dek, "pas-du-hex").is_none());
        assert!(wrap_dek_hex(&dek, "aabb").is_none()); // trop court
    }

    #[test]
    fn try_decrypt_hex_distingue_bonne_et_mauvaise_dek() {
        // #9 : un blob écrit avec la DEK v2 ne se déchiffre pas avec la DEK v1 (dé-enrôlé).
        setup();
        let dek_v1 = Dek::generate();
        let dek_v2 = Dek::generate();
        let blob = encrypt(b"vente apres rotation", &dek_v2);
        let nonce_hex = hex::encode(blob.nonce);
        let ct_hex = hex::encode(&blob.ciphertext);

        assert!(try_decrypt_hex(&hex::encode(dek_v2.as_bytes()), &nonce_hex, &ct_hex),
            "DEK courante doit déchiffrer");
        assert!(!try_decrypt_hex(&hex::encode(dek_v1.as_bytes()), &nonce_hex, &ct_hex),
            "DEK périmée (appareil dé-enrôlé) doit échouer");
        assert!(!try_decrypt_hex("zz", &nonce_hex, &ct_hex), "hex invalide → false, pas de panique");
    }

    #[test]
    fn recovery_wrap_unwrap_roundtrip() {
        // #11 : restauration de la DEK depuis la seule passphrase.
        setup();
        let dek = Dek::generate();
        let passphrase = b"gerant-coffre-haute-entropie-2026";

        let blob = recovery_wrap(&dek, passphrase).expect("emballage récupération");
        let restored = recovery_unwrap(&blob, passphrase).expect("restauration");
        assert_eq!(restored.as_bytes(), dek.as_bytes());
    }

    #[test]
    fn recovery_mauvaise_passphrase_echoue() {
        setup();
        let dek = Dek::generate();
        let blob = recovery_wrap(&dek, b"bonne-passphrase-secrete").expect("emballage");
        assert!(recovery_unwrap(&blob, b"mauvaise-passphrase-xxx").is_err(),
            "une passphrase incorrecte ne doit jamais restaurer la DEK");
    }
}
