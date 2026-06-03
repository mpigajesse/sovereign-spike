-- Correction des permissions pour PostgreSQL 15+ / 18
-- Le schéma public n'est plus accessible par défaut aux non-superusers

GRANT USAGE  ON SCHEMA public TO sovereign;
GRANT CREATE ON SCHEMA public TO sovereign;
GRANT ALL PRIVILEGES ON ALL TABLES    IN SCHEMA public TO sovereign;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO sovereign;

ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT ALL ON TABLES    TO sovereign;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT ALL ON SEQUENCES TO sovereign;

SELECT 'permissions corrigees' AS status;
