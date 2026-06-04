# Plan de Soutenance — Expérience Professionnelle de Fin d'Études
<!-- NOM FICHIER MOODLE : MPIGA_Jesse_FE_Promo2026_Plan_soutenance.pdf -->
<!-- DEADLINE DEPOT MOODLE : 24/06/2026 (1 semaine avant soutenance) -->

---

**Étudiant :** Jesse MPIGA-ODOUMBA
**Entreprise :** AL BARAA CONSULTING
**Tuteur entreprise :** Mme Soumia CHOKRI
**Tuteur EIGSI :** M. Ayoub AMRANI
**Date de soutenance :** 01 juillet 2026 à 10h00
**Lieu :** EIGSI Casablanca — en présentiel

**Sujet :** Conception et Implémentation d'une Architecture Coffre-Fort Data P2P Souveraine (SDA)

---

## Sommaire de la Présentation

| # | Partie | Durée | Slides |
|---|--------|-------|--------|
| 1 | Présentation de l'entreprise et contexte | 3 min | 1–2 |
| 2 | Problématique et enjeux | 3 min | 3–4 |
| 3 | État de l'art et positionnement | 2 min | 5 |
| 4 | Architecture SDA — vue d'ensemble | 4 min | 6–7 |
| 5 | Démonstration live — cluster 3 nœuds | 5 min | 8–9 |
| 6 | Résultats et validation — 32/33 PASS | 4 min | 10–11 |
| 7 | Sécurité et conformité AUDPF | 3 min | 12–13 |
| 8 | Difficultés rencontrées et valeur ingénieur | 4 min | 14–15 |
| 9 | Bilan et perspectives | 2 min | 16 |
| 10 | Conclusion | 1 min | 17 |
| — | **TOTAL** | **31 min** | **17 slides** |

---

## Détail par Partie

### Partie 1 — AL BARAA CONSULTING et Contexte (3 min — Slides 1–2)

**Slide 1 — Page de garde**
- Logo EIGSI + Logo AL BARAA CONSULTING
- Titre : "SDA — Sovereign Data Agent : Architecture Coffre-Fort Data P2P Souveraine"
- Nom, Promo 2026, Tuteur entreprise, Tuteur EIGSI, Date

**Slide 2 — Entreprise et mission**
- AL BARAA CONSULTING : cabinet de conseil numérique, Casablanca, fondé 2017
- Clientèle publique et privée, ingénierie de systèmes d'information
- La mission confiée : concevoir un prototype de BaaS local souverain
- Durée : 24 semaines (19 février → 01 juillet 2026)

*Message clé :* AL BARAA m'a confié la responsabilité complète d'un projet stratégique — pas de tâches d'exécution, mais une mission d'ingénieur.

---

### Partie 2 — Problématique et Enjeux (3 min — Slides 3–4)

**Slide 3 — La problématique**
- 63% des organisations africaines stockent leurs données sur des clouds étrangers (AWS, Azure, GCP)
- Risques : localisation inconnue, conditions d'accès imposées, discontinuité de service
- AUDPF (AU Data Policy Framework, déc. 2025) : cadre réglementaire exigeant la souveraineté des données
- Question centrale : *Comment garantir la souveraineté, la sécurité et la résilience des données sans cloud centralisé étranger ?*

**Slide 4 — La solution proposée**
- "Brique Universelle" décentralisée : chaque terminal devient un nœud de stockage intelligent
- 3 piliers : Autonomie infrastructurelle / Ingénierie par orchestration / Sécurité native
- 100% open-source, 0 MAD de licences, 0 dépendance externe

*Message clé :* Ce n'est pas un problème technique mais un enjeu de souveraineté. La solution technique répond à un besoin stratégique africain réel.

---

### Partie 3 — État de l'Art (2 min — Slide 5)

**Slide 5 — Positionnement vs alternatives**

| Critère | IPFS | Solid | **SDA** |
|---------|------|-------|---------|
| OLAP natif | ❌ | ❌ | ✅ DuckDB |
| Offline-total | ⚠️ | ❌ | ✅ |
| CRDT | ❌ | ❌ | ✅ LWW |
| Souveraineté | ⚠️ | ⚠️ | ✅ Totale |
| Conformité AUDPF | ⚠️ | ❌ | ✅ |

*Message clé :* SDA comble un vide réel — aucune solution existante ne combine OLAP embarqué, offline-first radical et souveraineté infrastructurelle totale.

---

### Partie 4 — Architecture SDA (4 min — Slides 6–7)

**Slide 6 — Vue d'ensemble architecture**
- Schéma C4 : 4 services Docker (nginx mTLS / FastAPI / DuckDB+SQLite / Syncthing)
- Paradigme Code-to-Data : les données restent locales, le code est l'artefact mobile
- Cluster 3 nœuds : Win11, Ubuntu 26.04, Kali Linux — réseau VMnet1

**Slide 7 — Flux de données**
- Ingestion : `POST /api/v1/data/ingest` → DuckDB → export Parquet → Fernet chiffré → Syncthing P2P
- Audit trail SHA-256 chaîné : chaque enregistrement intègre le hash du précédent
- CRDT LWW : réconciliation automatique des conflits en cas de disconnexion

*Message clé :* L'architecture est modulaire — chaque couche peut être remplacée indépendamment.

---

### Partie 5 — Démonstration Live (5 min — Slides 8–9)

**Slide 8 — Dashboard SDA** *(capture d'écran)*
- Vue d'ensemble : statut nœud, pairs connectés, métriques Syncthing
- Page Cluster P2P : visualisation réseau, paquets synchronisés en temps réel
- Page Coffre-fort : fichiers chiffrés P2P

**Slide 9 — Démo en direct** *(sur machine)*
1. `docker compose ps` → 4 conteneurs healthy sur Win11
2. `POST /api/v1/data/ingest` depuis Win11
3. Observation du fichier Parquet qui apparaît sur Ubuntu (~15s)
4. Requête DuckDB depuis Ubuntu — affiche les données de Win11

*Message clé :* Ce que vous voyez est réel — 3 machines hétérogènes, synchronisation automatique, zéro intervention manuelle.

---

### Partie 6 — Résultats et Validation (4 min — Slides 10–11)

**Slide 10 — 32/33 PASS**
- Tableau des résultats par nœud et par test
- Win11 : 10/11 (1 SKIP technique validé navigateur) — Ubuntu : 11/11 — Kali : 11/11
- 0 FAIL sur les 33 tests exécutés

**Slide 11 — Critères POC**
- Tableau des 9 critères de succès — tous atteints
- Stabilité : 45h d'uptime continu sur Win11
- Déploiement : 15 min (cible < 30 min ✅)
- DuckDB : 609 enregistrements, 16 tenants, < 1s

*Message clé :* Pas une démo de laboratoire — un cluster validé pendant 45 heures en conditions réelles.

---

### Partie 7 — Sécurité et Conformité AUDPF (3 min — Slides 12–13)

**Slide 12 — Sécurité en couches**
- mTLS x509 : HTTP 400 sans certificat client — validé sur 3 nœuds
- TLS 1.3 exclusif : TLS 1.2 rejeté (handshake failure)
- Fernet AES-128-CBC + HMAC-SHA256 : chiffrement at-rest des Parquet
- SQLCipher AES-256 : chiffrement de la base SQLite

**Slide 13 — Conformité AUDPF**
- Les données ne quittent jamais le périmètre de confiance de l'organisation
- Audit trail SHA-256 chaîné : traçabilité non falsifiable
- 0 dépendance à un service cloud externe — fonctionnement 100% local

*Message clé :* SDA est conforme par design, pas par configuration — impossible de contourner la souveraineté.

---

### Partie 8 — Difficultés et Valeur Ingénieur (4 min — Slides 14–15)

**Slide 14 — Top 3 incidents résolus**

| Problème | Approche technicien | Approche ingénieur |
|----------|--------------------|--------------------|
| nginx crash (envsubst) | Désactiver le template | Bypass structurel — montage direct en `/conf.d/` |
| Clé API Syncthing perdue | Relancer le script manuellement | Auto-injection via `nginx-entrypoint.sh` + healthcheck |
| Animations SVG tremblent | Désactiver les animations | `useRef` + DOM direct — comprendre le modèle React |

**Slide 15 — Ce que j'aurais fait comme technicien vs comme ingénieur**
- Technicien : corriger le symptôme, passer à la suite
- Ingénieur : comprendre la cause racine, concevoir une solution pérenne, documenter pour l'équipe
- Exemple concret : la clé API Syncthing → 3 commits successifs → solution qui s'auto-guérit à chaque redémarrage

*Message clé :* La valeur ajoutée de l'ingénieur n'est pas dans la vitesse d'exécution mais dans la qualité de la réflexion.

---

### Partie 9 — Bilan et Perspectives (2 min — Slide 16)

**Slide 16 — Bilan et ouvertures**

*Ce qui a été démontré :*
- Architecture souveraine P2P : faisable, performante, déployable
- 100% open-source, 0 MAD de licences propriétaires
- ROI estimatif : 184% dès la 1ère année pour 10 organisations adoptrices

*Perspectives techniques :*
- Tests de charge 10+ nœuds
- API gRPC pour les cas haute performance
- Interface mobile React Native
- Déploiement sur réseau WAN (Syncthing relay)

*Perspectives professionnelles :*
- Systèmes distribués et cybersécurité appliqués aux enjeux africains
- Contribution open-source à des outils de souveraineté numérique

---

### Partie 10 — Conclusion (1 min — Slide 17)

**Slide 17 — Message final**
- 32/33 PASS — tous les critères POC atteints
- Une architecture qui répond à un besoin réel et urgent pour l'Afrique
- Un projet technique + stratégique + humain

Citation de clôture (optionnelle) :
> *"La souveraineté numérique n'est pas un luxe technologique — c'est une condition de l'autonomie stratégique des organisations africaines."*

---

## Questions Jury — Préparation

| Question probable | Réponse préparée |
|-------------------|-----------------|
| Pourquoi pas une base de données cloud souveraine locale (type MinIO) ? | DuckDB résout le besoin analytique OLAP directement, sans infrastructure serveur séparée. MinIO est optimisé pour le stockage objet, pas pour les requêtes analytiques. |
| Comment gérez-vous la scalabilité au-delà de 3 nœuds ? | L'architecture est horizontalement scalable par nature — ajouter un nœud = `docker compose up`. Syncthing est testé jusqu'à 1000+ nœuds en production. |
| Le chiffrement Fernet est-il suffisant pour une production ? | Pour un POC, AES-128-CBC + HMAC-SHA256 est robuste. En production, on passerait à AES-256 et à une gestion de clés via HashiCorp Vault ou équivalent. |
| Que se passe-t-il si un nœud est compromis ? | L'architecture est cloisonnée par nœud. Un nœud compromis ne peut pas accéder aux données des autres nœuds (clés Fernet partagées uniquement entre nœuds de confiance). La révocation d'un certificat mTLS coupe immédiatement l'accès. |
| Pourquoi React et pas une solution plus légère ? | React permet une interface riche et maintenable pour un dashboard de monitoring. Pour un déploiement production en zones à faible bande passante, une version allégée ou une PWA serait envisagée. |
| Avez-vous eu des FAIL lors des tests ? | 0 FAIL sur les 33 tests. 1 SKIP sur Win11 (limitation technique TLS schannel), validé manuellement via navigateur Chrome. |

---

## Checklist Avant Dépôt (24/06/2026)

- [ ] Le contexte AL BARAA et la mission sont clairement définis (slide 2)
- [ ] La problématique est formulée de manière précise (slide 3)
- [ ] Les objectifs sont SMART et mesurables (slide 4)
- [ ] Les réalisations principales sont présentées avec chiffres (slides 6–9)
- [ ] Les résultats sont quantifiés : 32/33 PASS, 15 min, 45h, 609 enreg. (slides 10–11)
- [ ] Les compétences développées sont explicitées (slide 15)
- [ ] La valeur ajoutée pour AL BARAA est démontrée (slide 16)
- [ ] Le plan est logique et cohérent
- [ ] Durée totale : 27–33 minutes chronométrée
- [ ] PDF produit et prêt pour dépôt Moodle

---

*Plan de Soutenance — MPIGA-ODOUMBA Jesse — EIGSI Promo 2026*
*Soutenance : 01/07/2026 à 10h00 — EIGSI Casablanca*
