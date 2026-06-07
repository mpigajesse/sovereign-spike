# LOT 5 — Failover automatique par quorum (Windows)

> Cible : **100 % Windows**. Cluster de 3 machines (PC primary + VM1 + VM2),
> toutes sous Windows 11 + PostgreSQL 18. Aucun Linux, aucun Patroni, aucun etcd.

## 1. Ce que ce lot prouve (§7.4)

| Critère | Avant LOT 5 | Après LOT 5 |
|---------|-------------|-------------|
| #4a Failover **manuel** sans perte | ✅ prouvé | ✅ (inchangé) |
| #4b Failover **automatique** par quorum | ❌ manquant | ✅ **superviseur Rust** |
| #5 Pas de split-brain sous coupure | 🟡 fencing *après coup* | ✅ quorum *préventif* + fencing |

## 2. Pourquoi un superviseur natif Rust plutôt que Patroni/etcd

1. **Cible Windows** — Patroni est instable/non supporté proprement sous Windows ;
   il aurait fallu réécrire le nœud le plus important (le primary) sur Linux.
2. **Pas d'infra lourde** — objectif PFE : pas d'orchestrateur tiers ni de daemon
   etcd supplémentaire à déployer et superviser sur chaque machine.
3. **Réutilisation du socle prouvé** — la promotion (`pg_ctl promote`), la
   réplication WAL et le fencing par époque (`EpochGuard`, critère #9) existent
   déjà. Le superviseur ne fait qu'**automatiser la décision** de promotion.
4. **Anti-split-brain des deux côtés** :
   - *avant* la bascule : un nœud en **minorité** (partition réseau) ne peut pas
     atteindre la majorité stricte → il **refuse** de se promouvoir ;
   - *après* la bascule : l'ancien primary qui revient est **fencé** par l'époque.

## 3. Architecture du cluster

```
   PC (primary)                VM1 (standby, rang 0)        VM2 (standby, rang 1)
   PostgreSQL primary  ──WAL──► PostgreSQL standby   ──WAL──► PostgreSQL standby
   noeud actif :3000           (réplique)                    (réplique)
   supervisor  :3100  ◄──────► supervisor :3100     ◄──────► supervisor :3100
        primary                   standby                       standby
```

Chaque superviseur expose :
- `GET  /health` — vivacité du superviseur
- `GET  /supervisor/status` — rôle, rang, terme, primary vu vivant ?
- `POST /vote` — demande de vote (interne au cluster)

## 4. Logique de décision (résumé)

1. Les superviseurs **standby** pinguent `http://<primary>:3000/health` toutes les
   3 s. Après **3 échecs consécutifs** (≈ 9 s), le primary est jugé mort.
2. Le standby de **rang le plus faible encore joignable** se déclare candidat
   (rang 0 = VM1 = premier successeur). Les autres attendent → pas d'élection
   concurrente.
3. Le candidat ouvre un **terme** d'élection et demande un vote à ses pairs.
   Un pair n'accorde sa voix que s'il considère **lui aussi** le primary mort,
   et une seule fois par terme.
4. Promotion **uniquement si majorité stricte** atteinte (⌊3/2⌋+1 = **2 voix**,
   son propre vote inclus).
5. Le gagnant exécute `pg_ctl promote` (PostgreSQL) puis `POST /epoch/promote`
   (incrément d'époque → fencing de l'ancien primary).

## 5. Procédure de déploiement (3 machines Windows)

### Pré-requis (déjà en place)
- PostgreSQL 18 sur les 3 machines.
- Réplication WAL configurée : VM1 et VM2 sont standby du PC
  (cf. `03_setup_standby_win.ps1`, à exécuter sur **chaque** VM).
- Le nœud actif tourne sur le PC (`02_start_active.ps1`).

> ⚠️ Les standbys VM1/VM2 doivent aussi pouvoir exposer un nœud actif local sur
> `:3000` après promotion. Le superviseur appelle `POST http://127.0.0.1:3000/epoch/promote`
> sur la machine promue. Lancer un `sovereign-node-active` (pointant la base
> locale promue) avant ou juste après la bascule.

### Démarrage des superviseurs

**Option A — depuis l'interface (recommandé, 0.1.14+).** L'assistant d'installation
embarque désormais le failover : en choisissant le rôle **Nœud Actif** ou **Nœud
Standby**, une section « ⚡ Failover automatique (quorum) » apparaît. Renseigner
l'IP de cette machine (standby), les IP des 2 autres machines, et le rang (standby).
Le superviseur démarre automatiquement à la fin de l'installation — aucun script
manuel. Son `node_id` et ses pairs sont dérivés des IP (cohérents avec les slots PG).

**Option B — via le script embarqué `04_start_supervisor.ps1` :**
```powershell
.\scripts\windows\04_start_supervisor.ps1 `
    -Role primary -NodeId pc `
    -PrimaryIp 192.168.200.1 `
    -Peers "vm1@192.168.200.2:3100,vm2@192.168.200.3:3100"
```

**Sur VM1 (standby, premier successeur) :**
```powershell
.\scripts\windows\04_start_supervisor.ps1 `
    -Role standby -NodeId vm1 -Rank 0 `
    -PrimaryIp 192.168.200.1 `
    -Peers "pc@192.168.200.1:3100,vm2@192.168.200.3:3100"
```

**Sur VM2 (standby, second successeur) :**
```powershell
.\scripts\windows\04_start_supervisor.ps1 `
    -Role standby -NodeId vm2 -Rank 1 `
    -PrimaryIp 192.168.200.1 `
    -Peers "pc@192.168.200.1:3100,vm1@192.168.200.2:3100"
```

## 6. Scénario de démonstration

### 6.1 État initial (cluster sain)
```powershell
# Sur n'importe quelle machine
Invoke-RestMethod http://192.168.200.2:3100/supervisor/status
# vm1 -> role=standby, rank=0, primary_alive=true, promoted=false
Invoke-RestMethod http://192.168.200.1:3000/epoch
# -> epoch=1, primary_host=initial
```

### 6.2 Panne du primary (couper le PC actif)
Arrêter brutalement le nœud actif sur le PC (Ctrl+C ou `taskkill`).

```powershell
# Sur VM1, observer les logs du superviseur :
#   "heartbeat primary échoué" x3
#   "lancement d'une élection de promotion"
#   "voix accordée" (de vm2)
#   "QUORUM ATTEINT — promotion de ce nœud en primary"
#   "promotion réussie"
```

### 6.3 Vérifier la nouvelle topologie
```powershell
Invoke-RestMethod http://192.168.200.2:3100/supervisor/status
# vm1 -> promoted=true
Invoke-RestMethod http://192.168.200.2:3000/epoch
# -> epoch=2  (incrémentée !)
```

### 6.4 Preuve anti-split-brain : l'ancien primary revient
Relancer le nœud actif sur le PC (époque locale = 1).
```powershell
# Tenter une écriture sur l'ANCIEN primary
Invoke-RestMethod http://192.168.200.1:3000/write -Method POST `
    -ContentType "application/json" `
    -Body '{"op_type":"stock_adjust","item_id":"X","quantity":1}'
# -> HTTP 503 : "nœud dégradé — fencing : époque obsolète (locale=1, DB=2)"
```
**Ce que ça prouve :** l'ancien primary est **automatiquement neutralisé**. Pas
de double écriture, pas de divergence.

### 6.5 Preuve anti-split-brain préventive : partition réseau
Si VM1 est **isolée** (ne voit ni le PC ni VM2), elle juge le primary mort mais
ne récolte **aucune voix** → 1 voix sur 3 < majorité (2) → **promotion refusée**.
Les logs affichent :
```
quorum NON atteint (minorité ou partition) — promotion refusée (anti-split-brain)
```

## 7. Limites assumées (honnêteté de soutenance)

- La **re-bascule** (réintégrer l'ancien primary comme standby de VM1) reste
  manuelle : `pg_basebackup` depuis le nouveau primary. C'est le comportement
  attendu d'un cluster actif/passif — la reconstruction d'un nœud déchu n'est
  jamais automatique sans risque.
- Le **2ᵉ passif (VM2) doit aussi être réintégré manuellement** après une
  bascule : il restait standby de l'ancien primary (mort), donc il faut le
  re-pointer vers le nouveau primary (VM1) par `pg_basebackup`. En attendant,
  son superviseur **détecte la promotion de VM1** et **s'abstient** de toute
  élection (pas de double-promotion).
- Le superviseur suppose que la machine promue peut servir un nœud actif local
  sur `:3000` (base PostgreSQL fraîchement promue).
- Cluster validé à **3 nœuds** (majorité = 2). La logique de quorum est générique
  (`majority(n) = n/2 + 1`) et couverte par tests unitaires.

## 7bis. Garde-fous anti-split-brain (récapitulatif)

| Situation | Mécanisme | Effet |
|---|---|---|
| Partition réseau, nœud en minorité | quorum (majorité stricte) | refuse de se promouvoir |
| Ancien primary qui revient | fencing par époque | 503 sur écriture (époque obsolète) |
| 2ᵉ standby après promotion du 1ᵉʳ | sondage des pairs + un primary ne vote pas | abandon de l'élection (pas de 2ᵉ primary) |
| Nouveau primary bloqué en sync | relâchement auto de `synchronous_standby_names` | écritures non bloquées |

## 8. Deux passifs : slots de réplication distincts (CRITIQUE)

L'architecture officielle impose **2 nœuds passifs** par cluster. Chaque standby
DOIT avoir un **slot de réplication** et un **`application_name` distincts** sur
le primary, sinon les deux VMs entrent en collision.

`03_setup_standby_win.ps1` dérive automatiquement ces identifiants du **dernier
octet de l'IP locale** :
- VM 192.168.200.**2** → slot `sovereign_slot_2`, name `sovereign_standby_2`
- VM 192.168.200.**3** → slot `sovereign_slot_3`, name `sovereign_standby_3`

Vérification sur le primary après configuration des 2 standbys :
```powershell
psql -U postgres -d sovereign_active -c `
  "SELECT application_name, client_addr, state, sync_state FROM pg_stat_replication;"
# -> doit afficher DEUX lignes : sovereign_standby_2 et sovereign_standby_3
```

La réplication synchrone (`synchronous_standby_names = '*'`) attend l'accusé d'**au
moins un** des deux passifs → le cluster survit à la perte d'un passif (HA réelle).

## 9. Tests


Logique de quorum entièrement testée (fonctions pures) :
```powershell
cargo test --bin sovereign-supervisor
# 17 tests : majorité, quorum, candidature préférée, ledger de vote
#            (un vote/terme), détecteur de panne à seuil, parsing config.
```
