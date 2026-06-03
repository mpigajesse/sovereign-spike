"""Terminate idle/blocked connections and verify PG is responsive."""
import subprocess, os, sys

psql = r"C:\Program Files\PostgreSQL\18\bin\psql.exe"
env = {**os.environ, "PGPASSWORD": "admin"}

# Test simple connectivity first (timeout = 30s)
r = subprocess.run(
    [psql, "-U", "postgres", "-h", "127.0.0.1", "-p", "5432",
     "-c", "SELECT 1 AS alive"],
    env=env, capture_output=True, text=True, timeout=30
)
print(f"Connexion test: returncode={r.returncode}")
print(f"stdout: {r.stdout.strip()}")
print(f"stderr: {r.stderr.strip()}")

if r.returncode == 0:
    # Kill idle connections
    r2 = subprocess.run(
        [psql, "-U", "postgres", "-h", "127.0.0.1", "-p", "5432",
         "-d", "sovereign_active",
         "-c", "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='sovereign_active' AND pid <> pg_backend_pid() AND state IN ('idle','idle in transaction');"],
        env=env, capture_output=True, text=True, timeout=30
    )
    print(f"\nTerminate idle: {r2.stdout.strip()}")
