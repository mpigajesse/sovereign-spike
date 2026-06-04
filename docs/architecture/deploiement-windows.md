# Guide de Déploiement — Environnement Windows uniquement

**Sovereign Data Agent — Phase de test Windows**  
Jesse MPIGA-ODOUMBA — EIGSI × AL BARAA CONSULTING — Promo 2026  
Date : 2026-06-04

---

## Architecture de test — 3 machines Windows

```
┌─────────────────────────────────────────────────────────────────┐
│              Réseau VMnet1 — 192.168.200.0/24                   │
│                                                                  │
│  ┌─────────────────────┐                                        │
│  │  Windows 11         │  ← PC PHYSIQUE (serveur PME)           │
│  │  192.168.200.1      │                                        │
│  │                     │                                        │
│  │  • PostgreSQL 18    │  ← base de données + réplication      │
│  │  • sovereign-node   │  ← embarqué dans le .exe              │
│  │  • Sovereign .exe   │  ← mode LOCAL (auto-détecté)          │
│  └─────────┬───────────┘                                        │
│            │                                                     │
│            │  HTTP :3000 (API souveraine)                       │
│            │                                                     │
│  ┌─────────┴───────────┐   ┌─────────────────────┐             │
│  │  Windows 11 VM      │   │  Windows 11 VM       │            │
│  │  192.168.200.x      │   │  192.168.200.x       │            │
│  │                     │   │                      │            │
│  │  • Sovereign .exe   │   │  • Sovereign .exe    │            │
│  │  • Mode CLIENT      │   │  • Mode CLIENT       │            │
│  │  (pas de PG)        │   │  (pas de PG)         │            │
│  └─────────────────────┘   └─────────────────────┘            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Rôle de chaque machine

### Machine 1 — Windows 11 PC physique (serveur souverain)

| Élément | Détail |
|---------|--------|
| **Rôle** | Nœud actif — source de vérité |
| **IP VMnet1** | `192.168.200.1` |
| **PostgreSQL 18** | ✅ Déjà installé — OBLIGATOIRE |
| **pgAdmin** | ❌ Optionnel (outil admin uniquement) |
| **Sovereign .exe** | ✅ Installe et lance |
| **Mode au démarrage** | `LOCAL` (PostgreSQL détecté → sovereign-node-active démarre automatiquement) |
| **Port exposé** | `:3000` (API HTTP) |

**Comportement du `.exe` :**
1. Détecte PostgreSQL sur `127.0.0.1:5432` ✅
2. Démarre `sovereign-node-active` automatiquement
3. Dashboard → nœud actif vert `✓ Local (primary)`

---

### Machine 2 — Windows 11 VM n°1 (poste opérateur)

| Élément | Détail |
|---------|--------|
| **Rôle** | Client — poste de travail opérateur |
| **IP VMnet1** | `192.168.200.x` |
| **PostgreSQL** | ❌ Pas nécessaire |
| **pgAdmin** | ❌ Pas nécessaire |
| **Sovereign .exe** | ✅ Installe et lance |
| **Mode au démarrage** | `CLIENT` (pas de PG → connexion distante) |

**Configuration (une seule fois) :**
- Lancer l'app → page **⚙ Configuration**
- Nœud ACTIF : `http://192.168.200.1:3000`
- Cliquer **Sauvegarder**
- Dashboard → nœud actif vert (si Win11 physique est démarré)

---

### Machine 3 — Windows 11 VM n°2 (poste opérateur)

Identique à la Machine 2.

| Élément | Détail |
|---------|--------|
| **Rôle** | Client — poste de travail opérateur |
| **PostgreSQL** | ❌ Pas nécessaire |
| **Sovereign .exe** | ✅ Installe et lance |
| **Mode au démarrage** | `CLIENT` |

---

## Ce que chaque machine doit avoir

| Prérequis | Win11 PC physique | Win11 VM n°1 | Win11 VM n°2 |
|-----------|:----------------:|:------------:|:------------:|
| Windows 11 | ✅ | ✅ | ✅ |
| PostgreSQL 18 | ✅ **Obligatoire** | ❌ | ❌ |
| pgAdmin 4 | ❌ Optionnel | ❌ | ❌ |
| Sovereign .exe | ✅ | ✅ | ✅ |
| Connexion VMnet1 | ✅ | ✅ | ✅ |

> **Résumé :** PostgreSQL est installé **une seule fois**, sur le serveur (Win11 physique). Les postes opérateurs (VMs) n'ont besoin que du fichier `.exe`.

---

## Procédure de déploiement

### Étape 1 — Sur Windows 11 PC physique (serveur)

```
1. PostgreSQL 18 déjà installé ✅
2. Copier Sovereign Data Agent_0.1.0_x64-setup.exe
3. Double-clic → installation (30 secondes)
4. Lancer "Sovereign Data Agent"
5. L'app détecte PostgreSQL → démarre automatiquement
6. Dashboard : Nœud Actif = VERT ✓
```

### Étape 2 — Sur chaque Windows 11 VM (postes opérateurs)

```
1. Copier Sovereign Data Agent_0.1.0_x64-setup.exe
   (depuis partage réseau ou clé USB)
2. Double-clic → installation
3. Lancer "Sovereign Data Agent"
4. L'app détecte l'absence de PostgreSQL → mode CLIENT
5. Aller dans ⚙ Configuration
6. Saisir : http://192.168.200.1:3000
7. Cliquer Sauvegarder
8. Dashboard : Nœud Actif = VERT ✓ (si serveur démarré)
```

---

## Vérification de la connectivité réseau

Depuis chaque VM, vérifier que Win11 physique est joignable :

```powershell
# Dans PowerShell sur les VMs
Test-NetConnection -ComputerName 192.168.200.1 -Port 3000

# Résultat attendu :
# TcpTestSucceeded : True
```

Si `False` → vérifier que :
- Win11 physique a le pare-feu ouvert sur le port 3000
- Les deux machines sont sur le même réseau VMnet1

---

## Ce que voit chaque opérateur

Toutes les machines voient **les mêmes données en temps réel** :
- Stock PANTALON-L = 89 sur Win11 physique → 89 sur les VMs
- Une vente faite depuis VM n°1 est immédiatement visible sur VM n°2
- Les données sont stockées sur Win11 physique (machine du gérant)

```
Opérateur VM n°1          Opérateur VM n°2
     │                          │
     │ POST /write (vente)       │
     └──────────────────────────┘
                  │
                  ▼
         Win11 physique
         sovereign-node-active
         PostgreSQL (source de vérité)
                  │
         ◄────────┘
     Réponse seq=N "committed"
```

---

## Fichier à distribuer

```
Sovereign Data Agent_0.1.0_x64-setup.exe
Taille : 3.8 MB
Contient : sovereign-node-active embarqué + interface graphique Tauri
```

**Localisation sur Win11 physique :**
```
D:\PFE-FINAL\pfe\sovereign-spike\target\release\bundle\nsis\
Sovereign Data Agent_0.1.0_x64-setup.exe
```

---

*Sovereign-Spike Phase 0 — Déploiement Windows — 2026-06-04*
