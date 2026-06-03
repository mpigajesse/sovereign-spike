-- Schéma SQLite du nœud passif (réplica lecture seule).
--
-- Le passif ne reçoit que des blobs chiffrés depuis le nœud actif.
-- Il les déchiffre localement avec sa copie de la DEK et reconstruit
-- l'état du stock pour les lectures hors-ligne.

CREATE TABLE IF NOT EXISTS stock_replica (
    item_id  TEXT    PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 0
);

-- État de synchronisation : dernier seq appliqué localement.
CREATE TABLE IF NOT EXISTS sync_state (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    last_seq INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO sync_state (id, last_seq) VALUES (1, 0);
