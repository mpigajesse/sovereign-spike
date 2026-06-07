# TASKS — Sovereign-Spike PFE Jesse MPIGA-ODOUMBA

**Titre :** Conception et Implémentation d'une Architecture Coffre-Fort Data P2P Souveraine  
**Étudiant :** Jesse MPIGA-ODOUMBA — EIGSI, Promo 2026  
**Entreprise :** AL BARAA CONSULTING  
**Dernière mise à jour :** 2026-06-04

---

## Légende

- ✅ Terminé et validé
- 🔄 En cours
- ⬜ À faire
- ❌ Bloqué / dépendance

---

## PHASE 0 — Spike de dérisquage ✅ COMPLÈTE

> Objectif : prouver le socle technique avant tout code métier.

### Crypto + Sérialisation

- ✅ Chiffrement XChaCha20-Poly1305 via libsodium (crate `sodiumoxide`)
- ✅ DEK (Data Encryption Key) symétrique par entreprise
- ✅ Journal CBOR chiffré (crate `ciborium`) — blobs opaques hex
- ✅ Nonce unique par opération
- ✅ Idempotence via `op_id` UUID (déduplication des doublons réseau)

### Réplication PostgreSQL

- ✅ Nœud actif Windows 11 (PostgreSQL 18 primary, WAL level=replica)
- ✅ Slot de réplication physique `sovereign_slot`
- ✅ Rôle `replicator` configuré (pg_hba.conf + scram-sha-256)
- ✅ Nœud passif Ubuntu 26.04 (pg_basebackup + hot_standby)
- ✅ Streaming replication active (state=streaming, last_seq synchronisé)

### Nœuds applicatifs Rust

- ✅ `sovereign-node-active` — API HTTP (axum), écritures PostgreSQL, journal chiffré
- ✅ `sovereign-node-passive` — sync journal toutes 5s, déchiffrement, SQLite local
- ✅ `sovereign-relay` — relais aveugle (zero-knowledge, aucune DEK)
- ✅ Époque de fencing (protection anti-split-brain au démarrage)

### Invariants métier prouvés

- ✅ Anti-survente (409 Conflict si stock insuffisant)
- ✅ Idempotence op_id (seq identique si même op_id)
- ✅ Cohérence actif ↔ passif (PANTALON-L = même quantité)
- ✅ Zero-knowledge relais (blob opaque, aucun déchiffrement possible)

### Tests

- ✅ Test E2E 13/13 réussis (03_test_e2e.ps1)
- ✅ Scripts d'installation Ubuntu (01, 02_setup_standby, 03_start_passive)
- ✅ Scripts d'installation Kali (01_install_deps, 02_start_relay)
- ✅ Documentation docs/install/ complète et à jour

---

## PHASE 1 — Cœur Rust production ✅ COMPLÈTE

> Objectif : solidifier le cœur cryptographique et le journal avant le frontend.

### 1.1 Cryptographie

- ✅ Hiérarchie de clés complète (X25519 + sealed box + Argon2id)
- ✅ Paire de clés X25519 par appareil (`DeviceKeypair`)
- ✅ DEK emballée (sealed box) vers chaque appareil autorisé
- ✅ Enrôlement / révocation / rotation DEK (`DeviceRegistry`)
- ✅ Dérivation de clé depuis mot de passe (Argon2id)
- ✅ 8 tests unitaires crypto (dont vector de rotation et zéro-knowledge)

### 1.2 Journal CBOR

- ✅ Format CBOR versionné (`schema_version: u8`)
- ✅ Replay du journal pour reconstruction de l'état (`MemoryJournal::replay`)
- ✅ Hash chaîné entre entrées (SHA-256, `prev_hash`) — 2026-06-04
- ✅ Vérification d'intégrité (`verify_chain`) — altération détectée
- ✅ Tests : replay idempotent, séquence hors ordre rejetée, hash chaîné

### 1.3 Réplication PostgreSQL + Fencing

- ✅ Bascule manuelle documentée (docs/install/demo-guide.md §Failover)
- ✅ Époque de fencing (`EpochGuard`, `promote_epoch`)
- ✅ Détection ancien actif dans la transaction SERIALIZABLE
- ✅ Endpoint `POST /epoch/promote` — incrémentation époque
- ✅ 11 critères d'acceptation Phase 0 en tests (`spike_scenario.rs`)

### 1.4 API cœur

- ✅ Endpoints : `/write`, `/stock`, `/journal`, `/epoch`, `/epoch/promote`
- ✅ `DeviceRegistry` avec enrôlement, révocation, rotation DEK
- ✅ Endpoints HTTP parc : `/devices`, `/devices/enroll`, `/devices/revoke`,
  `/recovery/setup`, `/recovery/restore` — 2026-06-06 (`crates/node/src/devices.rs`)
- ✅ DEK opérationnelle MUTABLE par génération (`dek_state`) — rotation réelle au dé-enrôlement
- ⬜ Compactage journal (snapshot + reset — low priority)

### 1.5 Séparation stockage (architecture hexagonale) ✅ LIVRÉ 2026-06-04

- ✅ Trait `BusinessStore` (ports & adapters) — `crates/core/src/business_store.rs`
- ✅ `SqliteBusinessStore` — implémentation SQLite légère (production PME)
- ✅ 6 tests unitaires (ajustement, vente, anti-survente, liste, cumuls)
- ✅ Test d'intégration : indépendance journal chiffré ↔ stockage métier
- ⬜ Câbler `SqliteBusinessStore` dans le nœud actif à la place de PG-métier
- 📄 Documenté : `docs/architecture/couches-responsabilites.md` §5

---

## PHASE 2 — Relais éditeur + synchronisation multi-sites ✅ COMPLÈTE

> Objectif : permettre à la PME d'avoir plusieurs sites distants + sauvegarde hors-site chiffrée.

### 2.1 Relais aveugle (production)

- ✅ Push automatique du journal chiffré vers le relais (best-effort, non bloquant) — 2026-06-04
- ✅ Auth relais par `RELAY_API_KEY` (header X-Relay-Key)
- ✅ Fetch du journal depuis le relais (passif distant via GET /blobs)
- ✅ Rétention limitée (RELAY_MAX_BLOBS configurable)
- ✅ Stockage persistant SQLite (`relay_blobs` table, survit aux redémarrages) — 2026-06-04

### 2.2 Synchronisation multi-sites

- ✅ Site actif (Windows) → blobs poussés au relais automatiquement
- ✅ Site passif (Ubuntu) → synchronise via `/journal` de l'actif OU via relais
- ⬜ Fallback automatique relais si actif injoignable (architecture prévue)
- ⬜ Test réseau coupé → fallback → reconnecté → cohérence

### 2.3 Plan de contrôle éditeur

- ⬜ Endpoint `/license` (hors-scope PFE — prévu Phase suivante)
- ⬜ Distribution mises à jour via relais (hors-scope PFE)

---

## PHASE 3 — Frontend métier + installeur PME ⬜

> Objectif : rendre la solution accessible à une PME sans informaticien.

### 3.1 Installeur one-click (Windows)

- ✅ Script `00_install_all.ps1` — 2026-06-04 :
  - ✅ Télécharge et installe PostgreSQL 18 silencieusement
  - ✅ Installe Rust automatiquement si absent
  - ✅ Compile le binaire depuis les sources
  - ✅ Génère la DEK automatiquement (`shared.env`)
  - ✅ Crée un service Windows (démarrage automatique)
  - ✅ Configure les règles pare-feu
- ⬜ Installeur graphique Tauri (Phase 3 avancée — prévu)

### 3.2 Desktop Tauri (Windows + Linux)

- ✅ Scaffold Tauri 2 + React/TypeScript + Vite — 2026-06-04
- ✅ Backend Tauri (commandes : load_config, start_node, stop_node, check_health)
- ✅ Dashboard : état 3 nœuds en temps réel, métriques, propriétés sécurité
- ✅ Page Stock : vente / ajustement, consultation actif↔passif, cohérence
- ✅ Page Journal : blobs chiffrés paginés, explication zéro-knowledge
- ✅ Page Sécurité : fencing/époque, stack cryptographique, promote
- ✅ UI design dark premium (CSS custom properties, sans framework CSS)
- ✅ Build production fonctionnel (dist/ ~175KB JS + 5KB CSS)
- ✅ Icônes app générées (cargo tauri icon) — 2026-06-04
- ✅ Bundle .exe NSIS + MSI (`tauri build`) — 4.9 MB — 2026-06-04
- ✅ Sidecar `sovereign-node-active` embarqué dans le .exe
- ✅ Assistant installation : choix du rôle (Primary/Standby/Relais/Client)

### 3.2bis Mode PME Solo (SQLite autonome) — ❌ RETIRÉ 2026-06-07

> Livré le 2026-06-04 (binaire `sovereign-node-solo`, rôle "PME Solo" dans
> l'assistant, redémarrage auto, 7/7 tests E2E), puis **retiré** : ce mode
> mono-poste sans PostgreSQL ne correspond pas à l'architecture officielle
> retenue, qui impose toujours un cluster actif/passif (cf.
> `officiel-docs/Architecture Haute Disponibilité SaaS.md`). Conservé ici
> pour l'historique de livraison ; le code (binaire, sidecar, commandes
> Tauri, UI d'installation) a été supprimé du dépôt.
>
> Détail de ce qui avait été livré :
> - Binaire `sovereign-node-solo` (même API HTTP que l'actif : `/write`, `/stock`, `/journal`, `/epoch`)
> - Métier + journal chiffré dans UNE base SQLite (atomicité préservée)
> - Anti-survente, idempotence, journal chiffré, WAL + busy_timeout
> - Testé E2E : 7/7 (santé, stock, anti-survente, idempotence, journal, époque)
> - Persistance vérifiée, embarqué dans le .exe, redémarrage auto

### 3.3 Mobile UniFFI (Phase 3 avancée)

- ⬜ Exposition du cœur Rust via UniFFI (bindings Swift/Kotlin)
- ⬜ iOS : SwiftUI, lecture hors-ligne
- ⬜ Android : Jetpack Compose, lecture hors-ligne

---

## Tâches transversales ⬜

### Documentation

- ⬜ Rapport PFE (rédaction finale)
- ⬜ Diagrammes d'architecture (PlantUML / draw.io)
- ⬜ Preuve formelle zéro-knowledge (section soutenance)
- ✅ docs/install/ — guides de déploiement à jour

### Soutenance EIGSI

- ⬜ Slides de présentation (30 min)
- ⬜ Démonstration live (3 nœuds, test E2E, failover)
- ⬜ Réponses aux questions jury (crypto, CRDT vs sérialisation, coût PME)

---

## COUCHE MÉTIER multi-tenant (architecture cible HA SaaS) — en cours

> Réf. : `officiel-docs/Architecture Haute Disponibilité SaaS.md` +
> `docs/metier/plan-implementation.md`. Démo : `docs/demo/metier-demo.md`.

| Lot | Contenu | État |
|-----|---------|------|
| LOT 1 | Création de compte (tenant_id + DEK générés localement, auto-souverain) | ✅ 0.1.11 |
| LOT 2 | Schéma `business` + CRUD Produits & Clients (journalisé, chiffré, générique) | ✅ 0.1.11 |
| LOT 3 | Ventes / anti-survente (invariant fort) | ✅ déjà prouvé (inchangé) |
| LOT 4 | Relais « Amane » multi-tenant (blobs séparés par tenant_id) | ✅ 0.1.12 |
| LOT 5 | Topologie actif + 2 passifs + failover automatique quorum | ✅ 0.1.13 (superviseur Rust, code+tests) |
| LOT 6 | Plan de contrôle éditeur (licences, MAJ) | ⬜ futur hors-spike |

Décisions actées : D1 séparation non-disruptive (`business` schéma, moteur reste `public`),
D2 atomicité préservée (même base), D3 tenant_id porté, D4 compte auto-souverain,
D5 métier minimal + une règle forte. Journal étendu en générique (`BusinessData`),
rétrocompatible (blobs stock inchangés au bit près). Tests : 57/57.

---

## État réel des 10 critères d'acceptation officiels (§7.4 du cadrage)

> Distinction honnête : **prouvé en live** (démontrable avec de vraies machines) vs
> **code + tests** (logique présente et testée, pas encore un flux runtime complet) vs
> **manquant**.

| Critère §7.4 | État | Preuve |
|---|---|---|
| 1. Écritures concurrentes sans survente | ✅ live | SERIALIZABLE + FOR UPDATE, 409 |
| 2. Coupé du serveur → lecture OK, écriture refusée | ✅ par construction | passif sans `/write` |
| 3. Confirmation après réplication (synchrone) | ✅ live | `synchronous_standby_names` |
| 4a. Failover **manuel** sans perte | ✅ live | promotion standby zéro perte |
| 4b. Failover **automatique** par quorum (≥3) | ✅ code+tests (0.1.13) | superviseur Rust : majorité stricte → `pg_ctl promote` + époque (17 tests) |
| 5. Pas de split-brain sous coupure | ✅ code+tests | prévention par quorum (minorité refuse) + fencing après coup |
| 6. Retour ancien actif → fencé | ✅ live | 503 si époque périmée |
| 7. Relais → que du chiffré | ✅ live | blobs opaques, zéro dép. crypto |
| 8. Enrôlement sans exposer la clé | ✅ live (0.1.10) | page Parc : sealed box + unwrap |
| 9. Dé-enrôlé ne peut plus déchiffrer | ✅ live (0.1.10) | rotation DEK + preuve sur blob réel |
| 10. Retrait → recalcul quorum + alerte | ✅ live (0.1.10) | bannière quorum < 3 |
| 11. Perte machines → code de récupération | ✅ live (0.1.10) | Argon2id setup/restore |

**Optionnel non implémenté :** partage de secret Shamir (§5.6 — explicitement optionnel).

Démo : `docs/demo/gestion-parc-demo.md`.

---

## Résumé de progression

| Phase | Avancement | Statut |
|-------|-----------|--------|
| Phase 0 — Spike dérisquage | 100% | ✅ Validé 2026-06-04 |
| Phase 1 — Cœur Rust production | 98% | ✅ Validé 2026-06-04 (+ séparation SQLite/PG) |
| Phase 2 — Relais + multi-sites | 85% | ✅ Validé 2026-06-04 |
| Phase 3 — Installeur one-click | 70% | ✅ Script PowerShell créé |
| Phase 3 — Frontend Tauri | 90% | ✅ App + gestion du parc (0.1.10), build OK |
| Phase 3 — Mobile UniFFI | 0% | ⬜ Architecture définie, hors-scope PFE immédiat |
| Gestion du parc (#8–#11) | 100% | ✅ Livré 0.1.10 — 2026-06-06 |
| Failover automatique quorum (#4b) | 100% (code+tests) | ✅ superviseur Rust 0.1.13 — déploiement 3 machines à valider en live |

---

## Prochaine tâche immédiate

**Valider la gestion du parc (0.1.10) sur le PC primary** puis décider de la suite :
1. Réinstaller `Sovereign Data Agent_0.1.10_x64-setup.exe` (rebuild sidecar nœud inclus).
2. Suivre `docs/demo/gestion-parc-demo.md` (enrôlement → vente → dé-enrôlement → preuve #9 → récupération).

**Choix structurant restant** (un seul vrai manque §7.4) :
- **Failover automatique par quorum** (#4b) via Patroni + etcd sur les 3 nœuds — OU
  l'assumer comme arbitrage (manuel prouvé, automatique = étape Patroni identifiée) et
  passer à la couche **métier réelle** (le socle est prouvé).
