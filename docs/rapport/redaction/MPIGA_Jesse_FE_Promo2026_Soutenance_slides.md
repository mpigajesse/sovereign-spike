# Support de Soutenance — Slides
<!-- NOM FICHIER MOODLE : MPIGA_Jesse_FE_Promo2026_Soutenance.ppt -->
<!-- DEADLINE DEPOT MOODLE : 30/06/2026 (veille de soutenance) -->
<!-- DUREE CIBLE : 30 minutes (27–33 min tolérés) -->
<!-- FORMAT : PowerPoint .ppt — max 8 Mo -->

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 1 — PAGE DE GARDE -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 1 — Page de Garde

**[Design : fond sombre #0D0A07, motif zellige subtil en transparence, accent doré #C79A1B]**

---

**[LOGO EIGSI — haut gauche]** &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; **[LOGO AL BARAA CONSULTING — haut droite]**

---

## 🔐 SDA — Sovereign Data Agent

### Conception et Implémentation d'une Architecture  
### Coffre-Fort Data P2P Souveraine

---

**Expérience Professionnelle de Fin d'Études**  
EIGSI Casablanca — Spécialité Big Data & Intelligence Artificielle  
**Promotion 2026**

---

| | |
|---|---|
| **Étudiant** | Jesse MPIGA-ODOUMBA |
| **Encadrante entreprise** | Mme Soumia CHOKRI — AL BARAA CONSULTING |
| **Tuteur EIGSI** | M. Ayoub AMRANI |
| **Soutenance** | 01 juillet 2026 — EIGSI Casablanca |

---

*Notes présentateur :*
> Sourire, regarder le jury. Pause 3 secondes. Commencer par : "Mesdames, Messieurs, je vous remercie de m'accorder ce temps pour vous présenter mon projet de fin d'études, réalisé au sein d'AL BARAA CONSULTING..."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 2 — PLAN DE LA PRÉSENTATION -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 2 — Plan de la Présentation

**[Design : fond sombre, titre en doré, bullets avec icônes]**

---

## Plan

| # | Partie | ⏱ |
|---|--------|---|
| 1 | AL BARAA CONSULTING & contexte du stage | 3 min |
| 2 | Problématique & enjeux | 3 min |
| 3 | État de l'art & positionnement | 2 min |
| 4 | Architecture SDA | 4 min |
| 5 | **Démonstration live** — cluster 3 nœuds | 5 min |
| 6 | Résultats & validation — 32/33 PASS | 4 min |
| 7 | Sécurité & conformité AUDPF | 3 min |
| 8 | Difficultés & réflexion ingénieur | 4 min |
| 9 | Bilan & perspectives | 3 min |

---

*Notes présentateur :*
> "Je vais structurer cette présentation en 9 parties. Je commencerai par présenter l'entreprise et le contexte, avant d'entrer dans le cœur technique du projet. La démonstration live au milieu vous permettra de voir le système fonctionner en temps réel sur 3 machines."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 3 — AL BARAA CONSULTING -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 3 — AL BARAA CONSULTING

**[Design : fond sombre, logo AL BARAA prominent, données clés en cartes]**

---

**[LOGO AL BARAA CONSULTING — centré, grand]**

**[PHOTO : façade ou bureau — si disponible]**

---

## Un cabinet de conseil en ingénierie numérique

| 📅 Fondé | Mars 2017 |
|---------|-----------|
| 📍 Siège | Ain Sebaa, Casablanca |
| ⚖️ Statut | SARL AU — 100 000 MAD |
| 👤 DG | Mme Soumia CHOKRI |
| 🎯 Missions | Développement logiciel, Architecture SI, Transformation numérique |
| 🤝 Clients | Secteur public & privé (B2B) |

---

### Ma position dans l'entreprise

> Développeur & Architecte principal du projet SDA — **responsabilité totale** sur l'ensemble du cycle (conception → déploiement → validation)

---

*Notes présentateur :*
> "AL BARAA CONSULTING est un cabinet à taille humaine, ce qui m'a placé en situation de forte responsabilité dès le premier jour. Pas de tâches d'exécution — une mission d'ingénieur débutant avec une autonomie réelle."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 4 — PROBLÉMATIQUE -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 4 — La Problématique

**[Design : fond sombre, carte de l'Afrique en arrière-plan transparent, couleurs chaudes]**

---

## Le constat : une dépendance critique

```
┌─────────────────────────────────────────────────────┐
│                                                     │
│   Organisations africaines                         │
│         ↓  données                                 │
│   ☁️  AWS / Azure / Google Cloud                  │
│         ↓  stockage à l'étranger                  │
│   ❓ Localisation inconnue                         │
│   ❓ Conditions d'accès imposées                   │
│   ❓ Continuité de service non garantie            │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

### L'AUDPF — Union Africaine, Déc. 2025

> *"Les données des organisations africaines doivent rester sous contrôle local et ne pas transiter sans consentement par des serveurs étrangers."*

---

### ❓ La question centrale

> **Comment garantir la souveraineté, la sécurité et la résilience des données africaines — sans cloud centralisé étranger ?**

---

*Notes présentateur :*
> "L'Union Africaine a validé l'AUDPF en décembre 2025 — un cadre réglementaire qui crée à la fois un impératif et une opportunité : développer des alternatives souveraines. C'est exactement ce que AL BARAA m'a demandé de concevoir."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 5 — LA SOLUTION : BRIQUE UNIVERSELLE -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 5 — La Solution Proposée

**[Design : schéma P2P symétrique, 3 nœuds interconnectés, fond sombre, accents verts]**

---

## SDA — Sovereign Data Agent

### Une "Brique Universelle" décentralisée

```
    🖥️ NŒUD 1          🖥️ NŒUD 2          🖥️ NŒUD 3
  [Win11 : 192.168.200.1]  [Ubuntu : .130]  [Kali : .128]
         │                      │                 │
         ├──────────────────────┤─────────────────┤
         │         Syncthing P2P (BEP/TLS 1.3)    │
         └──────────────────────┴─────────────────┘
              shared_storage/ synchronisé automatiquement
```

---

### 3 piliers fondateurs

| 🏛️ Autonomie | Chaque terminal = nœud de stockage intelligent |
|-------------|----------------------------------------------|
| ⚙️ Orchestration | Briques open-source éprouvées — 0 from scratch |
| 🔐 Sécurité native | TLS 1.3 + mTLS + Fernet + SHA-256 |

---

### Ce que SDA n'est PAS

❌ Un cloud privé centralisé  
❌ Une solution nécessitant un serveur maître  
❌ Un outil à licences propriétaires

---

*Notes présentateur :*
> "L'idée fondamentale est simple mais puissante : plutôt que d'envoyer les données vers le cloud, on distribue le code sur chaque terminal. Les données ne bougent jamais — elles se répliquent entre les terminaux de l'organisation."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 6 — ÉTAT DE L'ART -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 6 — Positionnement vs Alternatives

**[Design : tableau comparatif, colonnes colorées, SDA en surbrillance]**

---

## Pourquoi pas les solutions existantes ?

| Critère | IPFS/Filecoin | Solid (Pods) | **✅ SDA** |
|---------|:---:|:---:|:---:|
| Stockage analytique OLAP | ❌ | ❌ | ✅ DuckDB |
| Fonctionnement offline total | ⚠️ | ❌ | ✅ |
| Résolution conflits (CRDT) | ❌ | Manuel | ✅ Automatique |
| Souveraineté infrastructurelle | ⚠️ Pinning | ⚠️ Hébergeur | ✅ Totale |
| Conformité AUDPF | ⚠️ | ❌ | ✅ |
| 100% open-source | ✅ | ✅ | ✅ |

---

### Les 3 différenciateurs SDA

> **1.** OLAP natif embarqué — analyses complexes sans serveur central  
> **2.** Offline-first radical — 100% fonctionnel sans internet  
> **3.** Souveraineté totale — zéro dépendance à un tiers externe

---

*Notes présentateur :*
> "IPFS est excellent pour le stockage de fichiers, mais ne fait pas d'analyse. Solid nécessite une connexion. Aucune solution existante ne combine ces trois piliers pour le contexte africain. SDA comble ce vide."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 7 — ARCHITECTURE TECHNIQUE -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 7 — Architecture Technique

**[Design : schéma d'architecture, 4 blocs colorés, flèches, fond sombre]**

---

## 4 services Docker orchestrés

```
┌─────────────────────────────────────────────────────────┐
│                    NŒUD SDA                             │
│                                                         │
│  🌐 sda-nginx        📊 sda-frontend                   │
│  TLS 1.3 + mTLS  ──▶  React Dashboard                  │
│  Port 443           Port 3000 (interne)                 │
│       │                                                 │
│       ▼                                                 │
│  ⚡ sda-backend      🔄 sda-syncthing                  │
│  FastAPI + Python     Réplication P2P                   │
│  DuckDB + SQLite      Port 22000 (P2P)                  │
│  Port 8000 (int.)     Port 8384 (GUI)                   │
│                                                         │
│  💾 Volumes : data/db/  +  data/shared_storage/        │
└─────────────────────────────────────────────────────────┘
```

---

## Paradigme Code-to-Data

| Cloud classique | **SDA** |
|-----------------|---------|
| Données → serveur distant | **Données restent locales** |
| Code s'exécute en cloud | **Code distribué sur chaque nœud** |
| Dépendance réseau | **Offline-first** |

---

*Notes présentateur :*
> "Le paradigme Code-to-Data est l'inversion du cloud : au lieu d'envoyer vos données vers un serveur, vous distribuez le code — l'image Docker — sur vos propres terminaux. Les données ne quittent jamais votre périmètre."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 8 — FLUX DE DONNÉES -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 8 — Flux de Données : De l'Ingestion à la Réplication

**[Design : flèche horizontale avec 5 étapes, icônes, couleurs par étape]**

---

## Pipeline complet

```
  Application   →   API REST   →   DuckDB   →   Parquet   →   Syncthing P2P
  (n'importe       FastAPI        OLAP         chiffré        réplication
  quelle app)      /ingest       analytics    Fernet          automatique
                                              AES-128         3+ nœuds
```

---

## Audit Trail SHA-256 Chaîné

```
Ingestion 1 :  record_hash = SHA256(data₁ + "genesis")
Ingestion 2 :  record_hash = SHA256(data₂ + hash₁)
Ingestion 3 :  record_hash = SHA256(data₃ + hash₂)
                              ↑
                    Toute falsification rompt la chaîne
```

> Traçabilité non falsifiable — conforme AUDPF

---

## CRDT — Résolution automatique des conflits

- **Scénario :** Nœud 1 et Nœud 3 modifient simultanément hors-ligne
- **Détection :** Syncthing crée `.sync-conflict-*`
- **Résolution :** `POST /api/v1/sync/reconcile` → Last-Write-Wins
- **Résultat :** `{"conflicts_resolved": 1}` — conflit éliminé automatiquement

---

*Notes présentateur :*
> "Le CRDT Last-Write-Wins est une solution éprouvée en théorie des systèmes distribués — je l'ai implémentée pour le cas d'usage spécifique des fichiers Parquet Syncthing. Un technicien aurait laissé l'utilisateur résoudre le conflit manuellement. L'approche ingénieur est de l'automatiser."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 9 — DÉMONSTRATION LIVE -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 9 — Démonstration Live

**[Design : fond vert foncé ou amber, "LIVE" badge en rouge, capture dashboard en arrière-plan]**

---

## 🔴 LIVE — Cluster 3 Nœuds

**[PHOTO/CAPTURE : Vue d'ensemble du cluster — 3 fenêtres terminales côte à côte]**

---

### Scénario de démonstration (5 min)

| Étape | Action | Résultat attendu |
|-------|--------|-----------------|
| **1** | `docker compose ps` sur Win11 | 4 conteneurs ✅ healthy |
| **2** | Navigateur Win11 : `https://localhost/` | Dashboard SDA — métriques Syncthing |
| **3** | `POST /api/v1/data/ingest` depuis Win11 | `{"status":"success", "record_hash":"..."}` |
| **4** | Terminal Ubuntu : `ls data/shared_storage/` | Fichier Parquet apparu en ~15s |
| **5** | Requête DuckDB depuis Ubuntu | Données de Win11 visibles localement |

---

**[CAPTURE : Dashboard page Vue d'ensemble]**

**[CAPTURE : Résultat ingest — terminal Win11]**

**[CAPTURE : shared_storage Ubuntu — fichier synchronisé]**

---

*Notes présentateur :*
> "Ce que vous voyez n'est pas une simulation. Ce sont 3 machines physiques — un PC Windows 11 et 2 VMs Linux — qui se synchronisent en temps réel via notre réseau VMnet1. Je vais exécuter les commandes en direct."
>
> *(Exécuter la démo — ne pas se précipiter — commenter ce qui se passe à voix haute)*
>
> "Vous voyez que le fichier Parquet créé sur Win11 apparaît automatiquement sur Ubuntu en moins de 15 secondes, sans aucune intervention manuelle. C'est la promesse de SDA — la réplication est transparente."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 10 — RÉSULTATS : 32/33 PASS -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 10 — Résultats : 32/33 Tests Réussis

**[Design : tableau de résultats, vert dominant, 1 case amber pour le SKIP]**

---

## Suite automatisée — 11 tests × 3 nœuds

**[CAPTURE : exécution `bash scripts/demo-tests.sh` — extrait résultats]**

---

| Test | Catégorie | Win11 | Ubuntu | Kali |
|------|-----------|:-----:|:------:|:----:|
| 1.1 | Health Check (Docker exec) | ✅ | ✅ | ✅ |
| 1.2 | HTTPS/mTLS depuis hôte | ⏭¹ | ✅ | ✅ |
| 1.3 | Rejet sans certificat (HTTP 400) | ✅ | ✅ | ✅ |
| 2 | Ingestion → Parquet chiffré | ✅ | ✅ | ✅ |
| 3.1 | DuckDB + déchiffrement Fernet | ✅ | ✅ | ✅ |
| 3.2 | Agrégation multi-nœuds | ✅ | ✅ | ✅ |
| 4.1 | Fichiers shared_storage | ✅ | ✅ | ✅ |
| 4.2 | Intégrité (0 corrompu) | ✅ | ✅ | ✅ |
| 5 | CRDT réconciliation | ✅ | ✅ | ✅ |
| 6.1 | 4 conteneurs healthy | ✅ | ✅ | ✅ |
| 6.2 | Stabilité uptime | ✅ | ✅ | ✅ |
| **Total** | | **10/11** | **11/11** | **11/11** |

---

> ¹ *SKIP Win11 : limitation technique `curl` schannel Windows — validé manuellement Chrome*

### Résultat global : **32/33 PASS — 0 FAIL**

---

*Notes présentateur :*
> "Ce tableau représente 33 tests exécutés sur 3 systèmes d'exploitation différents. Le seul skip est une limitation du client curl sous Windows, pas une limitation de SDA — le test a été validé manuellement via Chrome. Aucun échec."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 11 — CRITÈRES POC ATTEINTS -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 11 — Tous les Critères du POC Atteints

**[Design : tableau 2 colonnes — Cible vs Résultat, tous en vert]**

---

## Indicateurs de succès — Plan Directeur

| Critère | Cible | ✅ Résultat |
|---------|-------|-------------|
| Réplication P2P | 3+ nœuds, 0 perte | 3 nœuds — 0 perte confirmée |
| Conflits CRDT | 0 non résolu | 0 conflit actif |
| Architecture local-first | `offline_ready: true` | Confirmé sur 3 nœuds |
| Déploiement Docker | < 30 minutes | **~15 minutes** |
| Sécurité mTLS | Rejet sans cert | HTTP 400 — 3 nœuds |
| Chiffrement at-rest | Fernet AES-128 | Actif — fichiers nouveaux |
| Audit trail SHA-256 | Hash chaîné | `record_hash` unique/enreg. |
| Stabilité | Conteneurs healthy | **Win11 : 45h uptime continu** |
| DuckDB analytique | < 1s sur données réelles | 609 enreg., 16 tenants — < 1s |

---

**[CAPTURE : `docker compose ps` — 4 conteneurs healthy — uptime 45h]**

---

*Notes présentateur :*
> "45 heures d'uptime continu sur Win11 — ce n'est pas un test de 5 minutes. Le système a fonctionné sans intervention pendant presque 2 jours. C'est la preuve de stabilité que demandait le cahier des charges."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 12 — SÉCURITÉ -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 12 — Sécurité : Défense en Profondeur

**[Design : schéma en oignon — couches de sécurité emboîtées, rouge extérieur → vert centre]**

---

## Architecture de sécurité multicouche

```
┌─────────────────────────────────────────────────┐
│  🌐 TRANSIT (externe → nginx)                   │
│  TLS 1.3 EXCLUSIF + mTLS x509                   │
│  ↳ TLS 1.2 rejeté ✅  ↳ HTTP 400 sans cert ✅  │
├─────────────────────────────────────────────────┤
│  🔄 TRANSIT INTER-NŒUDS (Syncthing)             │
│  BEP + TLS 1.3 + authentification par Device ID │
├─────────────────────────────────────────────────┤
│  💾 AT-REST (fichiers Parquet)                  │
│  Fernet AES-128-CBC + HMAC-SHA256               │
├─────────────────────────────────────────────────┤
│  🗄️ AT-REST (base SQLite)                       │
│  SQLCipher AES-256                              │
├─────────────────────────────────────────────────┤
│  📋 TRAÇABILITÉ                                 │
│  Audit trail SHA-256 chaîné — non falsifiable   │
└─────────────────────────────────────────────────┘
```

---

**[CAPTURE : Chrome — cadenas HTTPS, "Connexion sécurisée", TLS 1.3, certificat client]**

---

*Notes présentateur :*
> "La sécurité n'est pas une option ajoutée après coup — elle est architecturale. Chaque couche protège la suivante. Même si l'attaquant compromet la couche réseau, les données Parquet restent chiffrées. Même si les Parquet sont volés, la DB SQLite reste protégée."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 13 — CONFORMITÉ AUDPF -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 13 — Conformité AUDPF : Souveraineté par Design

**[Design : carte Afrique, drapeau AU, fond sombre, texte doré]**

---

## AU Data Policy Framework — Les exigences

| Exigence AUDPF | Implémentation SDA |
|----------------|-------------------|
| Données restent locales | ✅ Parquet stocké uniquement sur les nœuds de l'organisation |
| Pas de transfert sans consentement | ✅ Syncthing = réseau privé inter-nœuds autorisés uniquement |
| Traçabilité des accès | ✅ Audit trail SHA-256 — chaque accès enregistré |
| Chiffrement obligatoire | ✅ Fernet at-rest + TLS 1.3 en transit |
| Contrôle des accès | ✅ mTLS x509 — seuls les porteurs du certificat CA interne accèdent |

---

## SDA est conforme par design, pas par configuration

> Il est **impossible** pour un utilisateur de contourner la souveraineté — le système rejette structurellement toute connexion non authentifiée et ne contacte aucun service externe.

---

### Impact économique estimatif

| Métrique | Valeur |
|---------|--------|
| Économie par organisation (vs cloud étranger) | 500 MAD/mois |
| Pour 10 organisations | 60 000 MAD/an |
| ROI première année | **184%** |

---

*Notes présentateur :*
> "La conformité AUDPF n'est pas un label qu'on colle sur un produit existant. SDA est construit de fond en comble pour la respecter. Aucun packet de données ne peut sortir du périmètre de confiance défini par l'organisation."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 14 — DIFFICULTÉS : POSTURE INGÉNIEUR -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 14 — Difficultés Surmontées : La Valeur Ingénieur

**[Design : 3 colonnes — Problème / Approche Technicien / Approche Ingénieur]**

---

## 5 incidents techniques — 5 solutions permanentes

---

### Incident 1 — nginx crash (envsubst détruit les variables)

| | |
|---|---|
| 🔴 **Symptôme** | Conteneur nginx redémarre en boucle |
| ❓ **Cause racine** | `envsubst` Alpine substitue `$host`, `$remote_addr`... → config invalide |
| 🔧 **Technicien** | Désactiver les variables → perte de fonctionnalité |
| ✅ **Ingénieur** | Bypass structurel : montage direct `/conf.d/` au lieu de `/templates/` |
| 📐 **Principe** | Comprendre le comportement interne de l'image Docker |

---

### Incident 2 — Clé API Syncthing perdue après git pull

| | |
|---|---|
| 🔴 **Symptôme** | Dashboard : "Impossible de joindre Syncthing" sur les VMs |
| ❓ **Cause racine** | Clé node-specific versionnée dans git → écrasée par git pull |
| 🔧 **Technicien** | Relancer le script manuellement à chaque pull |
| ✅ **Ingénieur** | Auto-injection via `nginx-entrypoint.sh` + `depends_on: service_healthy` |
| 📐 **Principe** | Rendre le système auto-configurant — éliminer la procédure manuelle |

---

### Incident 3 — Animations SVG tremblent (vibration en boucle)

| | |
|---|---|
| 🔴 **Symptôme** | Flèches SVG animées redémarrent toutes les 1,8s |
| ❓ **Cause racine** | `useState` → re-render React → recréation DOM → restart `<animateMotion>` |
| 🔧 **Technicien** | Désactiver les animations |
| ✅ **Ingénieur** | `useRef` + mise à jour DOM directe — sans re-render |
| 📐 **Principe** | Maîtriser le modèle de rendu React pour les cas hybrides SVG + données |

---

*Notes présentateur :*
> "Ces trois incidents illustrent la différence entre corriger un symptôme et résoudre un problème. Dans chaque cas, la solution ingénieur est plus difficile à concevoir mais elle élimine définitivement le problème — elle n'oblige pas l'utilisateur à s'en souvenir."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 15 — COMPÉTENCES DÉVELOPPÉES -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 15 — Compétences Développées

**[Design : hexagones de compétences, disposés en nid d'abeilles, couleurs par domaine]**

---

## Ce que ce stage m'a appris

---

### Techniques

| Domaine | Compétences acquises |
|---------|---------------------|
| 🔐 Cybersécurité | PKI interne, mTLS x509, TLS 1.3, Fernet, SQLCipher |
| 📊 Big Data | DuckDB OLAP, Parquet colonnaire, agrégation distribuée |
| 🔄 Systèmes distribués | CRDT, cohérence éventuelle, BEP, mDNS |
| 🐳 DevOps | Docker Compose multi-services, healthchecks, orchestration |
| ⚡ Backend | FastAPI, Python 3.11, API REST, audit trail chaîné |
| 🖥️ Frontend | React 18, TypeScript 5, animations SVG, Tailwind |

---

### Transversales

> 🎯 **Diagnostic en production** — 5 incidents résolus structurellement  
> 📝 **Rigueur documentaire** — journal technique, 15+ fichiers de doc  
> 🧭 **Autonomie** — décisions techniques sans supervision permanente  
> 💬 **Communication** — points hebdomadaires avec Mme CHOKRI + M. Amrani

---

*Notes présentateur :*
> "Ce qui m'a le plus appris, ce n'est pas la liste des technologies — c'est la capacité à diagnostiquer un système distribué en production, où les erreurs ne sont pas toujours visibles et où chaque couche cache la suivante."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 16 — BILAN & PERSPECTIVES -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 16 — Bilan et Perspectives

**[Design : deux colonnes — Acquis (vert) / Perspectives (bleu/or)]**

---

## Ce qui a été accompli

| ✅ | Résultat |
|----|---------|
| 32/33 PASS | Cluster 3 nœuds validé |
| 9/9 critères | Tous les objectifs POC atteints |
| ~15 min | Déploiement complet — cible 30 min |
| 45h | Uptime continu sans intervention |
| 100% | Open-source — 0 MAD de licences |
| 15+ | Fichiers de documentation technique |

---

## Valeur pour AL BARAA CONSULTING

> ✅ Prototype démontrable immédiatement  
> ✅ Base réutilisable pour des clients publics (conformité AUDPF)  
> ✅ ROI estimatif 184% dès la 1ère année  
> ✅ Positionnement sur le marché des solutions souveraines africaines

---

## Perspectives techniques

| Horizon court | Horizon moyen |
|---------------|---------------|
| Tests charge 10+ nœuds | API gRPC haute performance |
| Scans sécurité Trivy/Bandit | Interface mobile React Native |
| CI/CD GitHub Actions | Syncthing relay WAN inter-villes |

---

*Notes présentateur :*
> "Ce prototype est un point de départ, pas un produit fini. Mais il démontre la faisabilité technique et économique d'une alternative souveraine. AL BARAA dispose maintenant d'un actif démontrable pour répondre aux appels d'offres liés à l'AUDPF."

---
<!-- ═══════════════════════════════════════════════════════════ -->
<!-- SLIDE 17 — CONCLUSION -->
<!-- ═══════════════════════════════════════════════════════════ -->

# SLIDE 17 — Conclusion

**[Design : fond sombre, accent doré, texte centré, sobre et fort]**

---

## SDA — Ce que nous avons démontré

---

### Techniquement

> Une architecture Coffre-Fort Data P2P Souveraine est **faisable, performante et déployable** par orchestration de briques open-source éprouvées.

---

### Stratégiquement

> **32/33 PASS** sur 3 OS hétérogènes  
> **~15 min** de déploiement — n'importe quel terminal, n'importe quel OS  
> **100% open-source** — 0 dépendance à une infrastructure étrangère  
> **Conforme AUDPF** — par design, pas par configuration

---

### Personnellement

> Ce projet m'a confirmé que la valeur d'un ingénieur ne réside pas dans la quantité de code écrit, mais dans la **qualité des décisions architecturales** et la **rigueur de la documentation**.

---

## La souveraineté numérique africaine commence ici.

---

**[LOGO EIGSI + LOGO AL BARAA CONSULTING]**

*Jesse MPIGA-ODOUMBA — EIGSI Casablanca — Promotion 2026*

---

*Notes présentateur :*
> Pause. Regarder le jury.
>
> "Je vous remercie pour votre attention. Je suis à votre disposition pour répondre à vos questions."
>
> *(Ne pas dépasser 33 minutes — chronomètre personnel conseillé)*

---

<!-- ═══════════════════════════════════════════════════════════ -->
<!-- ANNEXE — SLIDES DE RÉSERVE (si questions jury) -->
<!-- ═══════════════════════════════════════════════════════════ -->

---
---

# SLIDES DE RÉSERVE — Pour les questions jury

*(Ne pas présenter sauf si le jury pose ces questions)*

---

## RÉSERVE A — Architecture détaillée API

**Endpoints REST :**

| Méthode | Endpoint | Description |
|---------|----------|-------------|
| GET | `/health` | Status, offline_ready, architecture |
| GET | `/api/v1/node/info` | ID Syncthing, OS, hostname, uptime |
| POST | `/api/v1/data/ingest` | Ingestion → DuckDB → Parquet Fernet |
| POST | `/api/v1/sync/reconcile` | CRDT LWW — résolution conflits |

**[CAPTURE : Swagger UI — https://localhost/docs]**

---

## RÉSERVE B — Détail chiffrement Fernet

```
Données brutes
     ↓
DuckDB export Parquet
     ↓
Fernet.encrypt(parquet_bytes, key=PARQUET_FERNET_KEY)
     ↓                    ↑
AES-128-CBC              clé partagée entre nœuds via .env
HMAC-SHA256              (jamais versionnée dans git)
     ↓
Fichier .parquet.enc → Syncthing P2P
```

---

## RÉSERVE C — NAT Docker sur Win11 (comportement Syncthing)

**Symptôme visible :** Syncthing GUI Win11 affiche `172.21.0.1` pour les pairs au lieu de `192.168.200.x`

**Cause :** NAT masquerade Docker/WSL2 — comportement normal Windows

**Impact :** Aucun — synchronisation 100% fonctionnelle

---

## RÉSERVE D — Scalabilité

**Question :** Comment SDA passe-t-il à 10, 50, 100 nœuds ?

**Réponse :**
- Syncthing est testé en production jusqu'à 1000+ nœuds
- Ajouter un nœud = `git clone` + `docker compose up --build -d` + couplage Syncthing
- Aucune reconfiguration du cluster existant nécessaire
- Le seul goulot d'étranglement est la bande passante réseau — non le logiciel

---

## RÉSERVE E — Sécurité : que se passe-t-il si un nœud est volé ?

| Donnée | Protection |
|--------|-----------|
| Fichiers Parquet | Chiffrés Fernet — illisibles sans la clé |
| Base SQLite | Chiffrée SQLCipher AES-256 |
| Clé Fernet | Dans `.env` — à sécuriser par l'OS (BitLocker/LUKS) |
| Certificat client | Dans le browser — révocable par la CA interne |

> **Action immédiate si nœud compromis :** révoquer le certificat client → accès coupé

---

*Fin du support de soutenance*

---

*MPIGA-ODOUMBA Jesse — EIGSI Casablanca — Promotion 2026*
*AL BARAA CONSULTING — Soutenance : 01/07/2026 à 10h00*
