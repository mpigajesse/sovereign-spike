# Transition vers la couche métier — Cadrage

**Sovereign Data Agent** — EIGSI × AL BARAA — Jesse MPIGA-ODOUMBA
Date : 2026-06-06 — Version socle : 0.1.10

> Ce document fige où on en est, pourquoi on passe au métier maintenant, et ce qu'il
> faut décider avant d'écrire la première ligne de code métier. Il sert de base à la
> session de travail suivante.

---

## 1. Où on en est (le socle est prouvé)

La règle d'or du projet — **« socle d'abord, métier ensuite »** — est désormais
respectée. Le socle technique à haut risque est validé :

| Brique du socle | État |
|---|---|
| Chiffrement XChaCha20-Poly1305 + DEK (libsodium) | ✅ prouvé |
| Journal CBOR chiffré + hash chaîné SHA-256 | ✅ prouvé |
| Sérialisation des écritures (SERIALIZABLE, anti-survente) | ✅ prouvé live |
| Réplication PostgreSQL synchrone (primary → standby) | ✅ prouvé live |
| Failover **manuel** sans perte | ✅ prouvé live |
| Fencing anti-split-brain (retour ancien actif → 503) | ✅ prouvé live |
| Relais éditeur aveugle (zero-knowledge) | ✅ prouvé live |
| Gestion du parc : enrôlement / rotation DEK / récupération (#8–#11) | ✅ livré 0.1.10 |

**Seul manque assumé et reporté** (décision 2026-06-06) :
le **failover automatique par quorum (≥ 3 machines, type Patroni/etcd)**, critère §7.4 #4b.
Le failover **manuel** étant prouvé, ce report est un arbitrage explicite, pas une dette
cachée. On y reviendra après la couche métier.

---

## 2. Pourquoi on peut passer au métier maintenant

Le cadrage Phase 0 (§7.5) excluait délibérément toute logique métier réelle tant que le
socle n'était pas figé — précisément pour ne pas bâtir le métier sur des fondations
mouvantes et devoir tout réécrire. Ces fondations sont maintenant stables et prouvées.
Le métier réel (facturation, paie, comptabilité, gestion de commandes…) — qui est la
cible finale du produit (cadrage §3.2bis) — peut donc commencer.

---

## 3. Ce qui est DÉJÀ en place pour accueillir le métier

On ne part pas de zéro. Trois acquis structurants :

### 3.1 Le trait `BusinessStore` (architecture hexagonale — ports & adapters)
`crates/core/src/business_store.rs` définit une interface de stockage métier abstraite,
et `sqlite_store.rs` en fournit une implémentation SQLite. Le cœur métier ne dépend pas
du moteur de stockage → on peut faire évoluer le métier sans réécrire l'infrastructure.

### 3.2 Le journal d'opérations (`Operation` / `OpType` / `Payload`)
`crates/core/src/journal.rs` modélise une opération métier sérialisée, ordonnée et
rejouable. Aujourd'hui il ne connaît que deux types : `Sale` (vente) et `StockAdjust`
(ajustement de stock). **C'est le premier endroit qu'on étendra** : chaque nouvelle
opération métier (créer une facture, encaisser…) deviendra un nouveau `OpType`.

### 3.3 Le chemin d'écriture prouvé (nœud actif)
`crates/node/src/active.rs` exécute déjà le flux complet d'une écriture dans une
transaction SERIALIZABLE : fencing → vérification d'invariant → séquence → chiffrement →
écriture métier + journal → commit → push relais. Le métier réel réutilisera ce flux.

---

## 4. Le point d'attention architectural NON NÉGOCIABLE (décision §5bis)

> **Toute opération à invariant fort doit rester dans UNE SEULE transaction PostgreSQL,
> sur le nœud actif** — métier ET journal chiffré écrits ensemble, atomiquement.

Raison : la vérification de l'invariant (ex. « stock ≥ quantité », « numéro de facture
continu ») et l'écriture du journal sont **indissociables**. Les séparer dans deux bases
(SQLite métier + PostgreSQL journal) imposerait un commit en deux phases (2PC), complexe
et faillible — exactement ce que le projet a voulu éviter.

| Atomicité | Coût | Fiabilité |
|---|---|---|
| Intra-base (1 transaction, 1 base) | Gratuite | Infaillible (garantie moteur) |
| Inter-bases (2PC) | Élevé | Faillible |

**Conséquence pratique :** quand tu décriras tes invariants métier, on identifiera
lesquels exigent cette atomicité. Ceux-là restent sur PostgreSQL (nœud actif). SQLite
garde son rôle là où aucun arbitrage concurrent n'est en jeu : nœud passif (rejeu d'un
journal déjà ordonné).

---

## 5. Ce dont j'ai besoin de ta part (spécification métier)

Pour cadrer proprement avant tout code, décris (même en vrac) :

1. **Le domaine** : facturation ? paie ? comptabilité ? gestion de commandes complète ?
   autre ? (un seul pour commencer, c'est mieux)
2. **Les entités** : quels objets (factures, clients, produits, lignes de commande, écritures
   comptables…) et leurs champs principaux.
3. **Les invariants forts** : les règles qui ne doivent JAMAIS être violées
   (numérotation de factures continue et sans trou, stock ≥ 0, débit = crédit, totaux
   cohérents…). → ce sont elles qui justifient de passer par le nœud actif.
4. **Les opérations** : ce qu'un opérateur peut faire (créer une facture, valider une
   commande, encaisser un paiement, passer une écriture…).

---

## 6. Méthode de travail pour la couche métier (proposée)

Une fois ta spec reçue, **avant tout code**, je produis un plan validable ensemble :

1. **Modèle de domaine** : entités, relations, champs typés.
2. **Inventaire des invariants** et, pour chacun, où il vit (PostgreSQL actif vs SQLite).
3. **Extension du journal** : les nouveaux `OpType` / `Payload` à ajouter.
4. **Découpage en étapes** : chaque opération métier = un incrément testable (TDD :
   test d'invariant d'abord, implémentation ensuite).
5. **Impact sur l'UI** : quelles pages Tauri (au-delà de Stock/Journal/Sécurité/Parc).

On garde la discipline du projet : **un invariant = un test qui prouve qu'il tient sous
concurrence**, comme l'anti-survente l'a été pour le socle.

---

## 7. Rappel — versions & déploiement

- Version courante du bundle : **0.1.10** (gestion du parc incluse).
- Installeur : `target/release/bundle/nsis/Sovereign Data Agent_0.1.10_x64-setup.exe`.
- Réinstallation : `taskkill` des process (app + sidecars) AVANT, sinon `.exe` verrouillé.
- La page **Gestion du parc** (onglet ⊕) n'est active que sur le **nœud actif (primary)**.
- Démo associée : `docs/demo/gestion-parc-demo.md`.

---

*Document de transition — à compléter avec la spécification métier de Jesse.*
