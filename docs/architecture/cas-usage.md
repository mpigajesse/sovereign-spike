# Cas d'usage — Sovereign Data Agent

**Principe directeur :** le moteur souverain (cœur) s'exécute en **démon** (service de fond).
La couche métier est un **client léger** : l'utilisateur effectue ses opérations sans jamais
voir ce que le démon fait derrière (chiffrement, journal, réplication, fencing).

Jesse MPIGA-ODOUMBA — EIGSI × AL BARAA CONSULTING — Promo 2026

---

## 1. Le principe : moteur = démon, métier = client léger

```
┌─────────────────────────────────────────────────────────────┐
│  COUCHE MÉTIER (ce que l'utilisateur voit)                   │
│                                                              │
│   "Vendre 3 PANTALON-L"   →   "✓ Vente enregistrée"         │
│   "Quel est le stock ?"   →   "Stock : 87"                   │
│                                                              │
│   Simple. Aucune notion de crypto, journal, réplication.    │
└────────────────────────┬─────────────────────────────────────┘
                         │  HTTP localhost (appels simples)
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  LE DÉMON SOUVERAIN (le cœur — ce qui est invisible)        │
│  Service Windows toujours actif, en arrière-plan             │
│                                                              │
│   À chaque opération, le démon fait AUTOMATIQUEMENT :        │
│   1. Vérifie l'invariant métier (anti-survente)             │
│   2. Sérialise l'écriture (ordre unique)                    │
│   3. Chiffre l'opération (XChaCha20-Poly1305)               │
│   4. Écrit dans le journal chiffré + le stock               │
│   5. Vérifie le fencing (époque)                            │
│   6. Réplique vers le standby (synchrone)                   │
│   7. Pousse le blob chiffré vers le relais éditeur          │
│                                                              │
│   L'utilisateur ne voit RIEN de tout ça. C'est le but.      │
└─────────────────────────────────────────────────────────────┘
```

> **Pourquoi c'est important :** une PME sans informaticien ne doit jamais
> manipuler la cryptographie ni la réplication. Le démon rend la souveraineté
> **automatique et invisible**. L'opérateur fait son métier ; le moteur protège
> ses données sans lui demander quoi que ce soit.

---

## 2. Le moteur comme démon (service)

| Aspect | Détail |
| :--- | :--- |
| **Forme** | Service Windows (`SovereignDataAgent`), démarrage automatique au boot |
| **Cycle de vie** | Démarre avec la machine, tourne en continu, redémarre seul si crash |
| **Interface** | API HTTP locale (`127.0.0.1:3000`) — le client métier s'y connecte |
| **Invisibilité** | Aucune fenêtre, aucune interaction — il travaille en silence |
| **Installation** | `00_install_all.ps1` crée le service ; ou l'app le démarre en sidecar |

Le client métier (app Tauri, ou tout autre logiciel) ne fait que des appels simples :
`POST /write`, `GET /stock/{id}`. Il ignore tout du chiffrement et de la réplication.

---

## 3. Acteurs

| Acteur | Rôle | Humain ? |
| :--- | :--- | :--- |
| **Opérateur** | Employé de la PME : fait des ventes, consulte le stock | ✅ |
| **Gérant** | Responsable PME : enrôle/retire des machines, gère la récupération | ✅ |
| **Démon souverain** | Le moteur : sérialise, chiffre, réplique, protège | ❌ (système) |
| **Relais éditeur** | Service externe aveugle : stocke du chiffré | ❌ (système) |

---

## 4. Cas d'usage MÉTIER (simples — ce que l'utilisateur fait)

### UC-01 — Enregistrer une vente
- **Acteur :** Opérateur
- **Geste :** saisit "vendre 3 PANTALON-L", clique "Valider"
- **Ce qu'il voit :** "✓ Vente enregistrée, stock restant : 87"
- **Ce que le démon fait (invisible) :** vérifie stock ≥ 3, sérialise, chiffre l'opération
  en CBOR, écrit le journal chiffré + décrémente le stock dans une transaction atomique,
  réplique vers le standby, pousse le blob au relais.
- **Garantie :** jamais de survente, même si deux opérateurs vendent en même temps.

### UC-02 — Réapprovisionner le stock
- **Acteur :** Opérateur / Gérant
- **Geste :** "ajouter 100 CHEMISE-M"
- **Ce qu'il voit :** "✓ Stock ajusté : 100"
- **Démon (invisible) :** même pipeline (chiffrement + journal + réplication).

### UC-03 — Consulter le stock
- **Acteur :** Opérateur
- **Geste :** cherche un article
- **Ce qu'il voit :** la quantité disponible
- **Démon :** lecture directe (déchiffrement transparent si nécessaire).

### UC-04 — Consulter l'historique
- **Acteur :** Gérant
- **Geste :** ouvre le journal des opérations
- **Ce qu'il voit :** la liste des ventes/ajustements (en clair, car sur sa machine)
- **Démon :** le journal est chiffré au repos ; il le déchiffre pour l'affichage local.

### UC-05 — Travailler hors-ligne (lecture)
- **Acteur :** Opérateur sur un poste coupé du serveur
- **Geste :** consulte le stock
- **Ce qu'il voit :** les données (réplique locale) + un indicateur "hors-ligne, lecture seule"
- **Démon :** sert la réplique locale ; refuse toute écriture tant que le serveur est injoignable.

---

## 5. Cas d'usage MOTEUR (le cœur — la vraie valeur, invisible)

### UC-06 — Sérialiser et chiffrer chaque écriture
- **Acteur :** Démon (déclenché par toute écriture)
- **Automatique :** ordre unique (seq), CBOR, XChaCha20-Poly1305, hash chaîné SHA-256.
- **Invariant :** le journal au repos ne contient jamais de clair.

### UC-07 — Répliquer vers le standby (synchrone)
- **Acteur :** Démon actif → démon standby
- **Automatique :** réplication WAL PostgreSQL ; confirmation à l'opérateur seulement
  après réplication (pour les invariants forts).

### UC-08 — Sauvegarder vers le relais éditeur (zéro-knowledge)
- **Acteur :** Démon actif → relais éditeur
- **Automatique :** push best-effort des blobs chiffrés ; le relais ne voit que du chiffré.

### UC-09 — Basculer en cas de panne (failover)
- **Acteur :** Gérant (manuel, 2 machines) ou Démon (auto, ≥ 3 machines)
- **Geste :** le serveur tombe → un standby est promu nouveau serveur
- **Garantie :** aucune perte de données (réplication synchrone).

### UC-10 — Bloquer l'ancien serveur revenu (fencing)
- **Acteur :** Démon
- **Automatique :** l'époque a changé → l'ancien actif détecte le mismatch et se neutralise
  (HTTP 503). Empêche le split-brain.

---

## 6. Cas d'usage ADMINISTRATION (Gérant)

### UC-11 — Enrôler une nouvelle machine
- **Acteur :** Gérant
- **Geste :** sur le nouveau poste, scanne un QR depuis un poste déjà autorisé
- **Démon :** la DEK est emballée (sealed box X25519) pour la nouvelle machine ;
  elle rejoint le cluster. Le relais ne voit jamais la clé.

### UC-12 — Retirer une machine (poste volé / employé parti)
- **Acteur :** Gérant
- **Geste :** "révoquer le poste de Karim"
- **Démon :** rotation de la DEK + re-emballage pour les seules machines restantes ;
  recalcul du quorum (alerte si failover auto n'est plus garanti).
- **Garantie :** la machine retirée ne peut plus déchiffrer les nouvelles données.

### UC-13 — Récupérer l'accès (perte de toutes les machines)
- **Acteur :** Gérant
- **Geste :** saisit le code de récupération (imprimé une fois, mis au coffre)
- **Démon :** dérive la clé (Argon2id), restaure l'accès aux données chiffrées.

---

## 7. Diagramme de cas d'usage (textuel)

```
                    ┌──────────────────────────────────────┐
                    │       SOVEREIGN DATA AGENT           │
                    │                                      │
   ┌──────────┐     │  MÉTIER (simple)                    │
   │Opérateur │─────┼─→ UC-01 Enregistrer une vente       │
   │          │─────┼─→ UC-02 Réapprovisionner            │
   │          │─────┼─→ UC-03 Consulter le stock          │
   │          │─────┼─→ UC-05 Travailler hors-ligne       │
   └──────────┘     │                                      │
                    │                                      │
   ┌──────────┐     │  ADMIN                              │
   │ Gérant   │─────┼─→ UC-04 Consulter l'historique      │
   │          │─────┼─→ UC-11 Enrôler une machine         │
   │          │─────┼─→ UC-12 Retirer une machine         │
   │          │─────┼─→ UC-13 Récupérer l'accès           │
   │          │─────┼─→ UC-09 Déclencher un failover      │
   └──────────┘     │                                      │
                    │  MOTEUR (démon — invisible)         │
   ┌──────────┐     │  · UC-06 Sérialiser + chiffrer      │
   │  Démon   │◇────┼─→ UC-07 Répliquer (synchrone)       │
   │souverain │◇────┼─→ UC-08 Sauvegarder au relais       │
   │          │◇────┼─→ UC-10 Fencing anti-split-brain    │
   └──────────┘     │                                      │
                    │  «include» : chaque UC métier        │
                    │  déclenche UC-06/07/08 du moteur     │
   ┌──────────┐     │                                      │
   │  Relais  │◇────┼─ reçoit UC-08 (blobs chiffrés)      │
   │ éditeur  │     │  ne voit jamais le clair             │
   └──────────┘     └──────────────────────────────────────┘
```

> **Relation clé :** chaque cas d'usage métier (UC-01, UC-02…) **inclut**
> automatiquement les cas d'usage moteur (UC-06 chiffrement, UC-07 réplication,
> UC-08 sauvegarde). L'utilisateur déclenche tout cela sans le savoir.

---

## 8. Ce que ça prouve pour la soutenance

1. **La souveraineté est automatique.** L'opérateur ne fait que son métier ;
   le démon protège les données en arrière-plan. Aucune compétence technique requise.
2. **Le moteur est le vrai produit.** La couche métier (vente/stock) est volontairement
   minimale — c'est une démonstration. La valeur est dans le démon souverain.
3. **Séparation nette des responsabilités.** Métier = quoi faire ; Moteur = comment le
   faire de façon souveraine. Les deux communiquent par une API simple.

---

*Document cas d'usage — Sovereign-Spike — EIGSI × AL BARAA CONSULTING — 2026*
