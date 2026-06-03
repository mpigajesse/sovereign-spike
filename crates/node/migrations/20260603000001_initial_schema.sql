-- Schéma initial du nœud actif souverain (Phase 0 spike).
--
-- Deux tables :
--   stock             → état courant par article (pièce maîtresse anti-survente)
--   operations_journal → journal append-only des écritures chiffrées (blobs CBOR)
--
-- Règle d'or : la contrainte CHECK sur stock.quantity est le dernier garde-fou ;
-- la vraie protection est l'isolation SERIALIZABLE sur chaque transaction d'écriture.

CREATE TABLE IF NOT EXISTS stock (
    item_id  TEXT   PRIMARY KEY,
    quantity BIGINT NOT NULL DEFAULT 0 CHECK (quantity >= 0)
);

-- Séquence dédiée pour les numéros d'ordre du journal.
-- Avantage : nextval() est atomique et ne se ré-alloue jamais même si la tx roule back.
-- Accepté pour le spike : les gaps de séquence sont possibles mais non problématiques.
CREATE SEQUENCE IF NOT EXISTS journal_seq START 1;

CREATE TABLE IF NOT EXISTS operations_journal (
    seq             BIGINT      PRIMARY KEY,                 -- imposé par le nœud actif
    op_id           UUID        UNIQUE NOT NULL,             -- clé d'idempotence
    written_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    blob_nonce      BYTEA       NOT NULL,                    -- 24 octets XChaCha20
    blob_ciphertext BYTEA       NOT NULL                     -- CBOR Operation chiffré
);
