# Guide de Déploiement — Banc d'Essai Sovereign-Spike

Architecture validée en Phase 0 (2026-06-04) : cluster actif/passif PostgreSQL + relais éditeur aveugle.

**Durée estimée :** 30–60 min selon la vitesse de téléchargement.

---

## Architecture des 3 nœuds

> **Topologie réelle du banc d'essai : 100 % Windows 11.** Les trois machines
> (PC physique + 2 VM VMware sur le réseau host-only VMnet1) tournent sous
> Windows 11 — il n'y a **aucune machine Linux** dans ce banc d'essai.

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
│  NŒUD PASSIF — Windows 11 (VM1) │  VM VMware VMnet1
│  PostgreSQL 18 (standby)         │  192.168.200.133
│  sovereign-node-passive :3001    │
│  SQLite (réplique locale)        │
└──────────────────────────────────┘

┌──────────────────────────────────┐
│  RELAIS AVEUGLE — Windows 11    │  VM VMware VMnet1
│  (VM2)                           │  192.168.200.134
│  sovereign-relay :4000           │
│  Zéro-knowledge : aucune DEK    │
└──────────────────────────────────┘
```

| Nœud | Machine | IP LAN (VMnet1) | Port |
|------|---------|-----------------|------|
| Actif | Windows 11 — PC physique | `192.168.200.1` | `:3000` |
| Passif | Windows 11 — VM1 | `192.168.200.133` | `:3001` |
| Relais | Windows 11 — VM2 | `192.168.200.134` | `:4000` |

---

## Prérequis matériels

| Exigence | Minimum |
|----------|---------|
| RAM | 4 Go par machine |
| Disque | 10 Go libres |
| Réseau | LAN VMnet1 host-only (192.168.200.0/24) |
| OS | Windows 11 sur les 3 machines (PC, VM1, VM2) |

---

## Ordre de déploiement

```
1. Windows (actif, PC)   → 01_setup_pg.ps1        → 02_start_active.ps1
2. Windows (passif, VM1) → 03_setup_standby_win.ps1 → cargo run --bin sovereign-node-passive
3. Windows (relais, VM2) → cargo run --bin sovereign-relay  (pas de script dédié — voir Étape 3)
4. Test E2E              → 03_test_e2e.ps1 (depuis Windows, IP à jour : 192.168.200.133/.134)
5. Licence (optionnel)   → sovereign-control + LICENSE_AUTHORITY_PUBKEY_HEX/LICENSE_TOKEN_PATH
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
- Configure `pg_hba.conf` pour autoriser le standby (VM1, `192.168.200.133`)
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

## Étape 2 — Nœud passif (Windows 11 — VM1, `192.168.200.133`)

### 2.1 Prérequis

- PostgreSQL 18 installé (`C:\Program Files\PostgreSQL\18\`)
- Rust toolchain (`rustup` + `cargo`)
- Git, dépôt cloné dans `D:\PFE-FINAL\pfe\sovereign-spike`

### 2.2 Configurer le standby PostgreSQL

```powershell
cd D:\PFE-FINAL\pfe\sovereign-spike
.\scripts\windows\03_setup_standby_win.ps1
```

Ce script (auto-élévation UAC) :
- Détecte l'IP locale VMnet1 (`192.168.200.133`) et dérive un slot/`application_name`
  **uniques** depuis le dernier octet (`sovereign_slot_133`, `sovereign_standby_133`) —
  indispensable pour cohabiter avec un futur 2ᵉ passif sans collision
- Crée le slot de réplication sur le primary (`192.168.200.1:5432`)
- Exécute `pg_basebackup` puis configure `postgresql.auto.conf` + `standby.signal`
- Démarre PostgreSQL en mode standby (streaming replication)

### 2.3 Démarrer le nœud passif

Pas de script PowerShell dédié pour ce binaire — compilation puis lancement manuel,
même schéma que `02_start_active.ps1` :

```powershell
cd D:\PFE-FINAL\pfe\sovereign-spike
cargo build --release --bin sovereign-node-passive

$env:DATABASE_URL = "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active"
$env:LISTEN_ADDR  = "0.0.0.0:3001"
# SOVEREIGN_DEK_HEX : IDENTIQUE à celle du nœud actif (cf. shared.env)

.\target\release\sovereign-node-passive.exe
```

Vérification :
```powershell
Invoke-RestMethod http://192.168.200.133:3001/health
# → ok (passif)
```

---

## Étape 3 — Relais aveugle (Windows 11 — VM2, `192.168.200.134`)

> Le relais est un **binaire séparé, zéro-knowledge** : il ne charge **aucune** clé
> crypto (ni DEK, ni clé d'autorité de licence) et ne stocke que des blobs opaques.
> Il n'existe pas de script PowerShell dédié pour le démarrer (seul un script bash
> existait pour la VM Linux désormais retirée du banc d'essai) — lancement manuel,
> même schéma que `sovereign-control` à l'Étape 5.1.

### 3.1 Prérequis

- Rust toolchain (`rustup` + `cargo`), libsodium
- Git, dépôt cloné dans `D:\PFE-FINAL\pfe\sovereign-spike`

> Un binaire précompilé existe aussi en sidecar Tauri
> (`crates/tauri-app/src-tauri/binaries/sovereign-relay-x86_64-pc-windows-msvc.exe`)
> et peut être copié/exécuté directement si l'on préfère éviter la compilation locale.

### 3.2 Compiler et démarrer le relais

```powershell
cd D:\PFE-FINAL\pfe\sovereign-spike
cargo build --release --bin sovereign-relay

$env:RELAY_LISTEN_ADDR = "0.0.0.0:4000"
$env:RELAY_API_KEY     = "sovereign-spike-relay-key-2026"
$env:RELAY_MAX_BLOBS   = "100000"
# Pas de SOVEREIGN_DEK_HEX, pas de LICENSE_* : le relais ne les charge jamais (zero-knowledge)

.\target\release\sovereign-relay.exe
```

Vérification :
```powershell
Invoke-RestMethod http://192.168.200.134:4000/health
# → { "role": "relay-aveugle" / "amane-relay", "status": "ok" }
```

---

## Étape 4 — Test E2E complet

Depuis Windows (PC actif), une fois les 3 nœuds démarrés :

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
RELAY_URL=http://192.168.200.134:4000
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

Les 3 machines sont sous Windows 11 — une seule syntaxe (`New-NetFirewallRule`,
PowerShell admin), à adapter selon le rôle de chaque machine.

### PC physique — nœud actif (`192.168.200.1`)

```powershell
New-NetFirewallRule -DisplayName "Sovereign-Active"    -Direction Inbound -Protocol TCP -LocalPort 3000 -Action Allow
New-NetFirewallRule -DisplayName "Sovereign-PG-Replic" -Direction Inbound -Protocol TCP -LocalPort 5432 -Action Allow
```

### VM1 — nœud passif (`192.168.200.133`)

```powershell
New-NetFirewallRule -DisplayName "Sovereign-Passive"   -Direction Inbound -Protocol TCP -LocalPort 3001 -Action Allow
New-NetFirewallRule -DisplayName "Sovereign-PG-Standby" -Direction Inbound -Protocol TCP -LocalPort 5432 -Action Allow
```

### VM2 — relais aveugle (`192.168.200.134`)

```powershell
New-NetFirewallRule -DisplayName "Sovereign-Relay" -Direction Inbound -Protocol TCP -LocalPort 4000 -Action Allow
```

---

## Checklist de validation

```
Nœud actif (Windows)
[ ] PostgreSQL 18 démarré — service postgresql-x64-18
[ ] Base sovereign_active créée, rôle replicator configuré
[ ] Slot de réplication sovereign_slot créé
[ ] sovereign-node-active.exe UP — GET /health → "ok"

Nœud passif (Windows — VM1, 192.168.200.133)
[ ] pg_basebackup terminé — C:\Program Files\PostgreSQL\18\data peuplé
[ ] standby.signal présent (créé par 03_setup_standby_win.ps1, sans BOM)
[ ] PostgreSQL standby UP — pg_isready localhost OK
[ ] sovereign-node-passive.exe UP — GET /health → "ok (passif)"
[ ] pg_stat_replication sur le PC actif → sovereign_standby_133, state=streaming

Relais (Windows — VM2, 192.168.200.134)
[ ] sovereign-relay compilé — target\release\sovereign-relay.exe
[ ] sovereign-relay UP — GET /health → role=relay-aveugle / amane-relay
[ ] Aucune SOVEREIGN_DEK_HEX ni LICENSE_* chargée (zero-knowledge confirmé)

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
