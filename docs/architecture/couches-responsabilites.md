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
- **Où le trait est utilisé** :
  - Nœud **passif** → SQLite (réplique reconstruite depuis le journal) ✅
  - Mode **PME solo / poste autonome** (futur) → SQLite seul, sans PostgreSQL
  - Nœud **actif** → **reste sur PostgreSQL délibérément** (voir §5bis)

---

## 5bis. Décision d'architecture — Pourquoi le nœud actif NE migre PAS vers SQLite

> ⚠️ **Décision actée (2026-06-04).** Le nœud actif conserve PostgreSQL pour le **métier
> ET le journal dans une seule transaction**. On ne sépare PAS les deux sur l'actif.
> Cette décision est volontaire et fondée sur la sécurité des invariants.

### Le raisonnement

Le nœud actif exécute aujourd'hui, dans **une seule transaction SERIALIZABLE PostgreSQL** :

```
BEGIN SERIALIZABLE
  1. assert_primary()              -- fencing : époque locale == époque DB ?
  2. SELECT quantity ... FOR UPDATE -- lecture verrouillée du stock
  3. vérification stock ≥ quantité  -- invariant métier (anti-survente)
  4. UPDATE stock ...               -- écriture de l'état métier
  5. INSERT operations_journal ...  -- écriture du blob chiffré
COMMIT   -- atomique : les 5 étapes réussissent ou échouent ENSEMBLE
```

**C'est cette atomicité qui garantit l'absence de survente sous concurrence.** La vérification de stock (étape 3) et l'écriture du journal (étape 5) sont indissociables.

### Ce que la séparation SQLite/PostgreSQL casserait sur l'actif

Si on plaçait le métier dans SQLite et le journal dans PostgreSQL :

```
Transaction SQLite      { vérifie stock, écrit stock }   -- base A
Transaction PostgreSQL  { écrit le journal }              -- base B
```

→ **Deux transactions sur deux bases distinctes.** Entre les deux, une panne laisse un état incohérent :
- stock décrémenté mais journal non écrit → la vente disparaît à la réplication
- ou journal écrit mais stock non décrémenté → survente au prochain rejeu

Restaurer l'atomicité exigerait un **commit en deux phases (2PC)** — un protocole de transaction distribuée **complexe et faillible**, exactement le type de mécanisme que le projet a voulu éviter (cf. document de cadrage §4.2 : « on n'écrit pas un consensus à la main »).

### La règle générale

| Atomicité | Coût | Fiabilité |
| :--- | :--- | :--- |
| **Intra-base** (une transaction, une base) | Gratuite | Infaillible (garantie par le moteur) |
| **Inter-bases** (2PC) | Élevé | Faillible (cas limites nombreux) |

> **Principe :** garder les opérations qui doivent être atomiques **dans la même base**. Le nœud actif arbitre les écritures → il a besoin de l'atomicité → il reste mono-base (PostgreSQL).

### Où SQLite a sa place — sans ce problème

| Nœud | Moteur | Atomicité requise entre stock et journal ? |
| :--- | :--- | :--- |
| **Actif** | PostgreSQL | ✅ **Oui** — il arbitre les écritures → mono-base obligatoire |
| **Passif** | SQLite | ❌ Non — il **rejoue** un journal déjà ordonné, ne décide rien |
| **PME solo** (futur) | SQLite | ❌ Non — un seul poste, pas de réplication, pas de journal séparé |

Le nœud **passif** applique le stock et avance son pointeur `sync_state` dans **sa propre** transaction SQLite — mais il n'arbitre aucune écriture concurrente, donc aucun invariant métier n'est en jeu : la convergence vient de l'ordre du journal, pas d'un verrou.

### Conclusion pour la soutenance

> Le trait `BusinessStore` prouve que le cœur métier est **découplé** du moteur de stockage.
> Mais le découplage n'oblige pas à séparer les bases **partout** : sur l'actif, garder
> métier+journal dans une transaction PostgreSQL unique est **un choix de sécurité**, pas une
> dette technique. On utilise SQLite là où l'atomicité inter-bases n'est pas en jeu (passif, solo).

---

## 5ter. Reste à faire — Mode PME solo (SQLite autonome)

- **Phase 3** : Frontend Tauri embarque `SqliteBusinessStore` pour un **poste autonome**
  (PME mono-poste sans PostgreSQL). Le trait est déjà prêt — il suffira de l'instancier
  dans le backend Tauri pour un mode « démo / solo » qui ne dépend d'aucun serveur.

---

## 6. Résumé pour la présentation à l'encadreur

> **Q : Le cœur backend Rust est-il séparé de la base de données métier ?**

**Oui — par conception, et c'est désormais prouvé par le code.** Le cœur Rust est un **arbitre stateless** : il ne stocke rien et n'a pas d'état propre. Il délègue le stockage via le trait `BusinessStore` — implémenté à la fois pour SQLite (`SqliteBusinessStore`) et, dans le spike, directement sur PostgreSQL (`active.rs`).

La **souveraineté** ne dépend pas du moteur de stockage choisi — elle dépend du fait que ce stockage est **sur la machine du client**, et que le relais éditeur ne voit que des blobs opaques.

La séparation PostgreSQL / SQLite est **implémentée et testée** (51 tests au total dans le workspace, dont 7 spécifiques à cette séparation). Elle renforce l'indépendance du cœur métier vis-à-vis de l'infrastructure de réplication : un poste opérateur pourra à terme tourner sur SQLite seul, sans PostgreSQL.

---

*Document technique — Sovereign-Spike Phase 0/1 — 2026-06-04*
