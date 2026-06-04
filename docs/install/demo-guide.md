# Guide de Démonstration — Sovereign-Spike Phase 0

**Validation :** 13/13 tests E2E réussis — 2026-06-04  
**Architecture :** cluster actif/passif PostgreSQL + relais éditeur aveugle

---

## 0. Architecture démontrée

```
┌──────────────────────────────────────┐
│  NŒUD ACTIF — Windows 11            │
│  PostgreSQL 18 (primary WAL)         │
│  sovereign-node-active :3000         │
│                                      │
│  • Sérialise toutes les écritures    │
│  • Chiffre chaque op → blob CBOR     │
│    (XChaCha20-Poly1305 / libsodium)  │
│  • Stocke en clair dans PostgreSQL   │
│  • Expose le journal chiffré via API │
└──────────┬───────────────────────────┘
           │  PostgreSQL streaming replication
           │  (WAL binaire, chiffré en transit TLS)
           ▼
┌──────────────────────────────────────┐
│  NŒUD PASSIF — Ubuntu 26.04         │
│  PostgreSQL 18 standby (hot_standby) │
│  sovereign-node-passive :3001        │
│                                      │
│  • Réplique le WAL depuis Windows    │
│  • Interroge /journal toutes 5s      │
│  • Déchiffre avec la DEK partagée    │
│  • Stocke localement dans SQLite     │
│  • Disponible hors-ligne             │
└──────────────────────────────────────┘

┌──────────────────────────────────────┐
│  RELAIS AVEUGLE — Kali Linux        │
│  sovereign-relay :4000               │
│                                      │
│  • Reçoit des blobs opaques          │
│  • Ne charge AUCUNE DEK              │
│  • Ne peut pas lire le contenu       │
│  → Propriété zéro-knowledge prouvée  │
└──────────────────────────────────────┘
```

---

## 1. Démarrage des 3 nœuds

### Nœud actif (Windows)

```powershell
# Terminal 1 — Windows
cd D:\PFE-FINAL\pfe\sovereign-spike
.\scripts\windows\02_start_active.ps1
```

### Nœud passif (Ubuntu)

```bash
# Terminal Ubuntu
sudo bash /opt/sovereign-spike/scripts/ubuntu/03_start_passive.sh
```

### Relais (Kali)

```bash
# Terminal Kali
bash /opt/sovereign-spike/scripts/kali/02_start_relay.sh
```

### Vérification rapide

```powershell
# Depuis Windows
curl http://192.168.200.1:3000/health      # → ok
curl http://192.168.200.130:3001/health    # → ok (passif)
curl http://192.168.200.128:4000/health    # → {"role":"relay-aveugle",...}
```

---

## 2. Scénarios de démonstration

### Scénario A — Écriture et réplication

```powershell
# 1. Ajuster le stock
Invoke-RestMethod http://192.168.200.1:3000/write -Method POST `
    -ContentType "application/json" `
    -Body '{"op_type":"stock_adjust","item_id":"PANTALON-L","quantity":100}'
# → {"status":"committed","seq":N,"op_id":"..."}

# 2. Lire le stock sur le nœud actif
Invoke-RestMethod http://192.168.200.1:3000/stock/PANTALON-L
# → {"item_id":"PANTALON-L","quantity":100}

# 3. Attendre 5-10s (intervalle de sync passif)
Start-Sleep -Seconds 10

# 4. Lire le stock sur le passif — doit être identique
Invoke-RestMethod http://192.168.200.130:3001/stock/PANTALON-L
# → {"item_id":"PANTALON-L","quantity":100}
```

**Ce que ça prouve :** la réplication WAL (PostgreSQL) + synchronisation journal (SQLite) fonctionnent de bout en bout avec la même DEK.

---

### Scénario B — Anti-survente (intégrité métier)

```powershell
# Stock actuel = 100
# Tenter une vente supérieure au stock
Invoke-RestMethod http://192.168.200.1:3000/write -Method POST `
    -ContentType "application/json" `
    -Body '{"op_type":"sale","item_id":"PANTALON-L","quantity":200}'
# → 409 Conflict : stock insuffisant
```

**Ce que ça prouve :** le nœud actif est le seul point d'écriture — il applique les règles métier avant de committer.

---

### Scénario C — Idempotence (résistance aux doublons)

```powershell
$op_id = [guid]::NewGuid().ToString()
$body = @{ op_type="sale"; item_id="PANTALON-L"; quantity=1; op_id=$op_id } | ConvertTo-Json

# Première soumission
$r1 = Invoke-RestMethod http://192.168.200.1:3000/write -Method POST -ContentType "application/json" -Body $body
# → seq=N

# Même op_id soumis une deuxième fois
$r2 = Invoke-RestMethod http://192.168.200.1:3000/write -Method POST -ContentType "application/json" -Body $body
# → seq=N (identique — opération dédupliquée)

$r1.seq -eq $r2.seq  # → True
```

**Ce que ça prouve :** les doublons réseau ne créent pas de double-débit.

---

### Scénario D — Journal chiffré (opaques pour le relais)

```powershell
# Lire le journal depuis le nœud actif
$journal = Invoke-RestMethod "http://192.168.200.1:3000/journal?after_seq=0&limit=3"

# Afficher un blob
$journal[0]
# → @{seq=1; blob_nonce="032ab89e8e8b5d6b..."; blob_ciphertext="4f2a..."}

# Le blob_nonce et blob_ciphertext sont du hex pur — illisibles sans la DEK
```

**Ce que ça prouve :** même si un attaquant intercepte le journal, les données sont opaques.

---

### Scénario E — Zéro-knowledge du relais

```powershell
# Pousser un blob factice vers le relais
$blob = @{
    seq             = 9999
    blob_nonce      = "aa" * 24
    blob_ciphertext = "deadbeef" * 16
}
Invoke-RestMethod http://192.168.200.128:4000/blobs -Method POST `
    -ContentType "application/json" `
    -Headers @{"X-Relay-Key"="sovereign-spike-relay-key-2026"} `
    -Body ($blob | ConvertTo-Json)
# → {"status":"stored"}

# Le relais retransmet le blob sans le déchiffrer
Invoke-RestMethod "http://192.168.200.128:4000/blobs?after_seq=9998&limit=1"
# → [{seq=9999, blob_nonce="aaaa...", blob_ciphertext="deadbeef..."}]
```

**Ce que ça prouve :** le relais (côté éditeur) ne voit que des blobs opaques. Il n'a jamais eu accès à la DEK.

---

### Scénario F — Époque de fencing

```powershell
Invoke-RestMethod http://192.168.200.1:3000/epoch
# → {"epoch":1,"primary_host":"initial"}
```

**Ce que ça prouve :** en cas de failover, l'époque change — les anciens primaires avec une époque inférieure sont fencés (bloqués automatiquement).

---

## 3. Test E2E automatisé

```powershell
# Lance tous les scénarios ci-dessus automatiquement
.\scripts\windows\03_test_e2e.ps1
```

Résultat validé :
```
═══════════════════════════════════════════════════
  13/13 tests réussis
  BANC D'ESSAI PHASE 0 ENTIÈREMENT VALIDÉ ✓
  Crypto + Sérialisation + Réplication + Zéro-Knowledge
═══════════════════════════════════════════════════
```

---

## 4. Monitoring en temps réel

### Réplication PostgreSQL (depuis Windows)

```powershell
# Voir les standbys connectés
$env:PGPASSWORD = "admin"
& "C:\Program Files\PostgreSQL\18\bin\psql.exe" -U postgres -h 127.0.0.1 -p 5432 `
    -c "SELECT client_addr, usename, application_name, state, sync_state, sent_lsn, write_lsn FROM pg_stat_replication;"
```

Résultat attendu :
```
client_addr    | application_name   | state     | sync_state
192.168.200.130 | sovereign_standby | streaming | async
```

### Slot de réplication

```powershell
& "C:\Program Files\PostgreSQL\18\bin\psql.exe" -U postgres -h 127.0.0.1 -p 5432 `
    -c "SELECT slot_name, active, restart_lsn FROM pg_replication_slots;"
```

### Sync status du passif

```powershell
Invoke-RestMethod http://192.168.200.130:3001/sync/status
# → {"last_seq":N,"lag_ms":M}
```

---

## 5. Arrêt propre

```powershell
# Windows — Ctrl+C dans le terminal du nœud actif
# ou
Get-Process -Name "sovereign-node-active" | Stop-Process
```

```bash
# Ubuntu — Ctrl+C dans le terminal du passif
# PostgreSQL standby reste actif (normal)

# Kali — Ctrl+C dans le terminal du relais
```

---

## Checklist de démonstration

```
Démarrage
[ ] GET /health actif  → "ok"
[ ] GET /health passif → "ok (passif)"
[ ] GET /health relais → role=relay-aveugle
[ ] pg_stat_replication → state=streaming

Scénarios
[ ] A. Stock actif = Stock passif après 10s
[ ] B. Anti-survente → 409 Conflict
[ ] C. Idempotence → même seq les deux fois
[ ] D. Journal → blobs hex opaques
[ ] E. Relais → blob stocké sans déchiffrement
[ ] F. Époque → epoch ≥ 1

E2E automatisé
[ ] 03_test_e2e.ps1 → 13/13 réussis
```

---

*Sovereign-Spike Phase 0 — EIGSI × AL BARAA CONSULTING — Jesse MPIGA-ODOUMBA (Promo 2026)*
