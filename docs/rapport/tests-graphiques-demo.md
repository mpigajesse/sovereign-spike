# Guide de Démonstration Graphique — SDA-Prototype

---

## Architecture — Ce qu'il faut comprendre avant la démo

```
┌─────────────────────────────────────────────────────────────────┐
│           ARCHITECTURE P2P SYMÉTRIQUE — AUCUN SERVEUR CENTRAL   │
│                                                                 │
│   Win11 (192.168.200.1)      Ubuntu (192.168.200.130)           │
│   ┌──────────────────┐       ┌──────────────────┐              │
│   │ nginx (443)      │       │ nginx (443)      │              │
│   │ backend (8000)   │       │ backend (8000)   │              │
│   │ frontend (3000)  │       │ frontend (3000)  │              │
│   │ syncthing (22000)│◄─────►│ syncthing (22000)│             │
│   └──────────────────┘       └──────────────────┘              │
│           ▲                          ▲                          │
│           │                          │                          │
│           └──────────┐   ┌───────────┘                         │
│               Kali (192.168.200.128)                            │
│               ┌──────────────────┐                             │
│               │ nginx (443)      │                             │
│               │ backend (8000)   │                             │
│               │ frontend (3000)  │                             │
│               │ syncthing (22000)│                             │
│               └──────────────────┘                             │
│                                                                 │
│  Chaque nœud est IDENTIQUE et AUTONOME.                        │
│  Syncthing réplique les fichiers entre TOUS les pairs.         │
│  Aucun nœud ne coordonne les autres.                           │
└─────────────────────────────────────────────────────────────────┘
```

**Règle fondamentale de cette démo :**
> Chaque nœud est montré **sur son propre écran**, avec son propre navigateur ouvert sur `https://localhost/`. On ne pilote jamais un nœud depuis un autre. La communication inter-nœuds se fait **uniquement via Syncthing** (partage de fichiers P2P), jamais via les APIs.

**Prérequis :**
- Les 3 nœuds : `docker compose up -d` exécuté sur chacun
- Certificat `sda-client.p12` importé dans Chrome **sur chaque nœud**
- Syncthing : 100% synchronisé sur les 3 nœuds (2 pairs verts)

**Interfaces disponibles sur chaque nœud (accès local uniquement) :**

| Interface | URL | Contenu |
|-----------|-----|---------|
| **Dashboard SDA** | `https://localhost/` | État, coffre-fort, métriques |
| **Syncthing GUI** | `http://localhost:8384` | Pairs connectés, progression sync |
| **Swagger UI** | `https://localhost/docs` | API interactive, résultats JSON |

---

## SCÉNARIO 1 — Chaque nœud est autonome et opérationnel

**Message clé :** *"Il n'y a pas de serveur central. Chaque machine est un SDA complet et indépendant. Voici les 3 nœuds actifs — chacun montré sur son propre écran."*

### Ce qu'on fait

**Disposition physique (3 écrans ou Alt+Tab entre VMware) :**

| Écran | Machine | URL ouverte |
|-------|---------|-------------|
| Écran principal | Win11 (PC physique) | `https://localhost/` |
| Fenêtre VMware 1 | Ubuntu | `https://localhost/` ← son propre navigateur |
| Fenêtre VMware 2 | Kali | `https://localhost/` ← son propre navigateur |

> Chaque nœud accède à **son propre backend local** — jamais à celui d'un autre.

Sur **chaque nœud indépendamment** :
1. Ouvrir Chrome → `https://localhost/`
2. Le dashboard SDA s'affiche avec `"status": operational`, `"offline_ready": true`
3. Vérifier : **"Pairs connectés 2/2"** dans la sidebar → cluster complet

### Captures à prendre

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 1a | Dashboard `https://localhost/` — sidebar "Pairs 2/2" | Win11 | `s1_dashboard_win11.png` |
| 1b | Dashboard `https://localhost/` — sidebar "Pairs 2/2" | Ubuntu | `s1_dashboard_ubuntu.png` |
| 1c | Dashboard `https://localhost/` — sidebar "Pairs 2/2" | Kali | `s1_dashboard_kali.png` |
| 1d | **Mosaïque : 3 fenêtres côte à côte, chacune sur son `localhost`** | Global | `s1_mosaic_3nodes.png` |

---

## SCÉNARIO 2 — Réplication P2P : une donnée injectée sur Win11 arrive sur Ubuntu et Kali

**Message clé :** *"Je vais injecter une donnée sur Win11. Elle est stockée localement dans la base DuckDB de Win11, exportée en fichier Parquet, puis Syncthing la réplique automatiquement vers Ubuntu et Kali — sans qu'aucun nœud ne soit le serveur."*

### Ce qu'on fait — étape par étape

**Étape 1 — État initial sur les 3 nœuds**

Sur **Win11** → `http://localhost:8384` (Syncthing GUI) :
- Les 2 pairs Ubuntu et Kali sont en **vert "À jour"**

→ **Capturer `s2_syncthing_initial_win11.png`**

**Étape 2 — Injection sur Win11 uniquement (via Swagger local)**

Sur **Win11** → `https://localhost/docs` → `POST /api/v1/data/ingest` → **"Try it out"** :
```json
{
  "tenant_id": "sda_demo",
  "data": {
    "source": "node1_win11",
    "type": "temperature",
    "capteur": "SENSOR-W11-001",
    "valeur_celsius": 23.5,
    "site": "Datacenter-A",
    "timestamp": "2026-05-28T14:30:00Z"
  }
}
```

Réponse : `"status": "success"` + `record_hash` SHA-256 + `audit_id`

→ **Capturer `s2_swagger_ingest_win11.png`** (réponse JSON avec `record_hash`)

**Étape 3 — Observer la propagation (attendre 15–30 s)**

Sur **Ubuntu** → `http://localhost:8384` :
- Barre de progression → "En cours…" → revient "À jour" avec `+1 fichier`

→ **Capturer `s2_syncthing_ubuntu_synced.png`**

Sur **Kali** → `http://localhost:8384` : même observation.
→ **Capturer `s2_syncthing_kali_synced.png`**

### Captures récapitulatives

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 2a | Syncthing initial — 2 pairs verts | Win11 | `s2_syncthing_initial_win11.png` |
| 2b | Swagger — injection + réponse `record_hash` | Win11 | `s2_swagger_ingest_win11.png` |
| 2c | Syncthing Ubuntu — fichier arrivé "À jour" | Ubuntu | `s2_syncthing_ubuntu_synced.png` |
| 2d | Syncthing Kali — fichier arrivé "À jour" | Kali | `s2_syncthing_kali_synced.png` |

---

## SCÉNARIO 3 — Tolérance aux pannes : un nœud tombe, les autres continuent

**Message clé :** *"Aucun nœud n'est indispensable. Si Ubuntu tombe, Win11 et Kali continuent normalement. À la reconnexion, Ubuntu rattrape automatiquement tout ce qu'il a manqué."*

### Ce qu'on fait — étape par étape

**Étape 1 — Montrer le cluster complet**

Sur **Win11** Syncthing GUI : Ubuntu ✅ Kali ✅ — tous en vert "À jour"
→ **Capturer `s3_all_connected_win11.png`**

**Étape 2 — Mettre Ubuntu hors service**

Dans VMware Player : VM Ubuntu → **Suspend**

Sur **Win11** Syncthing GUI (après 10 s) :
- Ubuntu : **rouge "Déconnecté"** | Kali : vert "À jour" (non affecté)

→ **Capturer `s3_ubuntu_offline_win11.png`**

**Étape 3 — Win11 et Kali continuent de travailler**

Sur **Win11** → `https://localhost/docs` → `POST /api/v1/data/ingest` :
```json
{
  "tenant_id": "resilience_win11",
  "data": { "source": "node1_win11", "message": "Win11 opérationnel malgré la panne Ubuntu", "capteur": "SENSOR-W11-002" }
}
```
→ **Capturer `s3_win11_works_without_ubuntu.png`**

**Étape 4 — Reconnecter Ubuntu**

Dans VMware : **Resume** la VM Ubuntu.

Sur **Win11** Syncthing GUI (attendre 15–30 s) :
- Ubuntu repasse en **vert** — les fichiers manqués sont rattrapés.

→ **Capturer `s3_ubuntu_back_online.png`**

### Captures récapitulatives

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 3a | Syncthing — tous connectés | Win11 | `s3_all_connected_win11.png` |
| 3b | **Syncthing — Ubuntu rouge "Déconnecté"** | Win11 | `s3_ubuntu_offline_win11.png` |
| 3c | Swagger — ingestion réussie sans Ubuntu | Win11 | `s3_win11_works_without_ubuntu.png` |
| 3d | Syncthing — Ubuntu de retour "À jour" | Win11 | `s3_ubuntu_back_online.png` |
| 3e | Dashboard Ubuntu — données rattrapées | Ubuntu | `s3_ubuntu_recovered_dashboard.png` |

---

## SCÉNARIO 4 — Mode offline total : Kali isolé du réseau

**Message clé :** *"Un nœud sans réseau reste 100% opérationnel en local. Les données créées offline sont synchronisées automatiquement dès la reconnexion — c'est le paradigme offline-first."*

### Ce qu'on fait — étape par étape

**Étape 1 — Déconnecter Kali du réseau VMnet1**

Dans VMware → VM Kali → **VM → Settings → Network Adapter → décocher "Connected"**

Sur **Win11** Syncthing GUI → Kali passe en rouge "Déconnecté"
→ **Capturer `s4_kali_isolated_win11.png`**

**Étape 2 — Kali fonctionne toujours en local**

Sur **Kali** → `https://localhost/` : dashboard normal, `offline_ready: true`

Sur **Kali** → `https://localhost/docs` → `POST /api/v1/data/ingest` :
```json
{
  "tenant_id": "kali_offline",
  "data": { "source": "node3_kali", "statut": "créé_hors_ligne", "capteur": "SENSOR-K03" }
}
```
→ **Capturer `s4_kali_ingest_offline.png`** — injection réussie sans réseau !

**Étape 3 — Reconnecter Kali**

Dans VMware : re-cocher **"Connected"** sur l'adaptateur réseau.

Sur **Win11** Syncthing GUI (attendre 15–30 s) : Kali repasse en vert, fichier arrivé.

→ **Capturer `s4_kali_reconnected.png`**

### Captures récapitulatives

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 4a | Syncthing — Kali rouge "Déconnecté" | Win11 | `s4_kali_isolated_win11.png` |
| 4b | Dashboard Kali — opérationnel hors-ligne | Kali | `s4_kali_dashboard_offline.png` |
| 4c | Swagger Kali — injection réussie hors-ligne | Kali | `s4_kali_ingest_offline.png` |
| 4d | Syncthing Win11 — Kali reconnecté, sync | Win11 | `s4_kali_reconnected.png` |
| 4e | Dashboard Win11 — donnée Kali arrivée | Win11 | `s4_win11_kali_data_arrived.png` |

---

## SCÉNARIO 5 — Sécurité mTLS : authentification mutuelle

**Message clé :** *"L'accès à chaque nœud est protégé par TLS mutuel — seul un client avec un certificat signé par notre CA interne peut accéder aux données."*

### Ce qu'on fait

**Test A — Accès refusé sans certificat**

Sur **Win11** → Firefox en navigation privée (Ctrl+Maj+P) → `https://localhost/` → annuler la sélection du certificat :
- Résultat : `400 No required SSL certificate was sent`

→ **Capturer `s5_rejected_no_cert_win11.png`**

**Test B — Accès autorisé avec certificat**

Sur **Win11** → Chrome → `https://localhost/` → dashboard visible
→ **Capturer `s5_accepted_with_cert_win11.png`**

**Test C — Détails TLS**

1. Cliquer sur le **cadenas** → "La connexion est sécurisée" → "Le certificat est valide"
2. Détails : `CN = sda-client-node-1`, signé par `SDA Internal CA`, protocole `TLS 1.3`

→ **Capturer `s5_chrome_cert_details.png`**

### Captures récapitulatives

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 5a | Firefox mode privé → `400 No SSL certificate` | Win11 | `s5_rejected_no_cert_win11.png` |
| 5b | Chrome avec cert → dashboard accessible | Win11 | `s5_accepted_with_cert_win11.png` |
| 5c | Chrome cadenas → "Connexion sécurisée" TLS 1.3 | Win11 | `s5_chrome_padlock.png` |
| 5d | Chrome → détails certificat CN + CA interne | Win11 | `s5_chrome_cert_details.png` |

---

## SCÉNARIO 6 — Ingestion simultanée sur les 3 nœuds

**Message clé :** *"Chaque nœud peut écrire des données en même temps — pas de verrou central, pas de coordination. Syncthing s'occupe de la cohérence."*

### Ce qu'on fait — les 3 nœuds en parallèle

Sur **Win11** → `POST /api/v1/data/ingest` :
```json
{ "tenant_id": "multinode_live", "data": { "node": "win11", "valeur": 100 } }
```

Sur **Ubuntu** (simultanément) → `POST /api/v1/data/ingest` :
```json
{ "tenant_id": "multinode_live", "data": { "node": "ubuntu", "valeur": 200 } }
```

Sur **Kali** (simultanément) → `POST /api/v1/data/ingest` :
```json
{ "tenant_id": "multinode_live", "data": { "node": "kali", "valeur": 300 } }
```

Attendre 30 s → Syncthing propage les 3 fichiers sur chaque nœud.

### Captures récapitulatives

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 6a | Swagger Win11 — ingestion `multinode_live` | Win11 | `s6_ingest_win11.png` |
| 6b | Swagger Ubuntu — ingestion `multinode_live` | Ubuntu | `s6_ingest_ubuntu.png` |
| 6c | Swagger Kali — ingestion `multinode_live` | Kali | `s6_ingest_kali.png` |
| 6d | **Mosaïque 3 Syncthing GUI — contenu identique** | Tous | `s6_mosaic_syncthing_3nodes.png` |

---

## SCÉNARIO 7 — Coffre-fort de fichiers : souveraineté et chiffrement P2P

> **Résultats validés le 28/05/2026 à 16:58**

**Message clé :** *"Chaque nœud possède ses fichiers. Syncthing les réplique, mais ils restent chiffrés avec la clé du propriétaire — seul le propriétaire peut les lire directement. Pour y accéder depuis un autre nœud, il faut demander la clé au propriétaire."*

---

### 7.1 — État observé le 28/05/2026 (résultats validés)

**Win11 (16:58:03)** — nœud propriétaire :

```
Coffre-fort de fichiers
Ma clé de coffre-fort [win11]
  ●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●● [masquée]

Mes fichiers                          2 · 220 B
  🔓 secret_win11.txt    120 B    28/05/2026 15:17    [↓] [🗑]
  🔓 test.txt            100 B    28/05/2026 13:35    [↓] [🗑]
```

→ Win11 voit ses 2 fichiers déverrouillés, téléchargeables directement.

---

**Kali (16:58:22)** — nœud pair :

```
Coffre-fort de fichiers
Ma clé de coffre-fort [kali]
  ●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●● [masquée]

Mes fichiers                          0 · 0 B
  Aucun fichier uploadé depuis ce nœud.

Fichiers des pairs                    2 fichiers
  🔒 secret_win11.txt    [win11]    120 B    [Déverrouiller]
  🔒 test.txt            [inconnu]  100 B    [Déverrouiller]

Ces fichiers sont répliqués par Syncthing mais chiffrés avec la clé
du nœud propriétaire. Demandez la clé via l'interface "Ma clé de
coffre-fort" du pair concerné.
```

---

**Ubuntu (16:58:40)** — nœud pair :

```
Coffre-fort de fichiers
Ma clé de coffre-fort [ubuntu]
  ●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●● [masquée]

Mes fichiers                          0 · 0 B
  Aucun fichier uploadé depuis ce nœud.

Fichiers des pairs                    2 fichiers
  🔒 secret_win11.txt    [win11]    120 B    [Déverrouiller]
  🔒 test.txt            [inconnu]  100 B    [Déverrouiller]
```

---

### 7.2 — Upload d'un PDF depuis Win11

**Ce qu'on fait** sur **Win11** → `https://localhost/` → section "Coffre-fort" :

1. Glisser-déposer (ou cliquer) un fichier PDF (ex: `rapport_sda.pdf`)
2. L'interface affiche **"Chiffrement et upload en cours…"** (spinner)
3. Le fichier apparaît dans **"Mes fichiers"** avec l'icône rouge PDF et le badge `🔓`

Attendre 15–30 s → Syncthing réplique.

Sur **Kali** → `https://localhost/` → section "Coffre-fort" :
- `rapport_sda.pdf` apparaît dans **"Fichiers des pairs"** avec badge `[win11]` et bouton `[Déverrouiller]`
- Le fichier est visible mais **inaccessible sans la clé**

Sur **Ubuntu** → même observation.

**Captures :**

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 7a | Upload PDF en cours — spinner | Win11 | `s7_upload_pdf_win11.png` |
| 7b | PDF dans "Mes fichiers" — `🔓` | Win11 | `s7_pdf_mine_win11.png` |
| 7c | PDF dans "Fichiers des pairs" — `🔒 [win11]` | Kali | `s7_pdf_locked_kali.png` |
| 7d | PDF dans "Fichiers des pairs" — `🔒 [win11]` | Ubuntu | `s7_pdf_locked_ubuntu.png` |

---

### 7.3 — Upload d'une image depuis Kali

**Ce qu'on fait** sur **Kali** → `https://localhost/` → section "Coffre-fort" :

1. Glisser-déposer une image (ex: `photo_kali.png` ou `diagram_sda.jpg`)
2. L'image apparaît dans **"Mes fichiers"** de Kali avec l'icône violette image et badge `🔓`

Attendre 15–30 s → Syncthing réplique.

Sur **Win11** → `https://localhost/` → section "Coffre-fort" :
- L'image apparaît dans **"Fichiers des pairs"** avec badge `[kali]` et bouton `[Déverrouiller]`

Sur **Ubuntu** → même observation.

**Captures :**

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 7e | Image dans "Mes fichiers" — `🔓` | Kali | `s7_image_mine_kali.png` |
| 7f | Image dans "Fichiers des pairs" — `🔒 [kali]` | Win11 | `s7_image_locked_win11.png` |
| 7g | Image dans "Fichiers des pairs" — `🔒 [kali]` | Ubuntu | `s7_image_locked_ubuntu.png` |

---

### 7.4 — Déverrouillage : Kali accède au PDF de Win11

**Ce qu'on fait** — scénario de partage de clé hors-bande :

**Étape 1 — Win11 partage sa clé**

Sur **Win11** → Coffre-fort → panel **"Ma clé de coffre-fort"** :
1. Cliquer sur l'icône **œil** pour révéler la clé Fernet
2. Cliquer **copier** (icône clipboard)
3. Communiquer cette clé à Kali (hors-bande : chat, note, etc.)

→ **Capturer `s7_key_revealed_win11.png`** (clé visible, bouton copier)

**Étape 2 — Kali déverrouille le PDF**

Sur **Kali** → Coffre-fort → ligne `rapport_sda.pdf` → cliquer **"Déverrouiller"** :
1. La modal s'ouvre : *"Ce fichier appartient au nœud win11"*
2. Coller la clé reçue de Win11
3. Cliquer **"Déchiffrer & Télécharger"**
4. Le fichier se télécharge déchiffré

→ **Capturer `s7_unlock_modal_kali.png`** (modal avec clé collée)
→ **Capturer `s7_download_success_kali.png`** (téléchargement déclenché)

**Étape 3 — Vérifier que Win11 peut accéder à l'image de Kali**

Sur **Win11** → Coffre-fort → ligne de l'image Kali → cliquer **"Déverrouiller"** :
1. Coller la clé de Kali (récupérée sur Kali via "Ma clé de coffre-fort")
2. Cliquer **"Déchiffrer & Télécharger"**
3. L'image se télécharge

→ **Capturer `s7_unlock_kali_image_win11.png`**

**Captures récapitulatives — Scénario 7 complet :**

| # | Ce qu'on capture | Sur quel nœud | Nom fichier |
|---|-----------------|---------------|------------|
| 7a | Upload PDF spinner | Win11 | `s7_upload_pdf_win11.png` |
| 7b | PDF dans "Mes fichiers" `🔓` | Win11 | `s7_pdf_mine_win11.png` |
| 7c | PDF verrouillé `🔒 [win11]` | Kali | `s7_pdf_locked_kali.png` |
| 7d | PDF verrouillé `🔒 [win11]` | Ubuntu | `s7_pdf_locked_ubuntu.png` |
| 7e | Image dans "Mes fichiers" `🔓` | Kali | `s7_image_mine_kali.png` |
| 7f | Image verrouillée `🔒 [kali]` | Win11 | `s7_image_locked_win11.png` |
| 7g | Image verrouillée `🔒 [kali]` | Ubuntu | `s7_image_locked_ubuntu.png` |
| 7h | **Ma clé révélée** + bouton copier | Win11 | `s7_key_revealed_win11.png` |
| 7i | **Modal Déverrouiller** — clé collée | Kali | `s7_unlock_modal_kali.png` |
| 7j | Téléchargement déchiffré réussi | Kali | `s7_download_success_kali.png` |
| 7k | Win11 déverrouille image Kali | Win11 | `s7_unlock_kali_image_win11.png` |

---

## Ordre de passage recommandé (25 minutes)

| Ordre | Scénario | Durée | Ce que ça prouve |
|-------|----------|-------|-----------------|
| 1 | **Vue d'ensemble** — 3 `localhost/` simultanément | 2 min | Chaque nœud autonome, aucun serveur central |
| 2 | **Réplication P2P** — Win11 injecte, Ubuntu/Kali reçoivent | 3 min | Sync automatique sans coordination |
| 3 | **Tolérance aux pannes** — Ubuntu suspendu, les 2 autres continuent | 4 min | Résilience P2P |
| 4 | **Mode offline** — Kali isolé, injection locale, rattrapage | 3 min | Offline-first réel |
| 5 | **Sécurité mTLS** — 400 sans cert vs accès avec cert | 2 min | Sécurité enterprise |
| 6 | **Ingestion simultanée** — 3 nœuds injectent en même temps | 3 min | Symétrie P2P sans coordination |
| **7** | **Coffre-fort** — upload PDF/image, chiffrement, partage de clé | **7 min** | **Souveraineté des données par nœud** |
| ★ | **Clôture** — Syncthing GUI mosaïque, même contenu partout | 1 min | Cohérence distribuée |

---

## Checklist pré-démo (30 min avant)

```
Démarrage :
[ ] Win11  : docker compose ps → 4 services healthy
[ ] Ubuntu : docker compose ps → 4 services healthy
[ ] Kali   : docker compose ps → 4 services healthy

Syncthing (sur chaque nœud via http://localhost:8384) :
[ ] Win11  : 2 pairs verts, dossier sda-shared "À jour"
[ ] Ubuntu : 2 pairs verts, dossier sda-shared "À jour"
[ ] Kali   : 2 pairs verts, dossier sda-shared "À jour"

Navigateurs (sur chaque nœud) :
[ ] Win11  : https://localhost/ → dashboard OK (Chrome avec cert)
[ ] Ubuntu : https://localhost/ → dashboard OK (Chrome avec cert)
[ ] Kali   : https://localhost/ → dashboard OK (Chrome avec cert)

Coffre-fort (sur chaque nœud) :
[ ] Win11  : "Ma clé de coffre-fort" visible avec node_name=win11
[ ] Ubuntu : "Ma clé de coffre-fort" visible avec node_name=ubuntu
[ ] Kali   : "Ma clé de coffre-fort" visible avec node_name=kali

Fichiers de démo préparés :
[ ] Un PDF prêt à uploader sur Win11 (ex: rapport_sda.pdf)
[ ] Une image prête sur Kali (ex: diagram_sda.png ou photo.jpg)
[ ] Un CSV ou JSON prêt sur Ubuntu (optionnel)
```

---

*Guide démonstration graphique — SDA v1.0 — Architecture P2P symétrique locale — EIGSI × AL BARAA CONSULTING — 2026*
