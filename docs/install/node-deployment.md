# Guide de Déploiement — Banc d'Essai Sovereign-Spike

Architecture validée en Phase 0 (2026-06-04) : cluster actif/passif PostgreSQL + relais éditeur aveugle.

**Durée estimée :** 30–60 min selon la vitesse de téléchargement.

---

## Architecture des 3 nœuds

```
┌──────────────────────────────────┐
│  NŒUD ACTIF — Windows 11        │  PC physique
│  PostgreSQL 18 (primary)         │  192.168.200.1
│  sovereign-node-active :3000     │
└────────────┬─────────────────────┘
             │  streaming replication WAL
             │  (rôle replicator / PostgreSQL)
             ▼
┌──────────────────────────────────┐
│  NŒUD PASSIF — Ubuntu 26.04     │  VM VMware VMnet1
│  PostgreSQL 18 (standby)         │  192.168.200.130
│  sovereign-node-passive :3001    │
│  SQLite (réplique locale)        │
└──────────────────────────────────┘

┌──────────────────────────────────┐
│  RELAIS AVEUGLE — Kali Linux    │  VM VMware VMnet1
│  sovereign-relay :4000           │  192.168.200.128
│  Zéro-knowledge : aucune DEK    │
└──────────────────────────────────┘
```

| Nœud | Machine | IP LAN (VMnet1) | Port |
|------|---------|-----------------|------|
| Actif | Windows 11 physique | `192.168.200.1` | `:3000` |
| Passif | Ubuntu 26.04 VM | `192.168.200.130` | `:3001` |
| Relais | Kali Linux VM | `192.168.200.128` | `:4000` |

---

## Prérequis matériels

| Exigence | Minimum |
|----------|---------|
| RAM | 4 Go par machine |
| Disque | 10 Go libres |
| Réseau | LAN VMnet1 host-only (192.168.200.0/24) |
| OS | Windows 11 (actif) · Ubuntu 26.04 (passif) · Kali Linux (relais) |

---

## Ordre de déploiement

```
1. Windows (actif)  → 01_setup_pg.ps1  → 02_start_active.ps1
2. Ubuntu (passif)  → 01_install_deps.sh → 02_setup_standby.sh → 03_start_passive.sh
3. Kali (relais)    → 01_install_deps.sh → 02_start_relay.sh
4. Test E2E         → 03_test_e2e.ps1 (depuis Windows)
5. Licence (optionnel) → sovereign-control + LICENSE_AUTHORITY_PUBKEY_HEX/LICENSE_TOKEN_PATH
```

---

## Étape 1 — Nœud actif (Windows 11)

### 1.1 Prérequis

- PostgreSQL 18 installé (`C:\Program Files\PostgreSQL\18\`)
- Rust toolchain (`rustup` + `cargo`)
- Git

### 1.2 Configurer PostgreSQL

```powershell
cd D:\PFE-FINAL\pfe\sovereign-spike
.\scripts\windows\01_setup_pg.ps1
```

Ce script :
- Crée la base `sovereign_active` et les rôles (`sovereign`, `replicator`)
- Configure `postgresql.conf` (wal_level=replica, synchronous_commit=on)
- Configure `pg_hba.conf` pour autoriser le standby Ubuntu
- Crée le slot de réplication `sovereign_slot`

### 1.3 Démarrer le nœud actif

```powershell
.\scripts\windows\02_start_active.ps1
```

Le binaire `sovereign-node-active.exe` :
- Se connecte à PostgreSQL (`sovereign_active`)
- Applique les migrations SQL
- Charge la DEK depuis `SOVEREIGN_DEK_HEX` (voir `scripts/shared.env`)
- Écoute sur `0.0.0.0:3000`

Vérification :
```powershell
Invoke-RestMethod http://192.168.200.1:3000/health
# → ok
```

---

## Étape 2 — Nœud passif (Ubuntu 26.04)

Voir `docs/install/node-ubuntu-config.md` pour le détail.

```bash
# Cloner le dépôt
sudo git clone https://github.com/mpigajesse/sovereign-spike.git /opt/sovereign-spike
cd /opt/sovereign-spike

# Installer PostgreSQL 18 + dépendances Rust
bash scripts/ubuntu/01_install_deps.sh

# Configurer le standby (pg_basebackup depuis le primary Windows)
sudo bash scripts/ubuntu/02_setup_standby.sh

# Démarrer le nœud passif SQLite
sudo bash scripts/ubuntu/03_start_passive.sh
```

Vérification :
```bash
curl http://192.168.200.130:3001/health
# → ok (passif)
```

---

## Étape 3 — Relais aveugle (Kali Linux)

Voir `docs/install/node-kali-config.md` pour le détail.

```bash
# Cloner le dépôt
sudo git clone https://github.com/mpigajesse/sovereign-spike.git /opt/sovereign-spike
cd /opt/sovereign-spike

# Installer Rust + libsodium + compiler le relais
bash scripts/kali/01_install_deps.sh

# Démarrer le relais
bash scripts/kali/02_start_relay.sh
```

Vérification :
```bash
curl http://192.168.200.128:4000/health
# → {"role":"relay-aveugle","status":"ok"}
```

---

## Étape 4 — Test E2E complet

Depuis Windows, une fois les 3 nœuds démarrés :

```powershell
.\scripts\windows\03_test_e2e.ps1
```

Résultat attendu : **13/13 tests réussis**.

---

## Étape 5 — Licence souveraine (optionnel)

> ⚠️ **Étape facultative.** Sans elle, le nœud actif démarre en *mode autonome*
> (`LicenseStatus::NotConfigured`) et fonctionne à l'identique — c'est tout le
> principe de la licence « **soft** » : son absence, son expiration ou son
> invalidité ne bloquent **jamais** `/write`, `/stock`, `/journal` ni le CRUD
> métier. Seules les mises à jour et le support seraient suspendus, et seule
> la bannière `GET /license` change. À tester séparément du socle technique.

### 5.1 Démarrer le service d'autorité (sovereign-control)

`sovereign-control` est un **binaire éditeur séparé** du relais aveugle — il
détient la clé privée Ed25519 de signature des licences mais ne stocke, ne
transmet et ne déchiffre **aucun blob ni DEK** (deux propriétés cloisonnées,
séparément prouvables en soutenance). Il peut tourner sur n'importe quelle
machine (poste de l'éditeur, pas du client) :

```powershell
cd D:\PFE-FINAL\pfe\sovereign-spike
$env:CONTROL_LISTEN_ADDR = "0.0.0.0:5000"
cargo run -p sovereign-control --bin sovereign-control
```

Au premier démarrage, sans `CONTROL_AUTHORITY_SK_HEX`, une autorité éphémère
est générée et **loguée** (clé privée + publique). Pour la persistance,
récupérer `secret_hex` du log et le redéfinir au prochain démarrage :

```powershell
$env:CONTROL_AUTHORITY_SK_HEX = "<secret_hex récupéré dans les logs>"
```

Noter la `authority_pubkey_hex` affichée — c'est elle qui sera distribuée
(hors-bande) à chaque nœud actif pour la vérification locale.

### 5.2 Émettre un jeton de licence

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:5000/licenses `
    -ContentType "application/json" `
    -Body '{"tenant_id": "al-baraa-demo", "plan": "pro", "validity_days": 365}' `
    | ConvertTo-Json -Depth 5 | Out-File -Encoding utf8 license.json
```

Le fichier `license.json` contient `{ payload_hex, signature_hex, authority_pubkey_hex }`
— à copier sur la machine du nœud actif (PC, `192.168.200.1`).

### 5.3 Configurer le nœud actif pour la vérification locale

Ajouter au `shared.env` du **nœud actif uniquement** (le passif et le relais
n'ont pas besoin de connaître la licence) :

```env
LICENSE_AUTHORITY_PUBKEY_HEX=<authority_pubkey_hex notée à l'étape 5.1>
LICENSE_TOKEN_PATH=D:\PFE-FINAL\pfe\sovereign-spike\license.json
```

Puis redémarrer `sovereign-node-active`. La vérification est **100 % locale et
hors-ligne** — `sovereign-control` n'est interrogé qu'au moment de l'émission,
jamais à l'exécution du nœud.

### 5.4 Vérifier la bannière

```powershell
Invoke-RestMethod http://192.168.200.1:3000/license
# → { "state": "valid", "claims": {...}, "banner": "Licence active" }
```

Scénarios à tester pour la démo (chacun ne doit affecter QUE cette route) :

| Scénario | Manipulation | `state` attendu | Impact sur `/write`, `/stock`, `/journal` |
|----------|--------------|-----------------|-------------------------------------------|
| Licence absente | ne pas définir `LICENSE_TOKEN_PATH` | `not_configured` | Aucun |
| Licence valide | jeton frais, `validity_days: 365` | `valid` | Aucun |
| Licence expirée | émettre avec `validity_days: -1` | `expired` | Aucun |
| Licence falsifiée | modifier un caractère de `signature_hex` dans `license.json` | `invalid` | Aucun |

Le statut est ré-évalué automatiquement toutes les heures (tâche
d'arrière-plan) — utile pour démontrer le renouvellement sans redémarrage.

---

## Variables d'environnement clés

Fichier `scripts/shared.env` (à créer sur chaque nœud, même contenu) :

```env
SOVEREIGN_DEK_HEX=174835f0e063680d4b4652c7edf9472a1db0626388dbbe4342d84a7c9bce035b
RELAY_API_KEY=sovereign-spike-relay-key-2026
ACTIVE_NODE_URL=http://192.168.200.1:3000
RELAY_URL=http://192.168.200.128:4000
```

> **Important :** `SOVEREIGN_DEK_HEX` doit être identique sur le nœud actif et le nœud passif. Le relais ne charge **jamais** cette variable (zéro-knowledge).

Variables de licence — **nœud actif uniquement**, optionnelles (cf. Étape 5) :

```env
LICENSE_AUTHORITY_PUBKEY_HEX=<clé publique de l'autorité — distribuée hors-bande>
LICENSE_TOKEN_PATH=D:\PFE-FINAL\pfe\sovereign-spike\license.json
```

> Absentes ⇒ le nœud démarre en mode autonome (`not_configured`), sans aucun
> impact sur les routes métier. Le nœud passif et le relais n'en ont pas besoin.

---

## Ports à ouvrir (pare-feu)

### Windows (PowerShell admin)

```powershell
New-NetFirewallRule -DisplayName "Sovereign-Active"    -Direction Inbound -Protocol TCP -LocalPort 3000 -Action Allow
New-NetFirewallRule -DisplayName "Sovereign-PG-Replic" -Direction Inbound -Protocol TCP -LocalPort 5432 -Action Allow
```

### Ubuntu / Kali (ufw)

```bash
sudo ufw allow 3001/tcp   # nœud passif
sudo ufw allow 4000/tcp   # relais
sudo ufw allow 5432/tcp   # PostgreSQL standby (Ubuntu uniquement)
sudo ufw reload
```

---

## Checklist de validation

```
Nœud actif (Windows)
[ ] PostgreSQL 18 démarré — service postgresql-x64-18
[ ] Base sovereign_active créée, rôle replicator configuré
[ ] Slot de réplication sovereign_slot créé
[ ] sovereign-node-active.exe UP — GET /health → "ok"

Nœud passif (Ubuntu)
[ ] pg_basebackup terminé — /var/lib/postgresql/18/main peuplé
[ ] standby.signal présent
[ ] PostgreSQL standby UP — pg_isready localhost OK
[ ] sovereign-node-passive UP — GET /health → "ok (passif)"
[ ] pg_stat_replication sur Windows → state=streaming

Relais (Kali)
[ ] sovereign-relay compilé — target/release/sovereign-relay
[ ] sovereign-relay UP — GET /health → role=relay-aveugle

Test E2E
[ ] 13/13 tests réussis
[ ] Cohérence actif ↔ passif vérifiée
[ ] Zero-knowledge relais prouvé

Licence souveraine (optionnel — Étape 5)
[ ] sovereign-control UP — authority_pubkey_hex notée
[ ] Jeton émis (license.json) et copié sur le nœud actif
[ ] GET /license → state="valid" après configuration
[ ] Scénario expirée/falsifiée/absente testé — banner correct, /write & /stock & /journal intacts
```

---

*Sovereign-Spike Phase 0 — EIGSI × AL BARAA CONSULTING — Jesse MPIGA-ODOUMBA (Promo 2026)*
