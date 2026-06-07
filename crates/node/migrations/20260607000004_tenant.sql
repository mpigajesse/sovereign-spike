-- Identité du tenant (la PME) — LOT 1 « création de compte ».
--
-- Modèle (cf. docs/metier/architecture-multitenant-et-metier.md) :
--   1 PME = 1 tenant = 1 cluster (1 actif + N passifs) = 1 DEK = 1 jeu de données.
--
-- Singleton (id = 1) : un cluster ne sert qu'UN seul tenant. Le tenant_id est généré
-- LOCALEMENT à la création du compte (auto-souverain — aucun serveur central éditeur),
-- puis partagé aux autres machines par l'enrôlement (sealed box de la DEK).
--
-- Le tenant_id sert d'identité opaque : il estampille les données métier et permet au
-- relais aveugle « Amane » de séparer les blobs de plusieurs PME sans jamais déchiffrer.

CREATE TABLE IF NOT EXISTS tenant (
    id         INTEGER     PRIMARY KEY CHECK (id = 1),
    tenant_id  UUID        NOT NULL,
    nom        TEXT        NOT NULL,           -- nom de l'entreprise (PME)
    gerant     TEXT        NOT NULL DEFAULT '',-- nom du gérant (optionnel)
    email      TEXT        NOT NULL DEFAULT '',-- email de contact (optionnel)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
