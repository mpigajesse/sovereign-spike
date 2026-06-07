# Démo — Gestion du parc (critères §7.4 : #8, #9, #10, #11)

**Sovereign Data Agent** — EIGSI × AL BARAA — Jesse MPIGA-ODOUMBA
Version 0.1.10 (2026-06-06)

Cette démo rend **vérifiables en live** les quatre critères d'acceptation qui n'étaient
jusqu'ici prouvés qu'au niveau des tests unitaires du cœur :

| Critère §7.4 | Démontré par |
|---|---|
| #8 Enrôlement sans exposer la clé | génération paire X25519 → sealed box → unwrap local |
| #9 Dé-enrôlé ne peut plus déchiffrer | rotation de DEK + test de déchiffrement sur blob réel |
| #10 Retrait → recalcul quorum + alerte | bannière « quorum insuffisant (< 3) » |
| #11 Perte de toutes les machines → récupération | code Argon2id setup/restore |

> **Où :** uniquement sur le **nœud actif (primary)** — c'est lui qui détient la DEK.
> Onglet **« Gestion du parc »** (icône ⊕).

---

## Architecture (rappel)

- **DEK opérationnelle mutable** : le nœud actif charge la DEK de génération la plus
  haute depuis la table `dek_state`. À chaque dé-enrôlement, il génère une **nouvelle
  génération**, la re-scelle pour les appareils restants, et l'utilise pour les écritures
  suivantes. Les anciens blobs restent lisibles avec leur génération d'origine.
- **Clé privée d'appareil** : générée et conservée **dans le backend Rust de l'app**
  (jamais exposée au JS ni au réseau). L'UI ne manipule que la clé publique (« le QR »).

Tables PostgreSQL (`migrations/20260606000003_device_management.sql`) :
`enrolled_devices`, `dek_state`, `recovery_blob`.

Endpoints (nœud actif) : `GET /devices`, `POST /devices/enroll`,
`POST /devices/revoke`, `POST /recovery/setup`, `POST /recovery/restore`.

---

## Scénario de démonstration (8 minutes)

### 1. État initial (#10)
- Ouvrir **Gestion du parc**. Observer : 0 appareil, génération 1.
- Bannière **« ⚠ Quorum insuffisant (0/3) »** → le failover automatique sûr exige ≥ 3 machines.

### 2. Enrôler deux appareils (#8)
- Saisir « Poste de Karim » → **+ Enrôler**. Puis « Poste de Salma » → **+ Enrôler**.
- Dans **Preuve #8**, chaque appareil affiche « ✓ DEK récupérée (gén. 1) » :
  → la clé a été emballée (sealed box) côté serveur et **ouverte localement** par la
  clé privée de l'appareil. À aucun moment elle n'a circulé en clair.

### 3. Produire un blob avec la DEK de génération 1
- Aller dans **Gestion du stock** → faire une **vente** (ex. PANTALON-L, 1).
- Cela écrit une entrée chiffrée dans le journal avec la **DEK génération 1**.

### 4. Dé-enrôler un appareil + rotation (#9, #10)
- Retour **Gestion du parc** → sur « Poste de Salma » : **Dé-enrôler + rotation**.
- Message : « DEK tournée → génération 2 ». La DEK opérationnelle change réellement.

### 5. Produire un blob avec la DEK de génération 2
- **Gestion du stock** → nouvelle **vente**. Ce blob est chiffré avec la **génération 2**.

### 6. Preuve du fencing cryptographique (#9)
- **Gestion du parc** → **Preuve #9** → « Tester le déchiffrement du dernier blob ».
- Résultat attendu sur le dernier blob (génération 2) :
  - « Poste de Karim (gén. 1) » → **✗ refusé (DEK périmée)** *(s'il a gardé la DEK v1)*
  - appareil ayant la DEK courante → **✓ réussi**
- **Lecture jury** : l'appareil retiré ne peut plus déchiffrer les écritures postérieures
  à la rotation. La révocation est cryptographique, pas une simple suppression de liste.

### 7. Code de récupération (#11)
- **Gestion du parc** → section « Code de récupération ».
- Saisir une passphrase (≥ 8 car.) → **Configurer** : la DEK courante est emballée sous
  cette passphrase (Argon2id, lent par construction).
- Cliquer **Restaurer (preuve)** : « ✓ DEK restaurée — identique à la courante ».
- **Lecture jury** : même si toutes les machines sont perdues, la passphrase seule (au
  coffre) reconstruit l'accès. Personne d'autre — pas l'éditeur — ne le peut.

---

## Points de vigilance honnêtes (à assumer en soutenance)

- **La rotation ne ré-chiffre PAS le journal existant.** Les anciens blobs gardent leur
  DEK d'origine (un dé-enrôlé garde l'accès à ce qu'il a déjà vu — inévitable). Le
  critère #9 porte sur les **écritures futures**, qui lui sont fermées. C'est le
  comportement correct et défendable.
- **Failover automatique par quorum (≥ 3 machines) non implémenté.** Le quorum est
  affiché et alerté (#10), mais l'élection automatique (type Patroni/Raft) reste une
  étape identifiée. Le failover **manuel** est, lui, prouvé en live (promotion standby).
- **Le coffre de clés privées d'appareil de l'UI est volatil** (vidé au redémarrage de
  l'app) : suffisant pour la démo, à persister chiffré en production.

---

## Tests automatisés associés

`cargo test -p sovereign-core` (44/44) — dont, pour ce module :
- `wrap_unwrap_dek_hex_roundtrip` (#8)
- `try_decrypt_hex_distingue_bonne_et_mauvaise_dek` (#9)
- `recovery_wrap_unwrap_roundtrip` + `recovery_mauvaise_passphrase_echoue` (#11)
- `critere_11_enrollment_desenrolement_rotation` (intégration registre)
