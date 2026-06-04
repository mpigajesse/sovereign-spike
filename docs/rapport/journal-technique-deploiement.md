# Journal Technique de Déploiement — SDA Prototype
## Certification mTLS, Accès Navigateur et Synchronisation P2P

**Projet :** Sovereign Data Agent (SDA) — PFE EIGSI 2025-2026  
**Auteur :** Jesse MPIGA-ODOUMBA  
**Période :** 26–27 mai 2026  
**Nœuds impliqués :** Win11 (`192.168.200.1`), Ubuntu 26.04 (`192.168.200.130`), Kali (`192.168.200.128`)

---

## Table des matières

1. [Contexte et objectifs](#1-contexte-et-objectifs)
2. [Génération des certificats TLS avec SAN](#2-génération-des-certificats-tls-avec-san)
3. [Import du certificat client dans les navigateurs](#3-import-du-certificat-client-dans-les-navigateurs)
4. [Correction nginx — template envsubst](#4-correction-nginx--template-envsubst)
5. [Correction du proxy Syncthing API](#5-correction-du-proxy-syncthing-api)
6. [Configuration de l'authentification Syncthing GUI](#6-configuration-de-lauthentification-syncthing-gui)
7. [Connexion directe LAN entre nœuds](#7-connexion-directe-lan-entre-nœuds)
8. [Résolution du conflit de synchronisation](#8-résolution-du-conflit-de-synchronisation)
9. [Déploiement Node 2 — VM Ubuntu 26.04 LTS](#9-déploiement-node-2--vm-ubuntu-2604-lts)
10. [État final validé — 3 nœuds](#10-état-final-validé--3-nœuds)
11. [Index des captures d'écran](#11-index-des-captures-décran)

---

## 1. Contexte et objectifs

À l'issue de la phase d'implémentation du prototype SDA, les objectifs de cette session étaient :

- **Sécuriser l'accès HTTPS** au frontend via mTLS (authentification mutuelle x509)
- **Valider la synchronisation P2P** entre le nœud Win11 et le nœud Kali Linux
- **Corriger les erreurs de certificat** bloquant l'accès depuis Chrome (Win11) et Firefox (Kali)
- **Activer le proxy Syncthing** dans le dashboard frontend
- **Documenter** chaque erreur et correction pour le rapport final

### Architecture impliquée

```
[Chrome/Win11]  ──mTLS──► [Nginx:443] ──HTTP──► [FastAPI:8000]
                                         └──────► [Syncthing:8384]
[Firefox/Kali]  ──mTLS──► [Nginx:443] ──HTTP──► [FastAPI:8000]
                                         └──────► [Syncthing:8384]
[Firefox/Ubuntu]──mTLS──► [Nginx:443] ──HTTP──► [FastAPI:8000]
                                         └──────► [Syncthing:8384]

[Syncthing/Win11] ◄──TCP:22000──► [Syncthing/Kali]
[Syncthing/Win11] ◄──TCP:22000──► [Syncthing/Ubuntu]  ← Node 2 ajouté
[Syncthing/Kali]  ◄──TCP:22000──► [Syncthing/Ubuntu]
   SDA_Shared synchronisé en maillage P2P entre les 3 nœuds
```

| Nœud | OS | IP LAN (VMnet1) | Rôle |
|------|----|-----------------|------|
| Node 1 | Windows 11 (PC physique) | `192.168.200.1` | Nœud principal |
| Node 2 | Ubuntu 26.04 LTS "resolute" (VM) | `192.168.200.130` | Nœud réplication |
| Node 3 | Kali Linux (VM) | `192.168.200.128` | Nœud réplication / sécurité |

---

## 2. Génération des certificats TLS avec SAN

### 2.1 Problème initial — Absence de SAN

**Symptôme :** Chrome affiche `ERR_CERT_COMMON_NAME_INVALID` à la connexion HTTPS.

**Cause technique :**  
Depuis Chrome 58 (2017), les navigateurs modernes **ignorent le champ CN** (Common Name) pour la validation du nom d'hôte. Seule l'extension **SAN** (Subject Alternative Name) est reconnue. Les anciens certificats générés avec uniquement `CN=sda-node` étaient rejetés.

**Capture :** `[SCREENSHOT: chrome_err_cert_common_name_invalid.png]`

---

### 2.2 Problème secondaire — Conversion de chemins Git Bash (Windows)

**Symptôme :** La commande `openssl req -subj "/C=MA/..."` échoue avec une erreur de chemin sous Git Bash sur Windows.

**Cause technique :**  
Git Bash (MSYS2) convertit automatiquement les arguments commençant par `/` en chemins Windows. Ainsi `/C=MA` devient `C:\MA` ce qui invalide le DN OpenSSL.

**Erreur rencontrée :**
```
error in reading certificate file /C:/MA...
```

**Fix appliqué :**
```bash
export MSYS_NO_PATHCONV=1
```
Placé au début du script `scripts/generate-certs.sh`, cette variable désactive la conversion automatique des chemins par MSYS2.

---

### 2.3 Problème — Chemin `/tmp/san.ext` non trouvable

**Symptôme :** Le fichier d'extension SAN créé dans `/tmp/` n'est pas trouvé par la commande `openssl x509 -extfile`.

**Cause technique :**  
Sous Git Bash sur Windows, `/tmp/` est mappé différemment selon la configuration MSYS2. Le chemin est transformé ou inaccessible depuis OpenSSL natif Windows.

**Fix appliqué :**  
Utilisation d'un chemin relatif dans le répertoire courant :
```bash
echo "subjectAltName=DNS:localhost,DNS:sda-node,IP:127.0.0.1,IP:192.168.200.1,IP:192.168.200.128,IP:192.168.200.100" > san.ext
# ... utilisation ...
rm -f san.ext
```

---

### 2.4 Script final — `scripts/generate-certs.sh`

```bash
#!/usr/bin/env bash
export MSYS_NO_PATHCONV=1

CERTS_DIR="config/nginx/certs"
mkdir -p "$CERTS_DIR"

# 1/3 — CA interne SDA
openssl req -x509 -newkey rsa:4096 -days 3650 -nodes \
    -keyout "$CERTS_DIR/ca.key" -out "$CERTS_DIR/ca.crt" \
    -subj "/C=MA/O=SDA-POC/CN=SDA-Internal-CA"

# 2/3 — Certificat serveur avec SAN
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERTS_DIR/server.key" -out "$CERTS_DIR/server.csr" \
    -subj "/C=MA/O=SDA-POC/CN=sda-node"

echo "subjectAltName=DNS:localhost,DNS:sda-node,IP:127.0.0.1,IP:192.168.200.1,IP:192.168.200.128,IP:192.168.200.100" > san.ext

openssl x509 -req -days 365 -in "$CERTS_DIR/server.csr" \
    -CA "$CERTS_DIR/ca.crt" -CAkey "$CERTS_DIR/ca.key" -CAcreateserial \
    -extfile san.ext -out "$CERTS_DIR/server.crt"
rm -f san.ext

# 3/3 — Certificat client + export PKCS#12 pour navigateur
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERTS_DIR/client.key" -out "$CERTS_DIR/client.csr" \
    -subj "/C=MA/O=SDA-POC/CN=sda-client-node-1"

openssl x509 -req -days 365 -in "$CERTS_DIR/client.csr" \
    -CA "$CERTS_DIR/ca.crt" -CAkey "$CERTS_DIR/ca.key" -CAcreateserial \
    -out "$CERTS_DIR/client.crt"

# Export .p12 pour import navigateur (mot de passe : sda2026)
openssl pkcs12 -export -out "$CERTS_DIR/sda-client.p12" \
    -inkey "$CERTS_DIR/client.key" -in "$CERTS_DIR/client.crt" \
    -certfile "$CERTS_DIR/ca.crt" -passout pass:sda2026

rm -f "$CERTS_DIR"/*.csr "$CERTS_DIR"/*.srl
```

**Vérification SAN généré :**
```bash
openssl x509 -in config/nginx/certs/server.crt -noout -text | grep -A 3 "Subject Alternative"
# Résultat attendu :
# X509v3 Subject Alternative Name:
#     DNS:localhost, DNS:sda-node, IP Address:127.0.0.1,
#     IP Address:192.168.200.1, IP Address:192.168.200.128
```

**Capture :** `[SCREENSHOT: san_verification_output.png]`

---

## 3. Import du certificat client dans les navigateurs

### 3.1 Windows 11 — Chrome

Le navigateur Chrome sur Windows utilise le **Windows Certificate Store** (magasin de certificats Windows).

**Étape 1 — Import de la CA dans les autorités de confiance :**

```powershell
# PowerShell échoue avec cette erreur sur certains systèmes :
# "L'interface utilisateur n'est pas autorisée"
Import-Certificate -FilePath ".\config\nginx\certs\ca.crt" `
    -CertStoreLocation Cert:\LocalMachine\Root
# → ERREUR : Lecteur introuvable 'Cert'
```

**Fix — Utiliser `certutil` (CLI) :**
```cmd
certutil -addstore -user "Root" config\nginx\certs\ca.crt
```
> `certutil` contourne les restrictions UAC sur le magasin LocalMachine et fonctionne dans le contexte utilisateur.

**Capture :** `[SCREENSHOT: certutil_ca_import_success.png]`

**Étape 2 — Import du certificat client P12 :**
```powershell
Import-PfxCertificate -FilePath ".\config\nginx\certs\sda-client.p12" `
    -CertStoreLocation Cert:\CurrentUser\My `
    -Password (ConvertTo-SecureString "sda2026" -AsPlainText -Force)
```

**Étape 3 — Navigation HTTPS :**
```
https://localhost
```
Chrome affiche la boîte de sélection du certificat client → sélectionner `sda-client-node-1`.

**Capture :** `[SCREENSHOT: chrome_cert_selection_dialog.png]`  
**Capture :** `[SCREENSHOT: win11_frontend_dashboard_operational.png]`

---

### 3.2 Kali Linux — Firefox

Firefox utilise sa **propre base NSS**, indépendante du système.

**Procédure :**
1. Firefox → `about:preferences#privacy`
2. Section **Certificats** → **Afficher les certificats**
3. Onglet **Autorités** → **Importer** → `config/nginx/certs/ca.crt` → cocher "Faire confiance pour identifier les sites web"
4. Onglet **Vos certificats** → **Importer** → `config/nginx/certs/sda-client.p12` → mot de passe `sda2026`
5. Naviguer vers `https://192.168.200.1`

**Symptôme rencontré :** `SSL_ERROR_BAD_CERT_DOMAIN`

**Cause :** Anciens certificats sans SAN encore servis par nginx (conteneur pas rechargé après régénération).

**Fix :**
```bash
# Sur Kali (depuis ~/PFE/sda-prototype/)
git pull
bash scripts/generate-certs.sh
docker compose up -d --force-recreate nginx
```

**Capture :** `[SCREENSHOT: firefox_kali_cert_import.png]`  
**Capture :** `[SCREENSHOT: kali_frontend_dashboard_operational.png]`

---

### 3.3 Tableau récapitulatif des erreurs de certificat

| Erreur | Navigateur | Cause | Solution |
|--------|-----------|-------|----------|
| `ERR_CERT_AUTHORITY_INVALID` | Chrome | CA non dans le magasin de confiance | `certutil -addstore -user "Root" ca.crt` |
| `ERR_CERT_COMMON_NAME_INVALID` | Chrome | Certificat sans SAN | Régénérer avec extension SAN |
| `SSL_ERROR_BAD_CERT_DOMAIN` | Firefox | Même cause (sans SAN) | Régénérer + `force-recreate nginx` |
| `ERR_SSL_PROTOCOL_ERROR` | Chrome | nginx non rechargé / cert périmé | `docker compose up -d --force-recreate nginx` |

---

## 4. Correction nginx — template envsubst

### 4.1 Problème initial — `worker_processes directive is not allowed here`

**Symptôme :** Le conteneur nginx refuse de démarrer avec l'erreur :
```
nginx: [emerg] "worker_processes" directive is not allowed here
```

**Cause technique :**  
L'image officielle `nginx:1.25-alpine` dispose du mécanisme de **templates envsubst** : les fichiers placés dans `/etc/nginx/templates/` sont traités par `envsubst` au démarrage et copiés dans `/etc/nginx/conf.d/`. Or `conf.d/` est inclus **à l'intérieur** du bloc `http {}` du `nginx.conf` principal. Un fichier de template contenant les directives globales (`worker_processes`, `events {}`, `http {}`) crée une imbrication invalide.

**Fix (premier niveau) :**  
Transformer le fichier en template contenant uniquement des blocs `server {}`, renommé `nginx.conf.template`.

---

### 4.2 Problème secondaire — `invalid variable name` / `invalid number of arguments` sur Alpine

**Symptôme (Kali, ligne 76) :**
```
nginx: [emerg] invalid variable name in "/etc/nginx/conf.d/default.conf"
```

**Symptôme (Ubuntu, ligne 59) :**
```
nginx: [emerg] invalid number of arguments in "proxy_set_header" directive
```

**Cause technique :**  
`envsubst` sur Alpine Linux (GNU gettext) **ne distingue pas** les variables nginx (`$host`, `$remote_addr`, `$proxy_add_x_forwarded_for`, `$scheme`, `$ssl_client_s_dn`) des variables shell. Il les remplace toutes par une chaîne vide ou les détruit. Résultat : le fichier généré dans `conf.d/` contient des directives syntaxiquement invalides :

```nginx
# AVANT envsubst (config source correcte)
proxy_set_header Host $host;

# APRÈS envsubst sur Alpine (corrompu)
proxy_set_header Host ;    # ← valeur vide → erreur nginx
```

De plus, la variable `${SYNCTHING_API_KEY}` était vide dans l'environnement Docker → `proxy_set_header X-API-Key ;` → erreur d'arguments.

**Tentatives de contournement échouées :**
- `$$host` (double dollar) → syntaxe non supportée sur Alpine gettext
- `\$host` → idem, ignoré par Alpine envsubst

**Solution finale — Contournement total d'envsubst :**  
Monter le fichier de config **directement** dans `/etc/nginx/conf.d/` (court-circuite le mécanisme template) :

```yaml
# docker-compose.yml — AVANT (mécanisme template actif)
volumes:
  - ./config/nginx/nginx.conf.template:/etc/nginx/templates/default.conf.template:ro

# docker-compose.yml — APRÈS (conf.d direct, envsubst ignoré)
volumes:
  - ./config/nginx/nginx.conf.template:/etc/nginx/conf.d/default.conf:ro
```

Les variables nginx (`$host`, `$remote_addr`, etc.) sont interprétées nativement par nginx à l'exécution — sans passer par envsubst. Ce mécanisme est la façon standard d'écrire des configs nginx et ne requiert aucun prétraitement.

**Effet de bord résolu simultanément :** La variable `SYNCTHING_API_KEY` est également supprimée de `docker-compose.yml` (plus besoin d'envsubst), mais l'injection de la clé Syncthing doit être gérée autrement (voir §5.3).

---

## 5. Correction du proxy Syncthing API

### 5.1 Problème — Double préfixe `/rest/rest/`

**Symptôme :** Le dashboard frontend affiche "Impossible de joindre Syncthing. Vérifiez que le conteneur est démarré." alors que Syncthing fonctionne.

**Analyse — logs nginx :**
```
GET /syncthing-api/rest/system/status → 404
```

**Cause technique :**  
La règle nginx était :
```nginx
location /syncthing-api/ {
    proxy_pass http://syncthing:8384/rest/;   # ← BUG
}
```
Comportement nginx avec `proxy_pass` contenant un path :
- nginx supprime le préfixe `location` (`/syncthing-api/`) de l'URI
- puis **concatène** le reste de l'URI au path du `proxy_pass`

Résultat : `/syncthing-api/rest/system/status` → strip `/syncthing-api/` → `rest/system/status` → concaténé à `/rest/` → `/rest/rest/system/status` → **404**

**Fix :**
```nginx
location /syncthing-api/ {
    proxy_pass http://syncthing:8384/;   # ← CORRECT : proxy_pass vers /
}
```
Résultat : `/syncthing-api/rest/system/status` → strip → `rest/system/status` → `/rest/system/status` → **200 OK**

**Capture :** `[SCREENSHOT: frontend_syncthing_impossible_joindre.png]`  
**Capture :** `[SCREENSHOT: frontend_syncthing_ok_apres_fix.png]`

---

### 5.2 Perte du header X-API-Key après fix envsubst

**Problème :** Après le fix §4.2 (suppression de `SYNCTHING_API_KEY` dans docker-compose), le dashboard affichait à nouveau "Impossible de joindre Syncthing" sur tous les nœuds.

**Cause :** En supprimant `envsubst`, le header `proxy_set_header X-API-Key "${SYNCTHING_API_KEY}";` a été retiré du bloc nginx. L'API Syncthing retourne **HTTP 403** pour toute requête sans ce header.

---

### 5.3 Solution — Injection de clé par nœud via `include` nginx

**Contrainte :** Chaque nœud Syncthing génère sa propre clé aléatoire au premier démarrage, stockée dans `/var/syncthing/config/config.xml`. Cette clé ne peut pas être versionnée (différente par nœud) ni injectée via `docker-compose.yml` (supprimé pour contourner envsubst).

**Architecture de la solution :**

```
config/nginx/certs/syncthing-key.conf   ← fichier local, non commité avec la vraie clé
scripts/setup-syncthing-key.sh          ← script d'injection, commité
nginx.conf.template (location /syncthing-api/)  ← include syncthing-key.conf
```

**Fichier nginx (extrait) :**
```nginx
location /syncthing-api/ {
    proxy_pass         http://syncthing:8384/;
    proxy_set_header   Host              $host;
    proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header   X-Requested-With  XMLHttpRequest;
    include            /etc/nginx/certs/syncthing-key.conf;  # ← injecté par nœud
}
```

**Script `scripts/setup-syncthing-key.sh` :**
```bash
API_KEY=$(docker exec sda-syncthing \
    sh -c 'grep -o "<apikey>[^<]*</apikey>" /var/syncthing/config/config.xml \
           | sed "s/<[^>]*>//g"')

cat > config/nginx/certs/syncthing-key.conf <<EOF
proxy_set_header X-API-Key "$API_KEY";
EOF

docker exec sda-nginx nginx -s reload   # rechargement à chaud, sans downtime TLS
```

**Exécution sur chaque nœud :**
```bash
bash scripts/setup-syncthing-key.sh
```

**Clés API par nœud (extraites en session) :**

| Nœud | Clé API Syncthing |
|------|-------------------|
| Win11 (GHIJH3G) | `FhrJ56rUqDmMejwqSYh5mRsntGMCWSqQ` |
| Ubuntu 26.04 (Node 2) | *(extraite localement via le script)* |
| Kali (VFTEXUZ) | `oAWCzUtHeKSjk9nogrtNnYCNonGisyjd` |

> **Règle de sécurité :** La clé API est node-specific. `syncthing-key.conf` est versionné vide (commentaires) ; la version avec la vraie clé reste locale sur chaque nœud et ne doit jamais être commitée.

---

## 6. Configuration de l'authentification Syncthing GUI

### 6.1 Comptes créés par nœud

Chaque nœud dispose d'un compte administrateur distinct pour traçabilité :

| Nœud | Identifiant Syncthing |
|------|----------------------|
| Win11 | `sda-admin-Win11` |
| Kali | `sda-admin-kali` |
| Ubuntu (à déployer) | `sda-admin-ubuntu` |

### 6.2 Mot de passe chiffré Syncthing (dossier partagé)

Le champ "Mot de passe pour chiffrer" dans l'onglet **Liaisons** du dossier SDA_Shared est destiné aux nœuds **non-fiables** (Untrusted Devices). Pour des nœuds de confiance du réseau SDA, ce champ **doit rester vide** — remplir ce champ empêcherait la synchronisation lisible des fichiers Parquet.

**Capture :** `[SCREENSHOT: syncthing_folder_liaisons_tab.png]`

---

## 7. Connexion directe LAN entre nœuds

### 7.1 Problème — Connexion via Relay externe

**Symptôme initial :** Le dashboard Win11 montrait la connexion Kali via `Relay` (serveur tiers sur internet) au lieu de la liaison LAN directe.

**Impact :** Latence accrue, dépendance à la connectivité internet, non-conforme au principe offline-first du SDA.

**Capture :** `[SCREENSHOT: syncthing_relay_connection.png]`

### 7.2 Fix — Adresses statiques LAN

**Sur Win11** (Syncthing GUI `http://localhost:8384`) :  
Éditer le pair Kali (`ec92ab7f9f4a`) → Liaisons :
```
tcp://192.168.200.128:22000
```

**Sur Kali** (Syncthing GUI `http://localhost:8384`) :  
Éditer le pair Win11 (`c217546e8f07`) → Liaisons :
```
tcp://192.168.200.1:22000
```

**Résultat :** Connexion directe établie `TCP` sur le réseau local.

**Capture :** `[SCREENSHOT: syncthing_win11_peer_lan_config.png]`  
**Capture :** `[SCREENSHOT: syncthing_kali_peer_lan_config.png]`  
**Capture :** `[SCREENSHOT: syncthing_direct_tcp_connection.png]`

---

## 8. Résolution du conflit de synchronisation

### 8.1 Contexte du conflit — Couplage Ubuntu ↔ Win11

Le conflit est apparu lors du premier couplage Syncthing entre Ubuntu (Node 2) et Win11 (Node 1), après que les 3 nœuds ont été reliés en maillage complet.

**Symptôme :** Dashboard Win11 affiche `⚠ 1 conflit` — `93% — 1 fichier en attente de synchronisation`. Les nœuds Kali et Ubuntu affichent `100%`.

**Identification — Vue détaillée Syncthing (GUI Win11, dossier SDA_Shared) :**

```
Éléments non synchronisés    1 élément(s), ~0 B
Éléments en échec            1 élément(s)
Dernier changement           .sync-conflict-20260526-153437-GHIJ
```

**Identification côté Ubuntu :**

```
Éléments non synchronisés — c217546e8f07 (Win11)
  .gitkeep     0 B     2026-05-26 23:58:48     0cf005eea6cb
```

**Fichier en conflit identifié :** `.gitkeep`

### 8.2 Cause technique — Fichier placeholder git dans le volume Syncthing

`.gitkeep` est un fichier vide (0 octet) conventionnellement placé dans les répertoires vides pour que git puisse les versionner (git ne tracke pas les dossiers vides). Il avait été ajouté dans `data/shared_storage/` pour préserver ce répertoire dans le dépôt.

**Problème :** Ce fichier s'est retrouvé dans le **volume Syncthing** (`/var/syncthing/SDA_Shared/`), qui est distinct du répertoire git. Quand les nœuds ont été couplés :
- Win11 avait `.gitkeep` avec son propre timestamp (`2026-05-26 15:36` UTC)
- Ubuntu avait aussi `.gitkeep` créé à un moment différent (`2026-05-26 23:58`)
- Les **vecteurs d'horloge** Syncthing divergeaient → Syncthing ne pouvait pas choisir un gagnant → conflit

**Confirmation par `ls -la` dans le conteneur Syncthing Win11 :**
```
-rwxrwxrwx 1 root root 0 May 26 15:36 .gitkeep     ← fichier parasite
drwxr-xr-x 1 1000 1000 4096 May 26 08:20 .stfolder
```

> **Note diagnostic :** Les commandes `find ... -name ".sync-conflict*"` et `find ... -name "*.sync-conflict*"` n'avaient trouvé aucun fichier physique — le conflit était enregistré dans l'**index interne Syncthing** (base SQLite locale), le fichier `.gitkeep` de Win11 étant le déclencheur, non une copie de conflit supplémentaire.

### 8.3 Résolution — Suppression directe dans le volume Syncthing

```powershell
# Supprimer .gitkeep du volume Syncthing sur Win11
docker exec sda-syncthing sh -c "rm -f /var/syncthing/SDA_Shared/.gitkeep && echo 'Supprimé'"
```

**Résultat immédiat (~15 s) :** Syncthing propage la suppression aux pairs. Les 3 nœuds passent à `100% — Synchronisé`.

> **Important :** Cette commande utilise `docker exec` (avec le **nom du conteneur** `sda-syncthing`), et non `docker compose exec` qui utilise le **nom du service** (`syncthing`). L'image Syncthing utilise BusyBox — `find -ls` n'est pas supporté (remplacer par `-print` ou `-delete`).

### 8.4 Prévention définitive — Fichier `.stignore`

Pour éviter que ce type de conflit ne se reproduise (fichiers git ou temporaires parasites dans le volume Syncthing), un fichier `.stignore` a été créé :

```powershell
# Créé directement dans le volume Syncthing sur Win11
docker exec sda-syncthing sh -c "printf '.gitkeep\n*.tmp\n*.part\n' > /var/syncthing/SDA_Shared/.stignore"
```

**Contenu du `.stignore` :**
```
.gitkeep
*.tmp
*.part
```

**Propriétés clés du `.stignore` :**

| Propriété | Détail |
|-----------|--------|
| Emplacement | Racine du dossier partagé (`/var/syncthing/SDA_Shared/.stignore`) |
| Auto-synchronisation | ✅ Syncthing réplique `.stignore` lui-même sur tous les pairs |
| Portée | S'applique automatiquement à Win11, Ubuntu, Kali après sync |
| Différence avec GUI | Les patterns GUI sont en base SQLite locale — `.stignore` est global au maillage |

**Commit associé :**
```
17c7d35  chore: replace .gitkeep with .stignore in shared_storage
```
- Suppression de `data/shared_storage/.gitkeep` du dépôt git
- Ajout de `data/shared_storage/.stignore` versionné

### 8.5 Tableau de synthèse — Chronologie du conflit

| Étape | Action | Résultat |
|-------|--------|---------|
| Couplage Ubuntu ↔ Win11 | Syncthing indexe les deux nœuds | `.gitkeep` détecté en conflit |
| `find ... -name ".sync-conflict*"` | Recherche fichier conflit physique | Aucun trouvé — conflit en index interne |
| `ls -la /var/syncthing/SDA_Shared/` | Inspection volume Syncthing Win11 | `.gitkeep` (0 B) identifié comme source |
| `docker exec sda-syncthing rm .gitkeep` | Suppression depuis le conteneur | Propagation automatique → 100% |
| Création `.stignore` | Protection permanente | `.gitkeep` ignoré sur les 3 nœuds |
| `git commit 17c7d35` | Mise à jour dépôt | `.gitkeep` retiré, `.stignore` versionné |

---

## 9. Déploiement Node 2 — VM Ubuntu 26.04 LTS

### 9.1 Contexte — Remplacement de Windows 10 par Ubuntu

La VM Windows 10 initialement prévue comme Node 2 a été abandonnée en raison d'une consommation excessive de ressources (mémoire RAM, overhead graphique) incompatible avec la présentation POC simultanée de 3 machines. Elle a été remplacée par une VM Ubuntu 26.04 LTS "resolute" (2 vCPU, 4 GB RAM, 120 GB disque).

| Paramètre | Valeur |
|-----------|--------|
| OS | Ubuntu 26.04 LTS "resolute" (64-bit) |
| Interface WAN (ens33) | `192.168.1.40` |
| Interface LAN VMnet1 (ens37) | `192.168.200.130` ← Syncthing P2P |
| Docker | `29.1.3` |
| Docker Compose plugin | `v5.1.4` |

### 9.2 Spécificités Ubuntu 26.04 — Problèmes d'installation

**Problème 1 — `docker-compose-plugin` absent des dépôts Ubuntu :**

```bash
sudo apt install -y docker-compose-plugin
# E: Unable to locate package docker-compose-plugin
```

Ubuntu 26.04 distribue `docker.io` mais pas `docker-compose-plugin`. Ce paquet est uniquement disponible dans le dépôt apt officiel Docker.

**Fix :**
```bash
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | \
    sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
    https://download.docker.com/linux/ubuntu \
    $(. /etc/os-release && echo $VERSION_CODENAME) stable" | \
    sudo tee /etc/apt/sources.list.d/docker.list
sudo apt update && sudo apt install -y docker-compose-plugin
```

**Problème 2 — `newgrp` absent par défaut :**

```bash
newgrp docker
# Command 'newgrp' not found
```

Sur Ubuntu 26.04, `newgrp` (qui permet d'appliquer un changement de groupe sans déconnexion) fait partie du paquet `util-linux-extra`, non installé par défaut — contrairement à Ubuntu 24.04.

**Fix :** `sudo apt install -y util-linux-extra`

### 9.3 Déploiement et validation

```bash
cd ~/Desktop/PFE && git clone https://github.com/mpigajesse/sda-prototype.git
cd sda-prototype
bash scripts/generate-certs.sh
cat > .env <<'EOF'
DB_ENCRYPTION_KEY=4a82d9f9199d1159159b55ef7359bc1c035d5587024e3ed15edb4f8f4b30bfb9
PARQUET_FERNET_KEY=PGKzI2PL8qrYh_IFs98fAguugjtpcOOzv4P05NbQ1lk=
EOF
docker compose up --build -d
sleep 30
docker compose ps
bash scripts/setup-syncthing-key.sh   # injection clé Syncthing
```

> Les clés `.env` sont identiques à Node 1 (Win11) pour garantir la lisibilité croisée des fichiers Parquet chiffrés.

**Résultat (4/4 conteneurs Up) :**
```
NAME            STATUS
sda-backend     Up X minutes (healthy)
sda-frontend    Up X minutes (healthy)
sda-nginx       Up X minutes
sda-syncthing   Up X minutes (healthy)
```

---

## 10. Corrections dashboard frontend — Plateforme et Mémoire

### 9.1 Problème — `Plateforme: undefined/undefined`

**Symptôme :** Le composant `NodeIdentityCard` affiche `undefined/undefined` pour le champ Plateforme sur tous les nœuds.

**Cause technique :**  
L'interface TypeScript `SyncthingSystem` déclarait les champs `os` et `arch`, supposés venir de `/rest/system/status`. Or cet endpoint ne retourne pas ces champs — ils proviennent de `/rest/system/version` :

```json
// /rest/system/status — champs retournés :
{ "alloc": 3718720, "cpuPercent": 0, "myID": "...", "uptime": 1630, "sys": 19933464, ... }
// ABSENT : os, arch

// /rest/system/version — champs retournés :
{ "arch": "amd64", "os": "linux", "version": "v2.1.0", ... }
```

TypeScript ne signale aucune erreur car le cast `as Promise<T>` sur `fetch().json()` est non-vérifié — les champs manquants sont silencieusement `undefined` au runtime.

**Fix — Endpoint backend `/api/v1/node/info` :**  
Plutôt que d'afficher `linux/amd64` (plateforme du conteneur), un endpoint FastAPI expose l'OS réel de la machine hôte via le module Python `platform` :

```python
import platform

@app.get("/api/v1/node/info", tags=["Système"])
def node_info():
    return {
        "host_os": platform.system(),
        "host_os_release": platform.release(),
        "host_arch": platform.machine(),
        "host_hostname": platform.node(),
    }
```

**Résultat par nœud :**

| Nœud | Plateforme affichée |
|------|---------------------|
| Win11 | `Linux 6.6.114.1-microsoft-standard-WSL2 (x86_64)` |
| Kali | `Linux 6.x.x (x86_64)` |
| Ubuntu | `Linux 6.x.x (x86_64)` |

> Le kernel affiché sur Win11 est celui de **WSL2** (Windows Subsystem for Linux) — comportement normal car Docker Desktop sur Windows utilise WSL2 comme backend.

**Capture :** `[SCREENSHOT: dashboard_plateforme_undefined.png]`  
**Capture :** `[SCREENSHOT: dashboard_plateforme_corrigee.png]`

---

### 9.2 Problème — `Mémoire: NaN MB`

**Symptôme :** La MetricCard Mémoire affiche `NaN MB` sur Win11 et Kali.

**Cause technique :**  
L'interface TypeScript déclarait `mem: number` dans `SyncthingSystem`, mais l'API `/rest/system/status` ne retourne pas de champ `mem`. Les champs réels sont `alloc` (heap Go alloué) et `sys` (mémoire totale obtenue de l'OS) :

```typescript
// AVANT — champ inexistant → undefined → NaN
interface SyncthingSystem { mem: number }
formatMem(system.mem)  // NaN MB

// APRÈS — champ correct
interface SyncthingSystem { alloc: number; sys: number }
formatMem(system.alloc)  // ex: "4 MB"
```

**Valeurs typiques :**

| Champ | Signification | Valeur observée |
|-------|---------------|-----------------|
| `alloc` | Heap Go actuellement alloué | ~3–8 MB |
| `sys` | Mémoire totale réservée par l'OS | ~19–25 MB |

`alloc` est la métrique la plus pertinente pour monitorer la consommation réelle de Syncthing.

**Capture :** `[SCREENSHOT: dashboard_memoire_nan.png]`  
**Capture :** `[SCREENSHOT: dashboard_memoire_corrigee.png]`

---

## 11. État final validé — 3 nœuds

### 11.1 Tableau de bord Win11

| Indicateur | Valeur |
|-----------|--------|
| Backend | Opérationnel |
| Pairs connectés | **2/2** (Ubuntu + Kali) |
| Syncthing version | v2.1.0 |
| Plateforme | `Linux 6.6.114.1-microsoft-standard-WSL2 (x86_64)` |
| Mémoire | ~4 MB (heap Syncthing) |
| Sync SDA_Shared | **100% — 15 fichiers** ✅ |
| Taille totale | 22.4 KB |
| Type connexion | TCP direct LAN (192.168.200.x) |
| Device ID | GHIJH3G-FFVIRSX-J5XXQNW-... |

**Capture :** `[SCREENSHOT: win11_dashboard_final_100pct.png]`

### 11.2 Tableau de bord Ubuntu 26.04 (Node 2)

| Indicateur | Valeur |
|-----------|--------|
| Backend | Opérationnel |
| Conteneurs | 4/4 healthy |
| Dashboard | Accessible via `https://localhost/` |
| Syncthing GUI | `http://localhost:8384` |
| Syncthing version | v2.1.0 |
| Device ID | G43Q6SJ-N65TTP5-BEESX7I-... |
| Syncthing API (dashboard) | ✅ Opérationnel (après setup-syncthing-key.sh) |
| Interface LAN | `ens37` — `192.168.200.130` |
| Pairs connectés | **2/2** (Win11 + Kali) ✅ |
| Sync SDA_Shared | **100% — 15 fichiers** ✅ |

**Capture :** `[SCREENSHOT: ubuntu_dashboard_operational.png]`

---

### 11.3 Tableau de bord Kali

| Indicateur | Valeur |
|-----------|--------|
| Backend | Opérationnel |
| Pairs connectés | 1/1 |
| Syncthing version | v2.1.0 |
| Plateforme | `Linux 6.x.x (x86_64)` |
| Mémoire | ~4 MB |
| Sync SDA_Shared | 100% — 15 fichiers |
| Taille totale | 22.4 KB |
| Device ID | VFTEXUZ-3T7QLXH-7ZBFASM-... |

**Capture :** `[SCREENSHOT: kali_dashboard_final_100pct.png]`

### 11.4 Commits git de la session

| Hash | Type | Description |
|------|------|-------------|
| `...` | `fix` | Correction SAN certificats + script generate-certs.sh |
| `...` | `feat` | docs: guide déploiement multi-nœuds (mTLS navigateur) |
| `2b6a7bd` | `fix` | nginx: corriger chemin proxy Syncthing API (`/rest/` doublé) |
| `7ffeece` | `chore` | gitignore: exclure `.stfolder` et `.sync-conflict-*` |
| `a23f2d8` | `docs` | journal technique déploiement mTLS + sync P2P |
| `5590d5f` | `fix` | frontend: os/arch depuis `/system/version` |
| `ab481ec` | `fix` | frontend+backend: OS hôte réel + mémoire NaN corrigée |
| `e337f6b` | `fix` | nginx: bypass envsubst Alpine (montage direct conf.d) |
| `e337f6b` | `docs` | node-ubuntu-config: VM Ubuntu 26.04 LTS — Node 2 |
| `2dc401c` | `fix` | nginx: injecter X-API-Key Syncthing via include par nœud |
| `2dc401c` | `feat` | scripts: setup-syncthing-key.sh (extraction clé + reload nginx) |

### 11.5 Critères de succès POC — État

| Critère | Cible | Résultat |
|---------|-------|----------|
| Réplication P2P | 3+ nœuds, 0 perte | ✅ 3 nœuds déployés — couplage P2P ⏳ à finaliser |
| Conflits CRDT | 0 conflit non résolu | ✅ Conflit `.gitkeep` résolu |
| Latence requête locale | < 100ms | ✅ (mesuré en session précédente) |
| Déploiement Docker | < 30 min | ✅ (~15 min sur nœud neuf) |
| Accès HTTPS mTLS | Navigateur + cert client | ✅ Chrome Win11 + Firefox Kali + Firefox Ubuntu |
| Test couverture | > 80% | ✅ (CI GitHub Actions) |
| Dashboard — Plateforme | OS hôte réel | ✅ WSL2/Linux affiché |
| Dashboard — Mémoire | Valeur numérique MB | ✅ NaN corrigé |
| Dashboard — Syncthing API | Métriques affichées | ✅ Fix include nginx + setup-syncthing-key.sh |
| Infrastructure 3 nœuds | 4/4 conteneurs healthy | ✅ Win11 + Ubuntu + Kali — tous opérationnels |

---

## 12. Index des captures d'écran

> **Instructions :** Insérer les captures dans le dossier `docs/rapport/screenshots/` et remplacer les balises `[SCREENSHOT: xxx.png]` par les chemins relatifs dans le rapport final.

| Balise | Description | Section |
|--------|-------------|---------|
| `chrome_err_cert_common_name_invalid.png` | Erreur Chrome sans SAN | 2.1 |
| `san_verification_output.png` | Sortie openssl confirmant le SAN | 2.4 |
| `certutil_ca_import_success.png` | Import CA dans Windows via certutil | 3.1 |
| `chrome_cert_selection_dialog.png` | Boîte de dialogue sélection cert client Chrome | 3.1 |
| `win11_frontend_dashboard_operational.png` | Dashboard Win11 — premier accès réussi | 3.1 |
| `firefox_kali_cert_import.png` | Import certificats dans Firefox Kali | 3.2 |
| `kali_frontend_dashboard_operational.png` | Dashboard Kali — accès réussi | 3.2 |
| `frontend_syncthing_impossible_joindre.png` | Erreur "Impossible de joindre Syncthing" | 5.1 |
| `frontend_syncthing_ok_apres_fix.png` | Dashboard Syncthing opérationnel | 5.1 |
| `syncthing_folder_liaisons_tab.png` | Onglet Liaisons du dossier SDA_Shared | 6.2 |
| `syncthing_relay_connection.png` | Connexion Kali via Relay (avant fix) | 7.1 |
| `syncthing_win11_peer_lan_config.png` | Config adresse LAN direct sur Win11 | 7.2 |
| `syncthing_kali_peer_lan_config.png` | Config adresse LAN direct sur Kali | 7.2 |
| `syncthing_direct_tcp_connection.png` | Connexion TCP directe établie | 7.2 |
| `dashboard_plateforme_undefined.png` | Plateforme `undefined/undefined` avant fix | 9.1 |
| `dashboard_plateforme_corrigee.png` | Plateforme OS hôte après fix | 9.1 |
| `dashboard_memoire_nan.png` | Mémoire `NaN MB` avant fix | 9.2 |
| `dashboard_memoire_corrigee.png` | Mémoire en MB après fix | 9.2 |
| `win11_dashboard_final_100pct.png` | État final Win11 — 100% sync | 11.1 |
| `ubuntu_dashboard_operational.png` | Dashboard Ubuntu — 4/4 conteneurs + Syncthing OK | 11.2 |
| `kali_dashboard_final_100pct.png` | État final Kali — 100% sync | 11.3 |
| `setup_syncthing_key_output.png` | Sortie du script setup-syncthing-key.sh | 5.3 |

---

## 13. Observations — Comportements normaux documentés

Cette section recense les comportements initialement interprétés comme des anomalies, mais qui s'avèrent être le fonctionnement attendu de l'infrastructure.

---

### 13.1 "Adresse active" Syncthing affiche 172.21.0.x sur Win11

**Observation :** Dans la GUI Syncthing de Win11 (`http://localhost:8384`), les fiches des pairs distants (Ubuntu, Kali) affichent une adresse active sur le réseau `172.21.0.0/16` au lieu du `192.168.200.0/24` configuré.

Exemple observé lors du couplage 3 nœuds :
```
Pair VFTEXUZ (Kali)
  Adresse active       172.21.0.1:50868
  Adresses configurées tcp://192.168.200.128:22000

Pair G43Q6SJ (Ubuntu)
  Adresse active       172.21.0.1:40088
  Adresses configurées tcp://192.168.200.130:22000
```

**Cause — NAT masquerade Docker sur Windows :**

Docker sur Windows (Docker Desktop / WSL2) publie les ports du conteneur via un mécanisme de NAT masquerade. Quand une connexion TCP entrante arrive sur Win11 depuis une VM (ex. `192.168.200.130`), le stack réseau WSL2 réécrit l'IP source en `172.21.0.1` (passerelle du bridge Docker interne) avant de la transmettre au conteneur :

```
VM Ubuntu (192.168.200.130)
  │  connexion vers 192.168.200.1:22000 (VMnet1 Win11)
  ▼
Win11 Host — VMnet1 adapter (192.168.200.1)
  │  port 22000 publishé → conteneur:22000
  │  Docker NAT : source 192.168.200.130 → 172.21.0.1
  ▼
Conteneur sda-syncthing (Win11)
  └─ voit la connexion provenir de 172.21.0.1 (artefact du NAT)
```

**Pourquoi les deux adresses coexistent dans l'UI :**

| Champ | Valeur | Signification |
|-------|--------|---------------|
| Adresses configurées | `tcp://192.168.200.130:22000` | Adresse que Win11 utilise pour *initier* les connexions vers Ubuntu |
| Adresse active | `172.21.0.1:40088` | Adresse source vue par le conteneur pour les connexions *entrantes* (après NAT) |

**Conclusion — Pas d'action corrective requise :**

La synchronisation fonctionne à 100% malgré cet affichage :
- Win11 → VMs : connexions sortantes utilisent bien `192.168.200.x` ✅
- VMs → Win11 : connexions entrantes reçues et relayées par Docker ✅

L'affichage `172.21.0.1` est un artefact inévitable du NAT Docker Windows. Sur Linux natif, l'IP réelle du pair serait visible. Une configuration macvlan ou `--network=host` permettrait d'y remédier mais est inutile ici — la sync est opérationnelle et les adresses configurées restent correctes.

**Référence :** Section Dépannage dans `docs/install/node-deployment.md`

---

*Document généré pour intégration dans le Rapport de Tests et le Rapport Final de Stage.*
