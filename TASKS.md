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
- ⬜ Endpoint `/enroll` HTTP (enrôlement via API REST — architecture en place)
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
- ✅ Assistant installation : choix du rôle (Solo/Primary/Standby/Client)

### 3.2bis Mode PME Solo (SQLite autonome) ✅ LIVRÉ 2026-06-04

> Cible TPE mono-poste : tout-en-un sur SQLite, **zéro PostgreSQL**.

- ✅ Binaire `sovereign-node-solo` — `crates/node/src/bin/solo_node.rs`
  - ✅ Même API HTTP que l'actif (`/write`, `/stock`, `/journal`, `/epoch`)
  - ✅ Métier + journal chiffré dans UNE base SQLite (atomicité préservée)
  - ✅ Anti-survente, idempotence, journal chiffré, WAL + busy_timeout
- ✅ Testé E2E : 7/7 (santé, stock, anti-survente, idempotence, journal, époque)
- ✅ Persistance vérifiée (données conservées au redémarrage)
- ✅ Embarqué dans le .exe + rôle "PME Solo" dans l'assistant
- ✅ Redémarrage auto du nœud solo au relancement de l'app

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

## Résumé de progression

| Phase | Avancement | Statut |
|-------|-----------|--------|
| Phase 0 — Spike dérisquage | 100% | ✅ Validé 2026-06-04 |
| Phase 1 — Cœur Rust production | 98% | ✅ Validé 2026-06-04 (+ séparation SQLite/PG) |
| Phase 2 — Relais + multi-sites | 85% | ✅ Validé 2026-06-04 |
| Phase 3 — Installeur one-click | 70% | ✅ Script PowerShell créé |
| Phase 3 — Frontend Tauri | 85% | ✅ App complète, build OK, packaging à finaliser |
| Phase 3 — Mobile UniFFI | 0% | ⬜ Architecture définie, hors-scope PFE immédiat |

---

## Prochaine tâche immédiate

**Phase 1.3 — Bascule manuelle + fencing :**  
Tester la promotion du standby Ubuntu en primary, incrémenter l'époque, et vérifier que l'ancien actif Windows est bloqué à l'époque inférieure.

```bash
# Sur Ubuntu — promouvoir le standby
sudo -u postgres pg_ctl promote -D /var/lib/postgresql/18/main
```

```powershell
# Sur Windows — vérifier que l'actif est fencé
Invoke-RestMethod http://192.168.200.1:3000/write -Method POST `
    -ContentType "application/json" `
    -Body '{"op_type":"sale","item_id":"TEST","quantity":1}'
# Attendu : 409 ou 503 (époque périmée)
```
