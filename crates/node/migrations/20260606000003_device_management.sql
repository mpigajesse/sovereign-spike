-- Gestion du parc de machines (§6 cadrage Phase 0) + rotation de DEK + récupération.
--
-- Rend démontrables en live les critères §7.4 :
--   #8  enrôlement sans exposer la clé    → enrolled_devices.sealed_dek (sealed box)
--   #9  dé-enrôlement → ne peut plus déchiffrer → rotation : dek_state nouvelle génération
--   #10 retrait → recalcul quorum + alerte → count(enrolled_devices) < 3
--   #11 perte de toutes les machines → code de récupération → recovery_blob

-- Registre persistant des appareils enrôlés.
-- public_key : clé publique X25519 (32 octets) reçue à l'enrôlement (QR ou équivalent).
-- sealed_dek : DEK COURANTE emballée (sealed box) pour cet appareil — re-scellée à chaque rotation.
CREATE TABLE IF NOT EXISTS enrolled_devices (
    device_id   UUID        PRIMARY KEY,
    label       TEXT        NOT NULL DEFAULT '',
    public_key  BYTEA       NOT NULL,
    sealed_dek  BYTEA       NOT NULL,
    enrolled_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Générations de DEK. Le nœud actif ÉCRIT le journal avec la génération la plus haute.
-- Les anciens blobs restent chiffrés avec leur génération d'origine (déchiffrables tant
-- qu'on détient l'ancienne DEK) : un appareil dé-enrôlé garde l'ancienne DEK et peut donc
-- lire l'ancien journal, mais PAS les écritures postérieures à la rotation (critère #9).
CREATE TABLE IF NOT EXISTS dek_state (
    generation BIGINT      PRIMARY KEY,
    dek_hex    TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Code de récupération : DEK courante emballée sous une clé dérivée d'une passphrase
-- (Argon2id). salt en clair (nécessaire à la dérivation), nonce + ciphertext du blob.
-- Avec la passphrase seule, on reconstruit la DEK même si toutes les machines sont perdues.
CREATE TABLE IF NOT EXISTS recovery_blob (
    id            INTEGER     PRIMARY KEY CHECK (id = 1),
    salt          BYTEA       NOT NULL,
    wrapped_nonce BYTEA       NOT NULL,
    wrapped_ct    BYTEA       NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
