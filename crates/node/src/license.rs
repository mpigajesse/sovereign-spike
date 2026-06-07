//! Vérification locale de la licence — licence « soft » (§ décisions licence).
//!
//! Le nœud actif vérifie hors-ligne un jeton de licence signé Ed25519 par l'autorité
//! éditeur (`sovereign-control`), avec la SEULE clé publique configurée localement
//! (jamais celle embarquée dans le jeton — voir `sovereign_core::license::open_token`).
//! Aucun appel réseau n'est requis pour fonctionner : le coffre reste pleinement
//! opérationnel hors-ligne, y compris avec une licence expirée ou absente.
//!
//! Propriété cardinale, vérifiable en lisant ce module : AUCUN handler ici ne peut
//! bloquer `/write`, `/stock`, `/journal` ou toute autre route métier. Une licence
//! invalide/expirée ne produit qu'une bannière d'information côté tableau de bord —
//! jamais une perte d'accès aux données. C'est tout le sens de « licence souveraine ».
//!
//! Variables d'environnement :
//!   LICENSE_AUTHORITY_PUBKEY_HEX — clé publique Ed25519 de l'autorité éditeur (hex,
//!                                  32 octets), distribuée hors-bande avec le binaire ;
//!                                  absente ⇒ vérification désactivée (mode autonome)
//!   LICENSE_TOKEN_PATH           — chemin du jeton signé au format JSON
//!                                  (défaut : "./license.json")

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sovereign_core::license::{open_token, LicenseClaims, LicenseToken};

/// État de licence exposé au tableau de bord. Purement informatif : ne conditionne
/// jamais l'exécution d'une opération métier (cf. propriété cardinale ci-dessus).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LicenseStatus {
    /// Jeton présent, signature vérifiée avec la clé configurée, fenêtre de validité en cours.
    Valid { claims: LicenseClaims },
    /// Jeton authentique mais date d'expiration dépassée — MAJ/support suspendus.
    Expired { claims: LicenseClaims },
    /// Jeton présent mais signature invalide, signataire inattendu, ou payload corrompu.
    Invalid,
    /// Aucune autorité configurée et/ou aucun jeton trouvé localement — mode autonome.
    NotConfigured,
}

impl LicenseStatus {
    /// Message court destiné à la bannière du tableau de bord — toujours rassurant sur
    /// la disponibilité des données, conformément à la licence « soft ».
    pub fn banner(&self) -> &'static str {
        match self {
            LicenseStatus::Valid { .. } => "Licence active",
            LicenseStatus::Expired { .. } => {
                "Licence expirée — mises à jour et support suspendus ; vos données restent pleinement accessibles"
            }
            LicenseStatus::Invalid => {
                "Licence invalide — mises à jour et support suspendus ; vos données restent pleinement accessibles"
            }
            LicenseStatus::NotConfigured => {
                "Aucune licence configurée — mode autonome ; vos données restent pleinement accessibles"
            }
        }
    }
}

/// Vérifie un jeton hors-ligne avec la clé de confiance CONFIGURÉE puis applique la
/// fenêtre de validité temporelle. Fonction pure (aucun I/O) — testable sans horloge réelle.
pub fn evaluate(token: &LicenseToken, trusted_authority_pubkey_hex: &str, now: DateTime<Utc>) -> LicenseStatus {
    match open_token(token, trusted_authority_pubkey_hex) {
        Ok(claims) if now > claims.expires_at => LicenseStatus::Expired { claims },
        Ok(claims) => LicenseStatus::Valid { claims },
        Err(_) => LicenseStatus::Invalid,
    }
}

/// Charge le jeton depuis le fichier JSON local. `None` si absent/illisible — laisse
/// l'appelant décider de l'état (`NotConfigured`), sans jamais faire échouer le démarrage.
fn load_token_from_file(path: &Path) -> Option<LicenseToken> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Calcule l'état de licence courant depuis la configuration locale (clé de confiance +
/// fichier jeton). Ne touche jamais au réseau — c'est la garantie du fonctionnement hors-ligne.
pub fn current_status(trusted_authority_pubkey_hex: Option<&str>, token_path: &Path) -> LicenseStatus {
    let (Some(pubkey_hex), Some(token)) = (trusted_authority_pubkey_hex, load_token_from_file(token_path)) else {
        return LicenseStatus::NotConfigured;
    };
    evaluate(&token, pubkey_hex, Utc::now())
}

/// Résout le chemin du jeton depuis `LICENSE_TOKEN_PATH` (défaut : `./license.json`).
pub fn token_path_from_env() -> PathBuf {
    PathBuf::from(std::env::var("LICENSE_TOKEN_PATH").unwrap_or_else(|_| "./license.json".to_string()))
}

/// Lit la clé publique de l'autorité de confiance depuis `LICENSE_AUTHORITY_PUBKEY_HEX`.
/// Absente ⇒ `None` : le nœud fonctionne en mode autonome, sans jamais bloquer le démarrage.
pub fn trusted_authority_pubkey_from_env() -> Option<String> {
    std::env::var("LICENSE_AUTHORITY_PUBKEY_HEX")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use sovereign_core::license::{issue_token, LicenseAuthorityKeypair};

    fn setup() {
        let _ = sovereign_core::crypto::init();
    }

    fn jeton_avec_validite(autorite: &LicenseAuthorityKeypair, validite: Duration) -> (LicenseToken, LicenseClaims) {
        let maintenant = Utc::now();
        let claims = LicenseClaims {
            tenant_id:  "al-baraa-demo".to_string(),
            plan:       "pro".to_string(),
            issued_at:  maintenant,
            expires_at: maintenant + validite,
        };
        (issue_token(autorite, &claims).expect("émission"), claims)
    }

    #[test]
    fn jeton_valide_et_non_expire_donne_un_statut_valid() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let (jeton, claims) = jeton_avec_validite(&autorite, Duration::days(365));

        match evaluate(&jeton, &autorite.public_hex(), Utc::now()) {
            LicenseStatus::Valid { claims: obtenues } => assert_eq!(obtenues, claims),
            autre => panic!("attendu Valid, obtenu {autre:?}"),
        }
    }

    #[test]
    fn jeton_authentique_mais_perime_donne_un_statut_expired_pas_invalid() {
        // Distinction importante : un jeton expiré reste AUTHENTIQUE — c'est sa fenêtre
        // temporelle qui est dépassée. Le confondre avec "invalid" masquerait au gérant
        // qu'il s'agit d'un simple renouvellement à faire, pas d'une corruption/usurpation.
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let (jeton, claims) = jeton_avec_validite(&autorite, Duration::days(-1));

        match evaluate(&jeton, &autorite.public_hex(), Utc::now()) {
            LicenseStatus::Expired { claims: obtenues } => assert_eq!(obtenues, claims),
            autre => panic!("attendu Expired, obtenu {autre:?}"),
        }
    }

    #[test]
    fn jeton_signe_par_une_autorite_non_configuree_donne_invalid() {
        setup();
        let autorite_legitime = LicenseAuthorityKeypair::generate();
        let autre_autorite = LicenseAuthorityKeypair::generate();
        let (jeton, _) = jeton_avec_validite(&autre_autorite, Duration::days(365));

        let statut = evaluate(&jeton, &autorite_legitime.public_hex(), Utc::now());
        assert!(matches!(statut, LicenseStatus::Invalid),
            "un jeton signé par une autorité non configurée doit être Invalid, jamais Valid");
    }

    #[test]
    fn aucune_autorite_configuree_donne_not_configured_sans_toucher_au_disque() {
        setup();
        // Chemin garanti absent : le statut doit rester NotConfigured (mode autonome),
        // jamais une erreur qui empêcherait le nœud de démarrer.
        let statut = current_status(None, Path::new("/chemin/totalement/inexistant/license.json"));
        assert!(matches!(statut, LicenseStatus::NotConfigured));
    }

    #[test]
    fn jeton_absent_donne_not_configured_meme_avec_autorite_configuree() {
        setup();
        let autorite = LicenseAuthorityKeypair::generate();
        let statut = current_status(Some(&autorite.public_hex()), Path::new("/chemin/totalement/inexistant/license.json"));
        assert!(matches!(statut, LicenseStatus::NotConfigured));
    }

    #[test]
    fn la_banniere_dun_etat_degrade_rassure_toujours_sur_les_donnees() {
        setup();
        for statut in [
            LicenseStatus::Invalid,
            LicenseStatus::NotConfigured,
        ] {
            assert!(statut.banner().contains("données restent pleinement accessibles"),
                "chaque bannière en état dégradé doit rassurer sur l'accès aux données — licence SOFT");
        }
    }
}
