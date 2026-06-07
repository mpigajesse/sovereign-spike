-- Schéma métier (LOT 2) — séparation moteur / métier (décision D1).
--
-- Le MOTEUR reste dans `public` (journal, époque, parc, DEK — prouvés et répliqués).
-- Le MÉTIER vit dans un schéma dédié `business`. Même base PostgreSQL → l'atomicité
-- d'une opération à invariant fort (vente : stock + journal) reste garantie en UNE
-- transaction (décision §5bis, pas de 2PC).
--
-- Toutes les données portent le `tenant_id` (identité de la PME) : design honnête et
-- prêt pour le multi-PME, et base de la séparation des blobs au relais « Amane ».

CREATE SCHEMA IF NOT EXISTS business;

-- Produits — CRUD pur (aucun invariant inter-lignes). Le `sku` sert de lien avec le
-- stock (table public.stock, clé item_id = sku) pour les ventes.
CREATE TABLE IF NOT EXISTS business.produits (
    id         UUID        PRIMARY KEY,
    tenant_id  UUID        NOT NULL,
    sku        TEXT        NOT NULL,
    nom        TEXT        NOT NULL,
    prix_cents BIGINT      NOT NULL DEFAULT 0 CHECK (prix_cents >= 0), -- montant en centimes (jamais de float pour de l'argent)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, sku)
);

-- Clients — CRUD pur.
CREATE TABLE IF NOT EXISTS business.clients (
    id         UUID        PRIMARY KEY,
    tenant_id  UUID        NOT NULL,
    nom        TEXT        NOT NULL,
    email      TEXT        NOT NULL DEFAULT '',
    telephone  TEXT        NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
