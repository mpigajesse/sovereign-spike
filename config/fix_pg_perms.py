"""
Fix PostgreSQL permissions for sovereign user.
Uses only stdlib - no external deps required.
Pure Python PostgreSQL wire protocol (simplified).
"""
import socket
import struct
import hashlib
import hmac
import os
import sys

def pg_connect_and_fix():
    """Connect to PG and run GRANT statements using simple protocol."""
    import subprocess

    psql = r"C:\Program Files\PostgreSQL\18\bin\psql.exe"
    env = os.environ.copy()
    env["PGPASSWORD"] = "admin"

    sql_commands = [
        "GRANT USAGE ON SCHEMA public TO sovereign",
        "GRANT CREATE ON SCHEMA public TO sovereign",
        "GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO sovereign",
        "GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO sovereign",
        "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO sovereign",
        "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO sovereign",
    ]

    for sql in sql_commands:
        result = subprocess.run(
            [psql, "-U", "postgres", "-h", "127.0.0.1", "-p", "5432",
             "-d", "sovereign_active", "-c", sql],
            env=env,
            capture_output=True,
            text=True,
            timeout=10
        )
        status = "OK" if result.returncode == 0 else f"ERREUR: {result.stderr.strip()}"
        print(f"  {sql[:50]}... → {status}")

    # Test: sovereign can create a table
    test = subprocess.run(
        [psql, "-U", "sovereign", "-h", "127.0.0.1", "-p", "5432",
         "-d", "sovereign_active", "-c",
         "CREATE TABLE IF NOT EXISTS _perm_test (x int); DROP TABLE _perm_test; SELECT 'PERMISSIONS OK'"],
        env={**env, "PGPASSWORD": "sovereign"},
        capture_output=True, text=True, timeout=10
    )
    if test.returncode == 0:
        print("\nTest permissions sovereign : OK ✓")
    else:
        print(f"\nTest permissions ECHEC : {test.stderr}")
        sys.exit(1)

if __name__ == "__main__":
    print("=== Fix PostgreSQL permissions ===")
    pg_connect_and_fix()
    print("=== Terminé ===")
