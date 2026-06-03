-- setup_replication.sql
-- À exécuter sur le PRIMARY via pgAdmin (Query Tool, connecté en superuser).
--
-- Étape 1 : créer la base applicative et les rôles
-- ─────────────────────────────────────────────────

CREATE DATABASE sovereign_active;

CREATE ROLE sovereign WITH LOGIN PASSWORD 'sovereign';
GRANT ALL PRIVILEGES ON DATABASE sovereign_active TO sovereign;

-- Rôle dédié à la réplication streaming (minimal : REPLICATION uniquement)
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'replicator_spike';

-- Étape 2 : slot de réplication physique (évite les gaps de WAL pendant l'arrêt)
-- ────────────────────────────────────────────────────────────────────────────────
SELECT pg_create_physical_replication_slot('sovereign_slot');

-- Étape 3 : vérifier la configuration
-- ────────────────────────────────────
SHOW wal_level;                        -- doit afficher 'replica'
SHOW synchronous_commit;               -- doit afficher 'on'
SHOW synchronous_standby_names;        -- doit afficher 'sovereign-standby'
SELECT * FROM pg_replication_slots;    -- doit montrer sovereign_slot

-- Étape 4 : après pg_basebackup sur le standby, vérifier la réplication
-- ────────────────────────────────────────────────────────────────────────
-- SELECT * FROM pg_stat_replication;  -- affiche les standbys connectés
-- SELECT * FROM pg_stat_wal_receiver; -- (depuis le standby)
