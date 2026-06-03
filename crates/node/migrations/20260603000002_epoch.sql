-- Fencing token : jeton d'époque pour prévenir le split-brain.
--
-- Principe :
--   - Le primary démarre avec epoch = 1 (ou N après N failovers).
--   - Chaque COMMIT d'écriture vérifie que l'époque DB == époque locale du nœud.
--   - Lors d'une promotion (Patroni → promote_command), le nouveau primary
--     incrémente l'epoch AVANT d'accepter la moindre écriture.
--   - L'ancien primary, dont l'époque locale est désormais obsolète (N vs N+1),
--     voit le mismatch et refuse toute écriture → neutralisation automatique.
--
-- Cette table est protégée par la transaction SERIALIZABLE des écritures
-- → la vérification d'époque et l'écriture du journal sont atomiques.

CREATE TABLE IF NOT EXISTS node_epoch (
    id           INTEGER     PRIMARY KEY CHECK (id = 1),
    epoch        BIGINT      NOT NULL DEFAULT 1,
    promoted_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    primary_host TEXT        NOT NULL DEFAULT ''
);

-- Époque initiale : 1 (avant tout failover)
INSERT INTO node_epoch (id, epoch, primary_host)
VALUES (1, 1, 'initial')
ON CONFLICT (id) DO NOTHING;
