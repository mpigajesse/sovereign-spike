//! Jetons de licence signés — autorité souveraine de licence (licence « soft »).
//!
//! Le service éditeur `sovereign-control` (distinct du relais aveugle) détient la clé
//! privée Ed25519 et signe des jetons de licence. Le nœud actif vérifie ces jetons
//! localement avec la seule clé publique de l'autorité — sans jamais contacter le
//! control-plane à l'exécution.
//!
//! Propriété clé : la vérification ne dépend QUE de la clé publique (connue à l'avance,
//! distribuée avec le binaire ou la config du nœud). Aucun appel réseau n'est requis
//! pour faire fonctionner le coffre — seule l'émission de nouvelles licences l'exige.
//! C'est ce qui rend la licence « soft » : un blocage de licence ne bloque jamais l'accès
//! aux données, seulement les mises à jour / le support.
//!
//! Primitive : Ed25519 (signature) via libsodium / sodiumoxide — jamais réimplémentée.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sodiumoxide::crypto::sign::ed25519::{self, PublicKey, SecretKey, Signature};

/// Revendications portées par un jeton de licence — le payload signé.
/// Sérialisées en CBOR avant signature (même format que le journal souverain) :
/// schéma partagé entre l'émetteur (`sovereign-control`) et le vérificateur (nœud actif),
/// pour qu'aucun des deux ne puisse dériver silencieusement de l'autre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseClaims {
    pub tenant_id:  String,
    pub plan:       String,
    pub issued_at:  DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Jeton de licence : payload + signature + clé publique de vérification, en hex —
/// autoporté, prêt pour le transport HTTP et le stockage local au nœud actif.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseToken {
    pub payload_hex:          String,
    pub signature_hex:        String,
    pub authority_pubkey_hex: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("sérialisation CBOR du jeton échouée : {0}")]
    Serialize(String),
    #[error("jeton illisible : signature invalide ou payload corrompu")]
    Invalid,
}

/// Émet un jeton de licence signé pour les revendications fournies.
/// Réservé à l'autorité (`sovereign-control`) — exige la clé privée.
pub fn issue_token(
    authority: &LicenseAuthorityKeypair,
    claims: &LicenseClaims,
) -> Result<LicenseToken, LicenseError> {
    let mut payload = Vec::new();
    ciborium::ser::into_writer(claims, &mut payload)
        .map_err(|e| LicenseError::Serialize(e.to_string()))?;

    Ok(LicenseToken {
        signature_hex:        authority.sign_hex(&payload),
        payload_hex:          hex::encode(&payload),
        authority_pubkey_hex: authority.public_hex(),
    })
}

/// Vérifie la signature d'un jeton avec la clé publique de l'autorité ATTENDUE
/// (jamais celle embarquée dans le jeton — sinon n'importe qui pourrait s'auto-signer)
/// puis décode ses revendications. Vérification 100% locale, hors-ligne.
pub fn open_token(token: &LicenseToken, expected_authority_pubkey_hex: &str) -> Result<LicenseClaims, LicenseError> {
    if !verify_hex(&token.payload_hex, &token.signature_hex, expected_authority_pubkey_hex) {
        return Err(LicenseError::Invalid);
    }
    let payload = hex::decode(token.payload_hex.trim()).map_err(|_| LicenseError::Invalid)?;
    ciborium::de::from_reader(&payload[..]).map_err(|_| LicenseError::Invalid)
}

/// Paire de clés Ed25519 de l'autorité de licence (détenue par l'éditeur, jamais par le client).
pub struct LicenseAuthorityKeypair {
    pub public:  PublicKey,
    pub private: SecretKey,
}

impl LicenseAuthorityKeypair {
    /// Génère une nouvelle paire de clés Ed25519 pour l'autorité de licence.
    pub fn generate() -> Self {
        let (public, private) = ed25519::gen_keypair();
        Self { public, private }
    }

    /// Reconstitue la paire depuis sa clé privée brute en hex (64 octets : graine + clé publique).
    /// Permet de recharger une autorité persistante (secret manager, fichier protégé) au démarrage
    /// de `sovereign-control`, plutôt que d'en régénérer une à chaque redémarrage.
    pub fn from_secret_hex(secret_hex: &str) -> Option<Self> {
        let bytes = hex::decode(secret_hex.trim()).ok()?;
        let private = SecretKey::from_slice(&bytes)?;
        let public = private.public_key();
        Some(Self { public, private })
    }

    /// Clé publique de l'autorité en hex — à distribuer aux nœuds actifs pour vérification locale.
    pub fn public_hex(&self) -> String {
        hex::encode(self.public.as_ref())
    }

    /// Clé privée brute en hex — à confier à un secret manager, jamais au code source ni aux logs.
    pub fn secret_hex(&self) -> String {
        hex::encode(self.private.as_ref())
    }

    /// Signe un jeton de licence (octets bruts du payload sérialisé : tenant, plan, expiration…).
    /// Retourne une signature détachée — le payload reste lisible séparément du sceau.
    pub fn sign(&self, payload: &[u8]) -> Signature {
        ed25519::sign_detached(payload, &self.private)
    }

    /// Signe et renvoie la signature détachée en hex (transport / stockage local au nœud).
    pub fn sign_hex(&self, payload: &[u8]) -> String {
        hex::encode(self.sign(payload).as_ref())
    }
}

/// Reconstitue la clé publique de l'autorité depuis du hex (config du nœud actif).
pub fn authority_public_key_from_hex(s: &str) -> Option<PublicKey> {
    PublicKey::from_slice(&hex::decode(s.trim()).ok()?)
}

/// Vérifie une signature détachée Ed25519 sur un payload, avec la clé publique de l'autorité.
/// Vérification 100% locale — aucune dépendance réseau, aucun appel à `sovereign-control`.
pub fn verify(payload: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    ed25519::verify_detached(signature, payload, public_key)
}

/// Vérifie un payload et une signature fournis en hex (frontière HTTP / stockage local).
/// Renvoie `false` sur toute entrée malformée — jamais de panique sur des données externes.
pub fn verify_hex(payload_hex: &str, signature_hex: &str, authority_pubkey_hex: &str) -> bool {
    let Some(public_key) = authority_public_key_from_hex(authority_pubkey_hex) else {
        return false;
    };
    let Ok(payload) = hex::decode(payload_hex.trim()) else {
        return false;
    };
    let Some(signature) = hex::decode(signature_hex.trim())
        .ok()
        .and_then(|bytes| Signature::from_bytes(&bytes).ok())
    else {
        return false;
    };
    verify(&payload, &signature, &public_key)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::init;

    fn setup() {
        init().expect("sodiumoxide init");
    }

    const PAYLOAD: &[u8] = b"tenant=al-baraa-demo;plan=pro;expire=2026-12-31";

    fn claims_de_test() -> LicenseClaims {
        let maintenant = Utc::now();
        LicenseClaims {
            tenant_id:  "al-baraa-demo".to_string(),
            plan:       "pro".to_string(),
            issued_at:  maintenant,
            expires_at: maintenant + chrono::Duration::days(365),
        }
    }

    #[test]
    fn emet_et_ouvre_un_jeton_avec_la_cle_attendue() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let claims = claims_de_test();

        let jeton = issue_token(&autorite, &claims).expect("émission");
        let revendications = open_token(&jeton, &autorite.public_hex()).expect("ouverture avec la clé attendue");

        assert_eq!(revendications, claims, "les revendications ouvertes doivent correspondre à l'émission");
    }

    #[test]
    fn open_token_rejette_une_cle_attendue_differente_de_celle_du_signataire() {
        // Sécurité critique : la vérification doit utiliser la clé CONFIGURÉE par le nœud,
        // jamais celle embarquée dans le jeton — sinon un attaquant pourrait forger un jeton
        // et y embarquer SA PROPRE clé publique pour s'auto-valider.
        setup();
        let autorite_legitime = LicenseAuthorityKeypair::generate();
        let autorite_attaquant = LicenseAuthorityKeypair::generate();
        let claims = claims_de_test();

        let jeton_legitime = issue_token(&autorite_legitime, &claims).expect("émission légitime");
        assert!(open_token(&jeton_legitime, &autorite_attaquant.public_hex()).is_err(),
            "un nœud configuré avec une AUTRE clé attendue doit rejeter le jeton, même signé correctement par sa propre autorité");

        // Le jeton forgé embarque la clé de l'attaquant — mais le nœud ne lui fait pas confiance.
        let jeton_forge = issue_token(&autorite_attaquant, &claims).expect("émission forgée");
        assert!(open_token(&jeton_forge, &autorite_legitime.public_hex()).is_err(),
            "un jeton auto-signé par un attaquant doit échouer face à la clé de confiance du nœud");
    }

    #[test]
    fn open_token_rejette_un_jeton_dont_le_payload_a_ete_altere() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let mut jeton = issue_token(&autorite, &claims_de_test()).expect("émission");

        // On bidouille le payload (ex: tentative d'extension de la durée de validité) sans re-signer.
        let mut payload = hex::decode(&jeton.payload_hex).expect("payload hex");
        payload[0] ^= 0xFF;
        jeton.payload_hex = hex::encode(&payload);

        assert!(open_token(&jeton, &autorite.public_hex()).is_err(),
            "un payload altéré doit invalider la signature et empêcher l'ouverture");
    }

    #[test]
    fn signe_et_verifie_un_jeton_valide() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();

        let signature = autorite.sign(PAYLOAD);
        assert!(
            verify(PAYLOAD, &signature, &autorite.public),
            "une signature valide doit être acceptée par la clé publique correspondante"
        );
    }

    #[test]
    fn restaure_lautorite_depuis_sa_cle_privee_hex() {
        setup();
        let originale = LicenseAuthorityKeypair::generate();
        let secret_hex = originale.secret_hex();

        let restauree = LicenseAuthorityKeypair::from_secret_hex(&secret_hex)
            .expect("restauration depuis la clé privée hex");

        assert_eq!(restauree.public_hex(), originale.public_hex(),
            "la clé publique doit être dérivée correctement de la clé privée restaurée");

        let signature = restauree.sign(PAYLOAD);
        assert!(verify(PAYLOAD, &signature, &originale.public),
            "une autorité restaurée doit signer de façon vérifiable avec la clé publique d'origine");
    }

    #[test]
    fn from_secret_hex_rejette_les_entrees_invalides() {
        setup();
        assert!(LicenseAuthorityKeypair::from_secret_hex("pas-du-hex").is_none());
        assert!(LicenseAuthorityKeypair::from_secret_hex("aabb").is_none(), "trop court pour une clé Ed25519");
    }

    #[test]
    fn rejette_un_payload_modifie() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let signature = autorite.sign(PAYLOAD);

        let payload_modifie = b"tenant=al-baraa-demo;plan=enterprise;expire=2026-12-31";
        assert!(
            !verify(payload_modifie, &signature, &autorite.public),
            "toute modification du payload doit invalider la signature (intégrité)"
        );
    }

    #[test]
    fn rejette_une_signature_dune_autre_autorite() {
        setup();
        let autorite_legitime = LicenseAuthorityKeypair::generate();
        let autre_autorite = LicenseAuthorityKeypair::generate();

        let signature_usurpee = autre_autorite.sign(PAYLOAD);
        assert!(
            !verify(PAYLOAD, &signature_usurpee, &autorite_legitime.public),
            "une signature émise par une autre clé privée doit être rejetée (authenticité)"
        );
    }

    #[test]
    fn roundtrip_hex_signe_et_verifie() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let payload_hex = hex::encode(PAYLOAD);

        let signature_hex = autorite.sign_hex(PAYLOAD);
        assert!(
            verify_hex(&payload_hex, &signature_hex, &autorite.public_hex()),
            "le roundtrip hex (transport HTTP / stockage local au nœud) doit valider une signature correcte"
        );
    }

    #[test]
    fn verify_hex_rejette_les_entrees_invalides() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let payload_hex = hex::encode(PAYLOAD);
        let signature_hex = autorite.sign_hex(PAYLOAD);

        assert!(!verify_hex(&payload_hex, &signature_hex, "pas-du-hex"), "clé publique invalide → false, pas de panique");
        assert!(!verify_hex(&payload_hex, "zz", &autorite.public_hex()), "signature hex invalide → false, pas de panique");
        assert!(!verify_hex("zz", &signature_hex, &autorite.public_hex()), "payload hex invalide → false, pas de panique");
    }
}
