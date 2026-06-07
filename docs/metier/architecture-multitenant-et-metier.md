# Architecture multi-tenant & couche métier — Cadrage validé

**Sovereign Data Agent** — EIGSI × AL BARAA — Jesse MPIGA-ODOUMBA
Date : 2026-06-06 — Version socle : 0.1.10

> Ce document fige le modèle multi-tenant et le découpage de la couche métier, tels que
> validés en discussion. Il sert de référence avant l'implémentation. Il complète
> `cadrage-couche-metier.md` (transition socle → métier) et `couches-responsabilites.md`
> (décision §5bis sur l'atomicité).

---

## 1. Vocabulaire — lever toute confusion

Le mot « client » a **deux sens** dans le projet ; on les distingue strictement :

| Terme | Définition |
|-------|------------|
| **Tenant = client = PME** | L'entreprise qui utilise la solution. Une PME = **un tenant**. |
| **Machine cliente** (sens client/serveur) | Un nœud **passif** qui lit depuis le nœud **actif** (serveur). C'est un **rôle technique**, à l'intérieur d'un tenant. |

**Règle à retenir :** un tenant n'est PAS une machine. Un tenant est **une entreprise**,
matérialisée par **un cluster de plusieurs machines** aux rôles différents.

---

## 2. Le modèle : 1 PME = 1 tenant = 1 cluster

```
          ┌──────────── UN SEUL TENANT = UNE PME ────────────┐
          │              tenant_id = abc123                   │
          │              DEK = (unique à cette PME)           │
          │                                                   │
          │   PC physique ──→ rôle : NŒUD ACTIF (serveur)     │
          │   VM1         ──→ rôle : NŒUD PASSIF (client)     │
          │   VM2         ──→ rôle : RELAIS (aveugle)         │
          │                                                   │
          │   → MÊME tenant_id sur les 3 machines             │
          │   → MÊME DEK                                       │
          │   → MÊME jeu de données métier (répliqué)         │
          └───────────────────────────────────────────────────┘
```

**Conséquences :**
- Les 3 machines (PC, VM1, VM2) forment **le cluster d'UNE seule PME**, pas 3 PME.
- Elles partagent **le même tenant_id, la même DEK, les mêmes données métier**.
- Une vente enregistrée sur l'actif (PC) est répliquée et lisible sur le passif (VM1).
- Le relais (VM2) ne détient **aucune donnée métier en clair** — uniquement des blobs
  chiffrés opaques, étiquetés par tenant_id.

État actuel du déploiement : moteur installé sur les 3 machines, rôles définis, version 0.1.10.

---

## 3. Le tenant_id : identité de la PME

Le **tenant_id** est un identifiant unique et opaque de l'entreprise. Il est :

- **Généré localement** à la création du compte (cf. §4), comme la DEK.
- **Partagé par toutes les machines** du cluster (fixé à l'enrôlement).
- Utilisé à trois niveaux :

| Niveau | Rôle du tenant_id |
|--------|-------------------|
| Cluster de la PME | identité commune des données (« appartiennent à abc123 ») |
| Relais éditeur | **séparer les blobs** de plusieurs PME, en restant aveugle |
| Base métier | **estampiller** chaque donnée → design honnête, prêt multi-PME |

```
RELAIS éditeur (multi-tenant, AVEUGLE)
├── tenant-abc123 → [blobs chiffrés opaques]
├── tenant-xyz789 → [blobs chiffrés opaques]
└── tenant-def456 → [blobs chiffrés opaques]
       → ne voit jamais ni l'identité réelle ni le clair d'aucun tenant
```

---

## 4. Multi-tenant en self-service : chaque client crée son compte

Le framework est **multi-tenant en self-service** : on n'attend pas qu'un administrateur
configure une PME. **Chaque client crée lui-même son compte.** C'est l'esprit du cadrage
§6 (« zéro administrateur », *appartenir au cluster = posséder la clé = avoir été enrôlé*).

### 4.1 Créer un compte (bootstrap d'un nouveau tenant)

```
Nouveau client installe l'app
        │
        ▼
« Créer mon compte »  ──→  saisit le nom de l'entreprise
        │
        ▼
Le framework génère LOCALEMENT :
   • tenant_id unique          (identité de la PME)
   • DEK                        (clé de chiffrement de la PME)
   • code de récupération       (Argon2id — déjà livré en 0.1.10)
        │
        ▼
Cette 1ère machine devient le NŒUD ACTIF de son cluster
```

### 4.2 Rejoindre un compte (autres machines de la même PME)

Les machines suivantes **rejoignent** le tenant via l'**enrôlement** déjà livré en 0.1.10
(échange de clé publique X25519 → sealed box → réception de la DEK). Elles héritent du
**même tenant_id** et de la **même DEK**.

> **Acquis 0.1.10 :** le mécanisme « rejoindre un compte » existe déjà (gestion du parc).
> Il reste à construire « **créer** un compte » (générer tenant_id + DEK + nom, devenir actif).

### 4.3 Contrainte de souveraineté (non négociable)

Le compte est **auto-souverain** : il se crée **localement**, sur la machine du client.
Il n'existe **AUCUN serveur central de l'éditeur** stockant la liste des comptes — sinon
l'éditeur connaîtrait tous ses clients, ce qui casserait la souveraineté.

| | SaaS classique | Ce framework |
|---|---|---|
| Où se crée le compte | serveur de l'éditeur | **machine du client** |
| Qui connaît l'identité du tenant | l'éditeur | **personne d'autre que le client** |
| Ce que voit le relais | tout | **tenant_id opaque + blobs chiffrés** |

Sécurité d'accès = cryptographique (DEK + code de récupération Argon2id), **pas** un
login/mot de passe vers un serveur central.

---

## 5. Découpage des bases : moteur vs métier (Option A retenue)

PostgreSQL fait aujourd'hui deux métiers à la fois (moteur + stockage métier). On les
sépare proprement, **sans casser l'atomicité** exigée par la décision §5bis.

### Option retenue : **deux schémas dans une même base PostgreSQL**

```
PostgreSQL (1 instance par machine, répliquée par WAL)
├── schema engine    ← le MOTEUR (cœur système)
│    ├── operations_journal     (journal chiffré append-only)
│    ├── journal_seq            (séquence atomique)
│    ├── node_epoch             (fencing anti-split-brain)
│    ├── enrolled_devices       (parc — 0.1.10)
│    ├── dek_state              (générations de DEK — 0.1.10)
│    └── recovery_blob          (code de récupération — 0.1.10)
│
└── schema business ← le MÉTIER (données de la PME)
     ├── produits
     ├── clients
     └── ventes / stock
```

### Pourquoi deux schémas (et pas deux bases séparées)

- **Séparation logique nette** des responsabilités (l'objectif voulu).
- **Atomicité préservée** : une opération à invariant fort (ex. vente anti-survente) peut
  vérifier le stock (`business`) ET écrire le journal (`engine`) dans **UNE seule
  transaction** → pas de 2PC, pas de risque de survente.
- **Réplication WAL inchangée** : PostgreSQL réplique toute l'instance d'un coup → le
  standby reçoit moteur + métier ensemble, automatiquement.

> Alternative écartée (deux bases physiquement séparées) : plus « pure » en event sourcing,
> mais impose de repenser les opérations à invariant fort et casserait le socle prouvé.
> Hors-périmètre pour le spike.

---

## 6. La couche métier : CRUD simple + une règle forte

Le métier reste **volontairement simple** — ce qu'on veut prouver, c'est **le moteur**.
On garde toutefois **au moins une opération à invariant fort**, sinon le moteur ne
démontre pas sa valeur.

| Objet | Type d'opérations | Ce que ça démontre |
|-------|-------------------|--------------------|
| **Produits** | CRUD pur (créer / modifier / supprimer) | le moteur marche pour des données simples, sans règle |
| **Clients** | CRUD pur | le moteur est **générique** (agnostique au domaine) |
| **Ventes / Stock** | opération à **invariant fort** (déjà prouvée) | **anti-survente** → la vraie valeur du moteur souverain |

Toutes les données portent le **tenant_id**.

---

## 7. Ce qui existe déjà vs ce qui reste à faire

| Brique | État |
|--------|------|
| Journal chiffré, séquençage, fencing, réplication | ✅ socle prouvé |
| Anti-survente (vente / stock) | ✅ prouvé live |
| Enrôlement / rotation DEK / récupération (rejoindre un compte) | ✅ livré 0.1.10 |
| Trait `BusinessStore` (ports & adapters) | ✅ en place |
| **Création de compte (bootstrap tenant : tenant_id + DEK + nom)** | ⬜ à construire |
| **Schéma `engine` / `business` (séparation)** | ⬜ à construire |
| **CRUD Produits + Clients** | ⬜ à construire |
| **tenant_id porté partout** | ⬜ à construire |
| **Nouveaux types d'opérations du journal** (create_produit, etc.) | ⬜ à construire |

---

## 8. Plan d'implémentation (à exécuter après validation)

1. **Onboarding tenant** : écran « Créer mon compte » (nom entreprise → tenant_id + DEK +
   code de récupération), en extension de l'assistant d'installation existant.
2. **Schémas PostgreSQL** : créer `engine` et `business` (migration), ranger les tables.
3. **Métier CRUD** : tables `produits`, `clients`, `ventes` — toutes estampillées `tenant_id`.
4. **Journal** : étendre `OpType` / `Payload` avec les nouvelles opérations métier.
5. **TDD** : pour chaque opération, écrire d'abord le test (surtout l'invariant), puis
   l'implémentation — comme l'anti-survente l'a été pour le socle.
6. **UI** : pages Tauri pour Produits et Clients (en plus de Stock/Journal/Sécurité/Parc).

---

## 9. Décisions verrouillées (récapitulatif)

- ✅ **1 PME = 1 tenant = 1 cluster (N machines, rôles différents) = 1 DEK = 1 jeu de données.**
- ✅ **Multi-tenant self-service** : chaque client crée son compte **localement** (souverain).
- ✅ **tenant_id** généré localement, partagé par le cluster, opaque pour le relais.
- ✅ **Séparation moteur/métier = Option A** (deux schémas dans une base PostgreSQL).
- ✅ **Métier = CRUD simple** (Produits, Clients) **+ une règle forte** (Ventes anti-survente).
- ✅ **Pas de serveur central de comptes** — la souveraineté l'interdit.

---

*Document de cadrage — à exécuter sur « top » de Jesse.*
