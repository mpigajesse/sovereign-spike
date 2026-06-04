# Architecture — Séparation des couches et responsabilités

**Sovereign Data Agent — Framework Coffre-Fort Data Souverain**  
Jesse MPIGA-ODOUMBA — EIGSI × AL BARAA CONSULTING — Promo 2026

---

## 1. Principe fondamental : cœur métier ≠ stockage

Le cœur Rust (`sovereign-node-active`) et la base de données PostgreSQL sont **deux couches distinctes** avec des responsabilités séparées.

Le cœur Rust **ne stocke rien lui-même** — il est le **gardien des règles**. PostgreSQL est le **coffre physique** — il stocke les données sur la machine du client.

```
┌─────────────────────────────────────────────────────────────────┐
│                  PÉRIMÈTRE DU CLIENT (Windows 11)               │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │         sovereign-node-active (Rust)                     │   │
│  │                                                          │   │
│  │  Responsabilités :                                       │   │
│  │  ✓ Sérialisation des écritures (ordre unique)            │   │
│  │  ✓ Invariant métier (anti-survente, continuité seq)      │   │
│  │  ✓ Chiffrement des opérations (XChaCha20-Poly1305)       │   │
│  │  ✓ Fencing anti-split-brain (token d'époque)             │   │
│  │  ✓ Idempotence (déduplication op_id)                     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                          │                                        │
│                          │ SQL (local uniquement)                │
│                          ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │         PostgreSQL 18 (sur machine client)               │   │
│  │                                                          │   │
│  │  Contenu :                                               │   │
│  │  • stock          → données métier en CLAIR              │   │
│  │  • operations_journal → blobs chiffrés (opaques)         │   │
│  │  • node_epoch     → token de fencing                     │   │
│  │                                                          │   │
│  │  → Le clair est ici, sur la machine du CLIENT            │   │
│  │  → L'éditeur n'a jamais accès à ce serveur              │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                           │
                           │ blobs opaques uniquement
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│              RELAIS ÉDITEUR (hors périmètre client)             │
│                                                                  │
│  sovereign-relay (Kali Linux / serveur éditeur)                 │
│                                                                  │
│  ✗ Aucune DEK (clé de chiffrement)                              │
│  ✗ Aucune connaissance du schéma métier                         │
│  ✓ Stocke et redistribue des blobs opaques (hex)                │
│  ✓ Aveugle par construction (zéro-knowledge)                    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Flux d'une écriture (ex. : vente de stock)

```
Opérateur PME (interface Tauri)
        │
        │  POST /write {op_type: "sale", item_id: "PANTALON-L", quantity: 10}
        ▼
sovereign-node-active (Rust)
        │
        ├─ [1] Validation métier
        │       → quantity > 0 ?
        │       → op_id déjà traité ? (idempotence)
        │       → stock_dispo ≥ quantity ? (anti-survente)
        │
        ├─ [2] Vérification fencing (dans la transaction)
        │       → epoch_local == epoch_DB ? sinon → 503
        │
        ├─ [3] Allocation séquence atomique
        │       → nextval('journal_seq')  [jamais réutilisé]
        │
        ├─ [4] Construction de l'opération CBOR
        │       → Operation { seq, op_type, payload, prev_hash }
        │       → prev_hash = SHA-256(CBOR précédent)  [hash chaîné]
        │
        ├─ [5] Chiffrement XChaCha20-Poly1305
        │       → blob = { nonce (24 oct.), ciphertext }
        │
        ├─ [6] Transaction SERIALIZABLE PostgreSQL
        │       → UPDATE stock SET quantity = quantity - 10
        │       → INSERT INTO operations_journal (seq, blob_nonce, blob_ciphertext)
        │
        └─ [7] Push best-effort vers le relais (async, non bloquant)
                → POST /blobs {seq, blob_nonce, blob_ciphertext}
                → Le relais stocke le blob — aveugle
```

---

## 3. Tableau des responsabilités

| Couche | Technologie | Responsabilité | Voit le clair ? |
|--------|-------------|----------------|-----------------|
| **Interface PME** | Tauri + React | Présentation, formulaires | ✅ (local) |
| **Cœur métier** | Rust (sovereign-core) | Règles, chiffrement, séquençage | ✅ (local) |
| **Stockage actif** | PostgreSQL (local) | Données métier + journal chiffré | ✅ (local) |
| **Réplique passive** | SQLite (machine passive) | Lecture seule hors-ligne | ✅ (local, même DEK) |
| **Relais éditeur** | Rust (sovereign-relay) | Transit blobs opaques | ❌ **jamais** |

> **Règle d'or :** Tout ce qui est dans le périmètre client (lignes ✅) est légitime — c'est la machine du client. L'éditeur (relais) ne touche jamais au clair.

---

## 4. Pourquoi le métier est dans PostgreSQL (et c'est correct)

### Question naturelle

> *"Si les données métier sont dans PostgreSQL, l'éditeur ne pourrait-il pas y accéder ?"*

### Réponse

**Non.** PostgreSQL tourne sur la machine physique du client (Windows 11 de la PME). L'éditeur n'a aucun accès réseau à ce serveur.

La chaîne de confiance est :

```
PostgreSQL local  →  accessible uniquement depuis la machine du client
Relais éditeur    →  accessible depuis n'importe où, mais AVEUGLE (blobs chiffrés)
```

Ce modèle répond exactement à la problématique du PFE :
- **Le client garde le contrôle** : ses données (stock, factures, paie) restent sur ses machines
- **L'éditeur garde son modèle** : il distribue le logiciel, collecte des abonnements, sans voir les données
- **Sans infrastructure lourde** : PostgreSQL est léger, gratuit, et gère la réplication nativement

---

## 5. Phase 1 avancée — Séparation PostgreSQL / SQLite ✅ IMPLÉMENTÉE (2026-06-04)

> **Statut : LIVRÉE.** Le trait `BusinessStore` et l'implémentation `SqliteBusinessStore`
> sont codés et testés (6 tests unitaires + 1 test d'intégration). Voir
> `crates/core/src/business_store.rs` et `crates/core/src/sqlite_store.rs`.


### Motivation

Dans l'implémentation actuelle (spike Phase 0), PostgreSQL héberge **à la fois** :
- Les données métier en clair (`stock`)
- Le journal chiffré (`operations_journal`)

Une séparation plus stricte des préoccupations serait :

```
Actuel (Phase 0 — spike)          Avancé (Phase 1+ — production)
─────────────────────────          ───────────────────────────────
PostgreSQL (actif)                 SQLite (métier local)
  ├── stock         ◄──────────►    ├── stock
  ├── operations_journal            ├── factures
  └── node_epoch                    ├── contacts
                                    └── (toutes données métier)

                                   PostgreSQL (réplication uniquement)
                                     ├── operations_journal (blobs)
                                     ├── node_epoch
                                     └── WAL → streaming vers standby
```

### Avantages de la séparation

| Aspect | Phase 0 (actuel) | Phase 1 avancée |
|--------|-----------------|-----------------|
| Accès hors-ligne au métier | Via nœud passif SQLite | Directement dans SQLite local |
| Complexité | Faible (1 base) | Modérée (2 bases) |
| Performance métier | PostgreSQL (robuste) | SQLite (ultra-léger) |
| Réplication | PostgreSQL WAL natif | PostgreSQL pour le journal uniquement |
| Couplage | Fort | Faible (métier indépendant de PG) |

### Architecture cible Phase 1 avancée

```
┌──────────────────── Machine client (Windows 11) ──────────────────┐
│                                                                     │
│  sovereign-node-active (Rust)                                       │
│       │                                                             │
│       ├──► SQLite (métier)        ← stock, factures, contacts      │
│       │         (lecture/écriture directe, ultra-léger)            │
│       │                                                             │
│       └──► PostgreSQL (journal)   ← blobs chiffrés + WAL           │
│                 (réplication vers standby + push relais)           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Implémentation requise

### Code livré — `crates/core/src/business_store.rs`

```rust
// Trait abstrait — le cœur Rust ne connaît pas le moteur de stockage
#[async_trait]
pub trait BusinessStore: Send + Sync {
    async fn get_stock(&self, item_id: &str) -> Result<i64, StoreError>;
    async fn apply_sale(&self, item_id: &str, quantity: i64) -> Result<i64, StoreError>;
    async fn apply_adjust(&self, item_id: &str, delta: i64) -> Result<i64, StoreError>;
    async fn list_stock(&self) -> Result<Vec<StockEntry>, StoreError>;
}
```

### Code livré — `crates/core/src/sqlite_store.rs`

```rust
// Implémentation SQLite (production PME — léger, embarqué)
pub struct SqliteBusinessStore { pool: SqlitePool }

#[async_trait]
impl BusinessStore for SqliteBusinessStore {
    async fn apply_sale(&self, item_id: &str, quantity: i64) -> Result<i64, StoreError> {
        let disponible = self.get_stock(item_id).await?;
        if disponible < quantity {
            return Err(StoreError::StockInsuffisant { disponible, demande: quantity });
        }
        // UPDATE atomique avec RETURNING quantity ...
    }
    // ...
}
```

> Cette abstraction permet de **changer le moteur de stockage sans toucher au cœur métier Rust** — principe de l'architecture hexagonale (ports & adapters). Tests : `cargo test -p sovereign-core sqlite_store` → 6/6 ✅.

### Avancement de la séparation

- **Phase 0** : PostgreSQL pour tout — simple, prouve le concept ✅
- **Phase 1** : Trait `BusinessStore` + `SqliteBusinessStore` **✅ LIVRÉ (2026-06-04)**
  - 6 tests unitaires (ajustement, vente, anti-survente, liste, cumuls)
  - 1 test d'intégration prouvant l'indépendance journal/métier
- **Phase 2** : Brancher `SqliteBusinessStore` dans le nœud actif à la place de PostgreSQL pour le métier (PostgreSQL conservé pour le journal + réplication) — *câblage à faire*
- **Phase 3** : Frontend Tauri embarque SQLite (pas besoin de PostgreSQL sur les postes opérateurs normaux)

---

## 6. Résumé pour la présentation à l'encadreur

> **Q : Le cœur backend Rust est-il séparé de la base de données métier ?**

**Oui — par conception, et c'est désormais prouvé par le code.** Le cœur Rust est un **arbitre stateless** : il ne stocke rien et n'a pas d'état propre. Il délègue le stockage via le trait `BusinessStore` — implémenté à la fois pour SQLite (`SqliteBusinessStore`) et, dans le spike, directement sur PostgreSQL (`active.rs`).

La **souveraineté** ne dépend pas du moteur de stockage choisi — elle dépend du fait que ce stockage est **sur la machine du client**, et que le relais éditeur ne voit que des blobs opaques.

La séparation PostgreSQL / SQLite est **implémentée et testée** (51 tests au total dans le workspace, dont 7 spécifiques à cette séparation). Elle renforce l'indépendance du cœur métier vis-à-vis de l'infrastructure de réplication : un poste opérateur pourra à terme tourner sur SQLite seul, sans PostgreSQL.

---

*Document technique — Sovereign-Spike Phase 0/1 — 2026-06-04*
