# Configuration — Relais Aveugle : VM Kali Linux

**Statut :** ✅ Déployé et opérationnel  
**Date de validation :** 2026-06-04  
**Rôle :** Relais éditeur aveugle (zéro-knowledge) — aucune DEK chargée

---

## Identité du nœud

| Paramètre | Valeur |
|-----------|--------|
| OS | Kali Linux (rolling) |
| Nom d'hôte | `kalilinux` |
| Utilisateur | `kalilinux` |
| Type | VM VMware |
| IP WAN (eth0) | `192.168.1.20` |
| IP LAN VMnet1 (eth1) | `192.168.200.128` |
| sovereign-relay | `:4000` |

---

## Services

| Service | Statut | Port |
|---------|--------|------|
| sovereign-relay | ✅ | `4000` |

> **Propriété zéro-knowledge :** le relais ne charge ni `SOVEREIGN_DEK_HEX` ni aucune clé cryptographique. Il stocke et redistribue des blobs opaques — il ne peut pas interpréter leur contenu.

---

## Étape 0 — Nettoyer les dépôts cassés (si nécessaire)

```bash
# Supprimer les dépôts PGDG mal configurés (le client PG n'est pas nécessaire pour le relais)
sudo rm -f /etc/apt/sources.list.d/pgdg.list /etc/apt/sources.list.d/pgadmin4.list
sudo apt-get update -q 2>/dev/null || true
```

> **Note :** le script `00_install_postgres_client_pgadmin.sh` installe le client PostgreSQL et pgAdmin pour l'inspection réseau. Il est optionnel — le relais n'en a pas besoin. Si le dépôt PGDG échoue (erreur 404 `kali-rolling-pgdg`), c'est parce que Kali retourne `kali-rolling` comme distro mais PGDG n'a que des noms Debian/Ubuntu. Fix commité : forcer `bookworm`.

---

## Étape 1 — Installation des dépendances + compilation

```bash
# Fixer les permissions si cloné avec sudo
sudo chown -R kalilinux:kalilinux /opt/sovereign-spike
git config --global --add safe.directory /opt/sovereign-spike

# Dépendances système
sudo apt-get update -q || true   # ignorer les warnings de clé Kali
sudo apt-get install -y build-essential pkg-config libssl-dev libsodium-dev curl

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# Cloner (si pas encore fait)
sudo git clone https://github.com/mpigajesse/sovereign-spike.git /opt/sovereign-spike

# Compiler
cd /opt/sovereign-spike
cargo build --release --bin sovereign-relay
# Durée : ~1-2 min (premier build)
```

Vérification :
```bash
ls /opt/sovereign-spike/target/release/sovereign-relay
# → sovereign-relay (binaire présent)
```

---

## Étape 2 — Démarrer le relais

```bash
bash /opt/sovereign-spike/scripts/kali/02_start_relay.sh
```

Configuration (depuis `scripts/shared.env` ou valeurs par défaut) :
```
RELAY_LISTEN_ADDR = 0.0.0.0:4000
RELAY_MAX_BLOBS   = 100000
RELAY_API_KEY     = sovereign-spike-relay-key-2026
```

> Le `shared.env` sur le relais ne contient **pas** `SOVEREIGN_DEK_HEX` — c'est intentionnel. Le relais ne connaît que la `RELAY_API_KEY` pour authentifier les pushes du nœud actif.

---

## Vérifications

```bash
# Santé
curl http://192.168.200.128:4000/health
# → {"role":"relay-aveugle","status":"ok","blob_count":N}

# Push d'un blob test
curl -X POST http://192.168.200.128:4000/blobs \
    -H "Content-Type: application/json" \
    -H "X-Relay-Key: sovereign-spike-relay-key-2026" \
    -d '{"seq":9999,"blob_nonce":"aaaa...","blob_ciphertext":"deadbeef..."}'
# → {"status":"stored"}

# Fetch des blobs (zéro-knowledge — le relais ne déchiffre pas)
curl "http://192.168.200.128:4000/blobs?after_seq=9998&limit=1"
# → [{"seq":9999,"blob_nonce":"aaaa...","blob_ciphertext":"deadbeef..."}]
```

---

## Inspection réseau (optionnel)

Kali est idéal pour observer le trafic réseau du spike :

```bash
# Voir les requêtes HTTP vers le relais
sudo tcpdump -i eth1 port 4000 -A

# Observer le trafic PostgreSQL (réplication WAL)
sudo tcpdump -i eth1 port 5432 -A

# Observer le trafic nœud actif
sudo tcpdump -i eth1 host 192.168.200.1 and port 3000 -A

# GUI Wireshark (si installé)
wireshark -i eth1 -f 'port 4000 or port 5432'
```

---

## Problèmes rencontrés et résolus

### P1 — Dépôt PGDG introuvable (404 kali-rolling-pgdg)

**Symptôme :** `E: The repository 'https://apt.postgresql.org/pub/repos/apt kali-rolling-pgdg Release' does not have a Release file.`  
**Cause :** `lsb_release -cs` retourne `kali-rolling` sur Kali, mais PGDG n'a pas ce nom — seulement Debian/Ubuntu (`bookworm`, `jammy`...).  
**Solution :** Hardcoder `DISTRO="bookworm"` dans le script. Fix commité : `4396cff`.  
**Contournement rapide :** `sudo rm -f /etc/apt/sources.list.d/pgdg.list` (client PG optionnel).

### P2 — Permission denied sur `Cargo.lock`

**Symptôme :** `error: failed to write /opt/sovereign-spike/Cargo.lock — Permission denied (os error 13)`  
**Cause :** `/opt/sovereign-spike` cloné avec `sudo git clone` → appartient à root.  
**Solution :** `sudo chown -R kalilinux:kalilinux /opt/sovereign-spike`

### P3 — `fatal: detected dubious ownership`

**Symptôme :** `fatal: detected dubious ownership in repository at '/opt/sovereign-spike'`  
**Cause :** Git refuse d'opérer dans un répertoire dont le propriétaire diffère de l'utilisateur courant.  
**Solution :** `git config --global --add safe.directory /opt/sovereign-spike`

---

*Relais aveugle — VM Kali Linux — Sovereign-Spike Phase 0 — EIGSI × AL BARAA CONSULTING*
