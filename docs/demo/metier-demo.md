# Démo — Couche métier multi-tenant (LOT 1 + LOT 2)

**Sovereign Data Agent** — EIGSI × AL BARAA — Jesse MPIGA-ODOUMBA
Version 0.1.11 (2026-06-07)

Démontre, sur le nœud actif (primary), la couche métier branchée sur le moteur souverain :
création de compte (tenant), puis CRUD Produits & Clients journalisés.

| Lot | Ce qui est démontré |
|-----|---------------------|
| LOT 1 | Création de compte auto-souveraine (tenant_id + DEK générés localement) |
| LOT 2 | CRUD Produits & Clients : chaque écriture chiffrée, journalisée, répliquée, opaque au relais |

---

## Pré-requis

- Réinstaller `Sovereign Data Agent_0.1.11_x64-setup.exe` sur le **PC (nœud actif)**.
  - `taskkill` préventif des process avant l'installeur (sinon `.exe` verrouillé).
- PostgreSQL 18 démarré localement.

---

## LOT 1 — Création de compte

1. Au premier lancement (ou après « Reconfigurer l'installation »), choisir le rôle **Nœud Actif**.
2. Écran de configuration : section **« Votre compte entreprise »**
   - Nom de l'entreprise (requis) : ex. *Boutique Salma*
   - Gérant, Email (optionnels)
3. Démarrer le nœud actif. Dans le log :
   - `Création de votre compte (génération du tenant_id local)...`
   - `Compte « Boutique Salma » créé ✓ (tenant a3f9…)`
4. Dans la barre latérale : le **nom de l'entreprise** s'affiche sous « Compte ».

**Lecture jury :** le `tenant_id` (UUID) est généré **dans le nœud du client**
(`Uuid::new_v4()` côté Rust), jamais par un serveur éditeur. L'identité de la PME est
souveraine dès sa création. Endpoint : `POST /tenant/bootstrap` (idempotent).

---

## LOT 2 — CRUD Produits & Clients

### Produits (onglet ⊠ Produits)
1. Créer un produit : SKU `PANTALON-L`, nom `Pantalon Lin`, prix `14,99 €` → **Créer**.
   - Message : « ✓ Produit créé et journalisé (chiffré). »
2. Modifier le produit (le SKU reste figé — clé de liaison avec le stock).
3. Créer un 2ᵉ produit, puis en supprimer un (suppression elle aussi journalisée).

### Clients (onglet ☻ Clients)
1. Créer un client : nom, email, téléphone → **Créer**.
2. Modifier / supprimer.

### Preuve que le métier passe par le moteur souverain
- **Journal chiffré** (onglet ⊞ Journal chiffré) : chaque opération métier apparaît comme
  un **blob opaque** (nonce + ciphertext hex). On ne voit jamais « Pantalon » en clair.
- **Réplication** : sur le standby (VM1), le schéma `business` est répliqué par WAL
  PostgreSQL → les produits/clients y sont présents (lecture).
- **Relais « Amane »** : le blob de chaque écriture métier est poussé au relais
  (best-effort), qui le stocke **sans pouvoir le déchiffrer**.

**Lecture jury :** le moteur est **agnostique au domaine**. Le journal ne connaît pas la
notion de « produit » : il transporte un `BusinessData { entity_id, fields }` générique
(`crates/core/src/journal.rs`). Le nœud actif range ces champs dans le schéma `business`.
C'est ce qui fait du socle un **framework** réutilisable pour n'importe quel métier.

---

## LOT 4 — Relais « Amane » multi-tenant

Le relais (renommé **« Amane »**, rôle `amane-relay`) est **mutualisé** : il sépare les
blobs **par tenant_id** sans jamais déchiffrer. Conforme à l'architecture cible officielle.

- Push actif → relais : inclut désormais le `tenant_id` (identité opaque de la PME).
- Stockage relais : clé composite `(tenant_id, seq)` → deux PME peuvent avoir le même seq
  sans collision (chacune a son propre journal).
- Fetch cloisonné : `GET /blobs?tenant_id=...` → un tenant ne récupère QUE ses blobs.
- `GET /health` expose `tenant_count` + la répartition par tenant (preuve du cloisonnement).

**Démo :** configurer le relais (page Configuration → URL du relais), faire une écriture
(produit ou vente) sur l'actif, puis vérifier sur le dashboard : `Relais « Amane »` →
`N blobs · 1 PME`. Le relais ne voit que des blobs opaques étiquetés par tenant_id.

**Lecture jury :** le relais est multi-tenant **et** aveugle — il range les sauvegardes de
plusieurs PME séparément, mais ne peut rien déchiffrer (aucune dépendance crypto, test
`relais_ne_contient_aucune_crypto` toujours vert). C'est exactement le « Relais Aveugle
(Amane Relay) » du document de référence.

⚠️ **Le relais (VM2) doit être en 0.1.12** pour gérer le tenant_id (sinon il stocke avec
tenant vide). Réinstaller VM2 avec le nouvel installeur.

---

## Architecture (rappel)

```
PostgreSQL (1 instance, répliquée WAL)
├── schema public   ← MOTEUR : operations_journal, journal_seq, node_epoch,
│                              enrolled_devices, dek_state, recovery_blob, tenant, stock
└── schema business ← MÉTIER : produits, clients   (estampillés tenant_id)
```

- **Atomicité (§5bis)** : une vente vérifie le stock ET écrit le journal dans UNE
  transaction (même base) → pas de 2PC, anti-survente préservée.
- **CRUD pur** (produits/clients) : pas d'invariant inter-lignes → pas de vérification de
  stock, mais journalisé/chiffré/répliqué comme tout le reste.

Endpoints métier (nœud actif) :
`GET/POST /produits`, `DELETE /produits/:id`, `GET/POST /clients`, `DELETE /clients/:id`.

---

## Points honnêtes (soutenance)

- Le **réplica SQLite du nœud passif** (`passive_node`) ne reconstruit que le stock ; le
  métier (`business`) est répliqué par **WAL PostgreSQL** vers le standby, pas par ce
  chemin SQLite de démonstration.
- Le **stock reste dans `public`** (chemin anti-survente prouvé, non déplacé pour éviter
  tout risque sur la réplication active). Les nouvelles entités métier sont dans `business`.
- La **séparation des blobs par tenant au relais** (multi-tenant strict) arrive au LOT 4.

---

## Tests automatisés

`cargo test --workspace` → 57 tests, dont pour ces lots :
- `operation_business_crud_journalisee_et_chainee` (journal générique + chaîne de hash)
- rétrocompatibilité : tous les tests stock/anti-survente/fencing restent verts.
