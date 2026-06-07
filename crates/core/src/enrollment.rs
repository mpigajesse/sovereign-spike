//! Enrôlement et désenrôlement d'appareils (§5.4 Phase 0).
//!
//! Modèle de confiance :
//!   - L'appareil 1 (déjà enrôlé, possède la DEK) reçoit la clé publique X25519
//!     de l'appareil 2 (via QR ou échange sécurisé hors-bande).
//!   - Il emballe la DEK dans une sealed box adressée à la clé publique de l'appareil 2.
//!   - Seul l'appareil 2, avec sa clé privée, peut ouvrir ce blob.
//!   - Le relais, même s'il intercepte le blob, ne peut rien en faire.
//!
//! Rotation de DEK :
//!   - Une nouvelle DEK est générée.
//!   - Elle est re-scellée pour chaque appareil encore enrôlé.
//!   - Le journal existant reste lisible avec l'ancienne DEK (on-disk, pas modifié).
//!   - À partir de la rotation, les nouvelles écritures utilisent la nouvelle DEK.

use std::collections::HashMap;

use sodiumoxide::crypto::box_::PublicKey;
use uuid::Uuid;

use crate::crypto::{wrap_dek, CryptoError, Dek};

/// Un appareil enrôlé dans le cluster souverain.
#[derive(Debug, Clone)]
pub struct EnrolledDevice {
    pub device_id:  Uuid,
    /// Clé publique X25519 de l'appareil (reçue lors de l'enrôlement).
    pub public_key: PublicKey,
    /// DEK courante scellée pour cet appareil (sealed box → XChaCha20 nonce + ciphertext).
    pub sealed_dek: Vec<u8>,
}

/// Registre des appareils enrôlés (en mémoire pour le spike).
/// En production : persisté en DB chiffrée.
#[derive(Default)]
pub struct DeviceRegistry {
    devices: HashMap<Uuid, EnrolledDevice>,
}

#[derive(Debug, thiserror::Error)]
pub enum EnrollmentError {
    #[error("appareil inconnu : {0}")]
    UnknownDevice(Uuid),
    #[error("erreur crypto : {0}")]
    Crypto(#[from] CryptoError),
    #[error("quorum insuffisant : {enrolled} appareil(s) enrôlé(s), {required} requis pour désenrôler")]
    InsufficientQuorum { enrolled: usize, required: usize },
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enrôle un nouvel appareil.
    /// L'appelant doit posséder la DEK courante (preuve qu'il est déjà autorisé).
    pub fn enroll(&mut self, dek: &Dek, device_public_key: PublicKey) -> EnrolledDevice {
        let sealed_dek = wrap_dek(dek, &device_public_key);
        let device = EnrolledDevice {
            device_id: Uuid::new_v4(),
            public_key: device_public_key,
            sealed_dek,
        };
        self.devices.insert(device.device_id, device.clone());
        device
    }

    /// Désenrôle un appareil.
    /// Exige qu'il reste au moins `min_remaining` appareils après suppression
    /// (évite de perdre l'accès à la DEK).
    pub fn revoke(
        &mut self,
        device_id: Uuid,
        min_remaining: usize,
    ) -> Result<(), EnrollmentError> {
        if !self.devices.contains_key(&device_id) {
            return Err(EnrollmentError::UnknownDevice(device_id));
        }
        let after_count = self.devices.len() - 1;
        if after_count < min_remaining {
            return Err(EnrollmentError::InsufficientQuorum {
                enrolled:  self.devices.len(),
                required:  min_remaining + 1,
            });
        }
        self.devices.remove(&device_id);
        Ok(())
    }

    /// Génère une nouvelle DEK et la re-scelle pour tous les appareils encore enrôlés.
    /// Retourne la nouvelle DEK (à utiliser pour les écritures suivantes).
    pub fn rotate_dek(&mut self) -> Dek {
        let new_dek = Dek::generate();
        for device in self.devices.values_mut() {
            device.sealed_dek = wrap_dek(&new_dek, &device.public_key);
        }
        new_dek
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn get(&self, device_id: &Uuid) -> Option<&EnrolledDevice> {
        self.devices.get(device_id)
    }

    pub fn all_devices(&self) -> impl Iterator<Item = &EnrolledDevice> {
        self.devices.values()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{decrypt, encrypt, init, unwrap_dek, DeviceKeypair};

    fn setup() -> Dek {
        init().expect("sodiumoxide init");
        Dek::generate()
    }

    #[test]
    fn enrolement_appareil_peut_dechiffrer_avec_dek_recue() {
        let dek = setup();
        let mut registry = DeviceRegistry::new();

        let appareil = DeviceKeypair::generate();
        let enrolled = registry.enroll(&dek, appareil.public.clone());

        // L'appareil récupère la DEK depuis sa sealed box
        let dek_recue = unwrap_dek(&enrolled.sealed_dek, &appareil.public, &appareil.private)
            .expect("unwrap DEK");

        // Peut déchiffrer un message chiffré avec la DEK originale
        let message = b"stock=100, facture=INV-0001";
        let blob = encrypt(message, &dek);
        let clair = decrypt(&blob, &dek_recue).expect("déchiffrement");
        assert_eq!(clair, message);
    }

    #[test]
    fn deuxieme_appareil_enrole_independamment() {
        let dek = setup();
        let mut registry = DeviceRegistry::new();

        let app1 = DeviceKeypair::generate();
        let app2 = DeviceKeypair::generate();

        let e1 = registry.enroll(&dek, app1.public.clone());
        let e2 = registry.enroll(&dek, app2.public.clone());
        assert_eq!(registry.device_count(), 2);

        // Les deux appareils reçoivent la même DEK (scellée séparément)
        let dek1 = unwrap_dek(&e1.sealed_dek, &app1.public, &app1.private).unwrap();
        let dek2 = unwrap_dek(&e2.sealed_dek, &app2.public, &app2.private).unwrap();
        assert_eq!(dek1.as_bytes(), dek2.as_bytes());
    }

    #[test]
    fn desenrolement_supprime_acces() {
        let dek = setup();
        let mut registry = DeviceRegistry::new();

        let app1 = DeviceKeypair::generate();
        let app2 = DeviceKeypair::generate();
        let e1 = registry.enroll(&dek, app1.public.clone());
        registry.enroll(&dek, app2.public.clone());

        // Désenrôlement OK (il reste 1 appareil → quorum respecté)
        registry.revoke(e1.device_id, 1).expect("révocation OK");
        assert_eq!(registry.device_count(), 1);
    }

    #[test]
    fn desenrolement_refuse_si_dernier_appareil() {
        let dek = setup();
        let mut registry = DeviceRegistry::new();

        let app = DeviceKeypair::generate();
        let enrolled = registry.enroll(&dek, app.public.clone());

        // On ne peut pas supprimer le dernier appareil (min_remaining = 1)
        let err = registry.revoke(enrolled.device_id, 1);
        assert!(matches!(err, Err(EnrollmentError::InsufficientQuorum { .. })));
        assert_eq!(registry.device_count(), 1);
    }

    #[test]
    fn rotation_dek_les_anciens_appareils_obtiennent_nouvelle_cle() {
        let dek_orig = setup();
        let mut registry = DeviceRegistry::new();

        let app1 = DeviceKeypair::generate();
        let app2 = DeviceKeypair::generate();
        registry.enroll(&dek_orig, app1.public.clone());
        registry.enroll(&dek_orig, app2.public.clone());

        let new_dek = registry.rotate_dek();

        // Les deux appareils ont reçu la nouvelle DEK
        for device in registry.all_devices() {
            let kp = if device.public_key == app1.public {
                (&app1.public, &app1.private)
            } else {
                (&app2.public, &app2.private)
            };
            let recovered = unwrap_dek(&device.sealed_dek, kp.0, kp.1).unwrap();
            assert_eq!(recovered.as_bytes(), new_dek.as_bytes(),
                "l'appareil doit avoir la nouvelle DEK après rotation");
        }

        // L'ancienne DEK ne correspond plus à la nouvelle
        assert_ne!(dek_orig.as_bytes(), new_dek.as_bytes());
    }

    #[test]
    fn appareil_non_enrole_ne_peut_pas_dechiffrer() {
        let dek = setup();
        let mut registry = DeviceRegistry::new();

        let app_legitime = DeviceKeypair::generate();
        registry.enroll(&dek, app_legitime.public.clone());

        // Appareil non enrôlé tente d'accéder
        let intrus = DeviceKeypair::generate();
        let blob = encrypt(b"donnee confidentielle", &dek);

        // L'intrus n'a pas reçu de sealed_dek → il ne peut pas récupérer la DEK
        // Simulation : il tente avec sa propre clé privée sur la sealed_dek de l'appareil légitime
        let enrolled = registry.all_devices().next().unwrap();
        let result = unwrap_dek(&enrolled.sealed_dek, &intrus.public, &intrus.private);
        assert!(result.is_err(), "l'intrus ne doit pas pouvoir ouvrir la sealed box");

        // Même si l'intrus avait trouvé la sealed_dek, il ne peut pas déchiffrer le blob
        let _ = blob; // blob chiffré inutilisable sans la DEK
    }
}
