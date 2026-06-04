# Configuration — Nœud Passif : VM Ubuntu 26.04 LTS

**Statut :** ✅ Déployé et opérationnel  
**Date de validation :** 2026-06-04  
**Rôle :** Standby PostgreSQL (réplication streaming) + nœud passif SQLite

---

## Identité du nœud

| Paramètre | Valeur |
|-----------|--------|
| OS | Ubuntu 26.04 LTS |
| Nom d'hôte | `ubuntu2604` |
| Utilisateur | `ubuntu` |
| Type | VM VMware |
| IP LAN (VMnet1 — réseau spike) | `192.168.200.130` |
| Primary (Windows) | `192.168.200.1:5432` |
| PostgreSQL | 18 (mode standby) |
| sovereign-node-passive | `:3001` |

---

## Services

| Service | Statut | Port |
|---------|--------|------|
| PostgreSQL 18 standby | ✅ streaming | `5432` (local) |
| sovereign-node-passive | ✅ | `3001` |

---

## Étape 1 — Installation des dépendances

```bash
sudo apt-get update -q
sudo apt-get install -y build-essential pkg-config curl libssl-dev libsodium-dev

# PostgreSQL 18 (dépôt PGDG — utiliser bookworm même sur Ubuntu 26.04)
curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    | sudo gpg --dearmor -o /usr/share/keyrings/postgresql.gpg
echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] https://apt.postgresql.org/pub/repos/apt jammy-pgdg main" \
    | sudo tee /etc/apt/sources.list.d/pgdg.list
sudo apt-get update -q
sudo apt-get install -y postgresql-18

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
```

---

## Étape 2 — Cloner et compiler

```bash
sudo git clone https://github.com/mpigajesse/sovereign-spike.git /opt/sovereign-spike
sudo chown -R ubuntu:ubuntu /opt/sovereign-spike
cd /opt/sovereign-spike
source ~/.cargo/env
cargo build --release --bin sovereign-node-passive
```

---

## Étape 3 — Configurer le standby PostgreSQL

```bash
sudo bash /opt/sovereign-spike/scripts/ubuntu/02_setup_standby.sh
```

Ce script :
1. Teste la connectivité vers le primary (192.168.200.1:5432)
2. Arrête le PostgreSQL local
3. Lance `pg_basebackup` avec le rôle `replicator` (mot de passe : `replicator_spike`)
4. Configure `postgresql.auto.conf` :
   ```
   primary_conninfo = 'host=192.168.200.1 port=5432 user=replicator password=replicator_spike application_name=sovereign_standby'
   primary_slot_name = 'sovereign_slot'
   hot_standby = on
   ```
5. Crée `standby.signal`
6. Démarre PostgreSQL en mode standby

Vérification depuis Windows :
```powershell
PGPASSWORD=admin psql -U postgres -h 127.0.0.1 -p 5432 -c "SELECT client_addr, state, sync_state FROM pg_stat_replication;"
# → 192.168.200.130 | streaming | async
```

---

## Étape 4 — Démarrer le nœud passif

```bash
sudo bash /opt/sovereign-spike/scripts/ubuntu/03_start_passive.sh
```

Configuration chargée depuis `scripts/shared.env` :
```
ACTIVE_NODE_URL   = http://192.168.200.1:3000
PASSIVE_DB_PATH   = /tmp/sovereign_passive.db
PASSIVE_LISTEN_ADDR = 0.0.0.0:3001
SYNC_INTERVAL_SECS  = 5
DEK               = 174835f0...
```

Le nœud passif :
- Interroge `GET /journal?after_seq=N` sur le nœud actif toutes les 5s
- Déchiffre les blobs avec la DEK partagée
- Stocke le stock dans SQLite (`/tmp/sovereign_passive.db`)
- Expose `/health`, `/stock/{item_id}`, `/sync/status`

---

## Vérifications

```bash
# Santé
curl http://192.168.200.130:3001/health
# → ok (passif)

# Statut de synchronisation
curl http://192.168.200.130:3001/sync/status
# → {"last_seq": N}

# Stock (après sync)
curl http://192.168.200.130:3001/stock/PANTALON-L
# → {"item_id": "PANTALON-L", "quantity": N}
```

---

## Pare-feu

```bash
sudo ufw allow 3001/tcp   # nœud passif (accès depuis Windows pour le test E2E)
sudo ufw allow 5432/tcp   # PostgreSQL (réplication depuis Windows)
sudo ufw reload
```

---

## Problèmes rencontrés et résolus

### P1 — Authentification `pg_basebackup` échouée

**Symptôme :** `FATAL: authentification par mot de passe échouée pour l'utilisateur «replicator»`  
**Cause :** Exécution via double sudo (`sudo bash script` → `sudo -u postgres pg_basebackup`) — le prompt interactif ne transmettait pas le mot de passe correctement dans certains contextes.  
**Solution :** Utiliser `PGPASSWORD=replicator_spike sudo -E -u postgres pg_basebackup ...` dans le script.  
**Fix commité :** `cbcfabe`

### P2 — Passif bloqué à last_seq=3

**Symptôme :** Le passif ne progresse plus après seq=3 malgré de nouvelles écritures.  
**Cause :** Les opérations précédentes (seq 4-6) avaient été chiffrées avec une DEK éphémère (`df385092…`) différente de la DEK de `shared.env` (`174835f0…`). Le passif ne pouvait pas déchiffrer ces blobs et restait bloqué.  
**Solution :** Redémarrer le nœud actif avec `SOVEREIGN_DEK_HEX` correctement chargé depuis `shared.env`, puis `TRUNCATE` des tables et redémarrer le passif.  
**Règle :** `SOVEREIGN_DEK_HEX` doit être défini au **premier démarrage** et ne jamais changer.

---

*Nœud passif — VM Ubuntu 26.04 — Sovereign-Spike Phase 0 — EIGSI × AL BARAA CONSULTING*
