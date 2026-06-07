# Plan d'implémentation — Couche métier multi-tenant (aligné architecture cible)

**Sovereign Data Agent** — EIGSI × AL BARAA — Jesse MPIGA-ODOUMBA
Date : 2026-06-07 — Socle : 0.1.10

> Plan d'ensemble à valider **avant tout code**. Aligné sur la référence officielle
> `officiel-docs/Architecture Haute Disponibilité SaaS.md` et sur
> `architecture-multitenant-et-metier.md`. Méthode : socle d'abord (prouvé), puis métier,
> en incréments testables (TDD).

---

## 1. Rappel de la cible (réf. officielle)

```
        INFRASTRUCTURE ÉDITEUR SAAS
   ┌──────────────────────────────────┐
   │  Éditeur : Licences · UI · MAJ    │
   │  Relais « Amane » (aveugle,       │  ← mutualisé, multi-tenant, zero-knowledge
   │  zero-knowledge, sauvegarde)      │
   └──────┬───────────┬───────────┬────┘
          ▼           ▼           ▼
       PME A       PME B       PME C
      (DEK A)     (DEK B)     (DEK C)
      Actif       Actif       Actif        ← seul nœud qui écrit + voit le clair
      /   \       /   \       /   \
   Pas1  Pas2  Pas1  Pas2  Pas1  Pas2       ← 1 actif + 2 passifs = quorum HA
```

Invariants de la cible :
- **1 PME = 1 tenant = 1 cluster (1 actif + 2 passifs) = 1 DEK.**
- **Relais Amane mutualisé**, aveugle, sépare les blobs par tenant, ne déchiffre jamais.
- **Éditeur** fournit logiciel / UI / mises à jour / licences. Ne voit jamais le clair.
- **Clair uniquement dans le périmètre client.**

---

## 2. État actuel vs cible (analyse d'écart)

| Élément | Actuel (0.1.10) | Cible | Écart |
|---------|-----------------|-------|-------|
| Moteur (journal, fencing, réplication) | ✅ prouvé | idem | — |
| Anti-survente (vente/stock) | ✅ prouvé | idem | — |
| Enrôlement / rotation DEK / récupération | ✅ 0.1.10 | idem | — |
| **Création de compte (tenant)** | ❌ DEK générée mais pas de tenant_id ni nom | bootstrap tenant local | **à faire** |
| **tenant_id** | ❌ absent | porté partout | **à faire** |
| **Séparation moteur / métier** | ❌ tout en `public` | schémas `engine` + `business` | **à faire** |
| **CRUD Produits / Clients** | ❌ | présent | **à faire** |
| **Relais multi-tenant « Amane »** | 🟡 relais OK mais mono-espace, pas de tenant_id | namespacing par tenant_id | **à faire** |
| **Topologie actif + 2 passifs + quorum** | 🟡 actif + 1 passif | actif + 2 passifs, failover auto | **reporté** |
| **Plan de contrôle éditeur (licences)** | ❌ | présent | **hors-spike (futur)** |

---

## 3. Décisions structurantes (à acter dans ce plan)

### D1 — Séparation des schémas : NON-disruptive
On ne **déplace pas** les tables moteur déjà prouvées et répliquées (`operations_journal`,
`journal_seq`, `node_epoch`, `enrolled_devices`, `dek_state`, `recovery_blob`). On les
considère comme le **schéma moteur de fait** (elles restent dans `public`). On crée
**uniquement** un nouveau schéma **`business`** pour les nouvelles tables métier.

> Raison : migrer des tables sous réplication WAL active est risqué pour zéro bénéfice. La
> séparation logique est obtenue par le **nouveau** schéma `business`. (Option A, version
> pragmatique.) On documentera `public` = moteur, `business` = métier.

### D2 — Atomicité préservée (§5bis)
Les tables `business` sont dans la **même base PostgreSQL** que le moteur → une opération à
invariant fort (vente) vérifie le stock (`business`) ET écrit le journal (`public`) dans
**une seule transaction SERIALIZABLE**. Pas de 2PC.

### D3 — tenant_id porté partout
- Généré à la création de compte (UUID), stocké dans une table `tenant` (schéma `public`)
  et dans la config locale.
- Colonne `tenant_id` sur les tables `business` et sur `operations_journal` (et sur les
  blobs poussés au relais).
- En mono-tenant par cluster, c'est une valeur constante — mais elle rend le design
  honnête et prêt pour le multi-PME, et permet au relais de séparer les blobs.

### D4 — Compte auto-souverain
Création de compte = bootstrap **local** (tenant_id + DEK + code de récupération). Aucun
serveur central de comptes. Le relais ne voit qu'un tenant_id opaque.

### D5 — Métier minimal mais avec une règle forte
`Produits` + `Clients` = CRUD pur (démontre la généricité du moteur). `Ventes` = invariant
fort (anti-survente, déjà prouvé) → démontre la valeur du moteur.

---

## 4. Lots de travail (séquencés)

### LOT 1 — Fondation tenant (création de compte) ✅ LIVRÉ (0.1.11)
**But :** un nouveau client crée son compte → tenant_id + DEK + code récup, en local.

- Migration `public.tenant (tenant_id UUID PK, nom TEXT, created_at)`.
- Cœur : fonction de bootstrap (génère tenant_id, DEK, recovery) — réutilise `crypto.rs`.
- Endpoint actif : `POST /tenant/bootstrap {nom}` → crée le tenant, amorce `dek_state`.
- Endpoint : `GET /tenant` → identité du tenant courant.
- Tauri : commande + écran « Créer mon compte » (extension de `Install.tsx`).
- Tests : bootstrap génère tenant_id unique + DEK + recovery cohérents.

**Livrable démontrable :** « Je crée le compte *Boutique Salma* → tenant_id + clé générés localement, cette machine devient l'actif. »

### LOT 2 — Schéma `business` + CRUD Produits & Clients (CRUD pur) ✅ LIVRÉ (0.1.11)
**But :** prouver que le moteur achemine des données métier simples, sans règle.

- Migration `business.produits (id, tenant_id, sku, nom, prix, ...)`,
  `business.clients (id, tenant_id, nom, email, ...)`.
- Cœur : étendre `OpType` / `Payload` (journal) avec `ProduitCreate/Update/Delete`,
  `ClientCreate/Update/Delete`. Chiffré comme le reste.
- Actif : endpoints CRUD → chaque écriture = transaction (business + journal) + push relais.
- `BusinessStore` : étendre le trait / l'implémentation pour produits & clients.
- Tauri : pages « Produits » et « Clients » (CRUD).
- Tests : création/MAJ/suppression journalisées + rejouables ; tenant_id porté.

**Livrable démontrable :** « Je crée un produit sur l'actif → il est journalisé (chiffré), répliqué au passif (lecture), opaque au relais. »

### LOT 3 — Ventes (invariant fort) ✅ DÉJÀ COUVERT
**But :** rattacher l'anti-survente déjà prouvée au nouveau modèle.

> **Statut :** le flux ventes/anti-survente (`/write`, page Stock) est **déjà prouvé**
> et inchangé. Décision D1 : le `stock` reste dans `public` (chemin prouvé non déplacé,
> zéro risque). Les produits (LOT 2) se lient au stock par `sku = item_id`. Pas de
> nouveau code nécessaire — la règle forte est déjà démontrée.

(spécification d'origine ci-dessous, conservée pour référence)

- Migration : `business.stock` (ou conserver `public.stock` et le référencer) + `tenant_id`.
- Réutiliser le flux SERIALIZABLE existant (active.rs) en pointant le stock vers `business`.
- Tests : anti-survente toujours verte sous concurrence, avec tenant_id.

**Livrable démontrable :** « Deux ventes concurrentes du dernier article → une seule passe (409), tenant_id respecté. »

### LOT 4 — Relais « Amane » multi-tenant ✅ LIVRÉ (0.1.12)
**But :** séparer les blobs par tenant, conformément à la cible mutualisée.

- Renommer le rôle/identité en « Amane Relay » (logs, /health, UI).
- Push actif : ajouter `tenant_id` au corps `POST /blobs`.
- Relais : colonne `tenant_id` sur `relay_blobs`, clé primaire `(tenant_id, seq)`,
  `GET /blobs?tenant_id=...&after_seq=...`. Reste **aveugle** (jamais de déchiffrement).
- Tests : deux tenants → blobs cloisonnés, aucun mélange ; toujours zero-knowledge.

**Livrable démontrable :** « Le relais stocke les blobs de 2 PME séparément, sans rien déchiffrer. »

### LOT 5 — (reporté) Topologie actif + 2 passifs + failover automatique par quorum
**But :** atteindre la HA cible (1 actif + 2 passifs) + élection automatique (Patroni/etcd).

- Ajouter un 2ᵉ passif au cluster de démo.
- Évaluer Patroni + etcd pour l'élection automatique (critère §7.4 #4b).
- **Reporté** par décision (2026-06-06) ; le manuel reste prouvé.

### LOT 6 — (futur, hors-spike) Plan de contrôle éditeur
Licences, distribution des mises à jour, métriques. Hors périmètre PFE immédiat.

---

## 5. Séquencement & dépendances

```
LOT 1 (compte/tenant_id)
   └─> LOT 2 (schéma business + CRUD Produits/Clients)
          └─> LOT 3 (Ventes dans business, invariant)
                 └─> LOT 4 (Relais Amane multi-tenant)
                        └─> LOT 5 (topologie + quorum — reporté)
                               └─> LOT 6 (éditeur/licences — futur)
```

Chemin critique pour la soutenance : **LOT 1 → 2 → 3 → 4**. Les LOTs 5 et 6 sont
explicitement reportés/futurs et assumés comme tels.

---

## 6. Méthode (rappel)

- **TDD** : pour chaque opération, test d'abord (surtout les invariants), puis implémentation.
- **Incréments démontrables** : chaque LOT produit une démo live (cf. « Livrable »).
- **Versioning** : un bump de version par LOT livré (0.1.11, 0.1.12, …), rebuild du
  sidecar nœud **obligatoire** avant chaque build NSIS (le code métier est dans le nœud).
- **Docs** : un guide de démo par LOT (comme `docs/demo/gestion-parc-demo.md`).

---

## 7. Points ouverts à confirmer avant LOT 1

1. **Objets métier** : Produits + Clients + Ventes — OK, ou ajouter/retirer ?
2. **Champs** : quels champs minimaux pour Produits (sku, nom, prix, stock ?) et Clients
   (nom, email, téléphone ?) ?
3. **Création de compte** : champs demandés (juste le nom de l'entreprise, ou aussi
   gérant / email de contact) ?
4. **Schéma** : valider D1 (nouveau schéma `business`, moteur reste dans `public`).

---

*Plan de référence — exécution sur « top » de Jesse, LOT par LOT.*
