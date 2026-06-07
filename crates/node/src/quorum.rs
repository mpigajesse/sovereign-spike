//! Logique de quorum pour le failover automatique (LOT 5 — §7.4 #4b & #5).
//!
//! Problème résolu (le seul vrai manque du socle) :
//!   Le failover MANUEL est déjà prouvé (promotion `pg_ctl` + incrément d'époque).
//!   Le fencing anti-split-brain *après coup* est prouvé (`EpochGuard`).
//!   Manquait : la DÉCISION AUTOMATIQUE de promouvoir un standby quand le primary
//!   tombe, SANS créer de split-brain lors d'une partition réseau.
//!
//! Principe (élection de leader simplifiée, inspirée de Raft) :
//!   1. Chaque superviseur surveille le primary par heartbeat.
//!   2. Quand le primary est jugé mort, le standby le PLUS PRIORITAIRE encore
//!      joignable se déclare candidat et demande un vote à tous ses pairs.
//!   3. Une promotion n'a lieu QUE si le candidat réunit la MAJORITÉ STRICTE
//!      des superviseurs du cluster (⌊N/2⌋ + 1, son propre vote inclus).
//!
//! Propriété anti-split-brain *préventive* :
//!   Un nœud isolé en minorité (réseau partitionné) ne peut jamais atteindre la
//!   majorité → il REFUSE de se promouvoir. Combiné au fencing par époque (qui
//!   bloque l'ancien primary à son retour), le split-brain est interdit des deux
//!   côtés : empêché *avant* (quorum) et neutralisé *après* (époque).
//!
//! Ce module ne contient QUE de la logique pure (aucune E/S) afin d'être
//! entièrement testable. Le binaire `supervisor` câble HTTP + PostgreSQL autour.

use std::collections::HashSet;

/// Nombre de votes requis pour la majorité stricte d'un cluster de `total` nœuds.
///
/// `majority(3) == 2`, `majority(2) == 2`, `majority(5) == 3`.
/// Un cluster d'un seul nœud a une majorité de 1 (mode solo dégénéré).
pub fn majority(total: usize) -> usize {
    total / 2 + 1
}

/// Le candidat réunit-il le quorum ?
///
/// `votes_granted` inclut le vote du candidat pour lui-même.
pub fn has_quorum(votes_granted: usize, total: usize) -> bool {
    total > 0 && votes_granted >= majority(total)
}

/// Décision d'auto-candidature.
///
/// Un standby ne se porte candidat que s'il est le PLUS prioritaire parmi les
/// standbys qu'il sait encore vivants (lui inclus). Une priorité plus FAIBLE
/// (numériquement) = plus prioritaire (rang 0 = premier successeur désigné).
///
/// Évite que deux standbys lancent une élection concurrente : le rang départage
/// de façon déterministe. En cas d'égalité de rang (configuration erronée), on
/// départage par `node_id` croissant pour rester déterministe.
pub fn is_preferred_candidate(
    my_rank: u32,
    my_node_id: &str,
    reachable_standbys: &[(u32, String)], // (rang, node_id) des autres standbys joignables
) -> bool {
    reachable_standbys.iter().all(|(rank, id)| {
        my_rank < *rank || (my_rank == *rank && my_node_id <= id.as_str())
    })
}

/// État de vote d'un superviseur pour le terme d'élection courant.
///
/// Le `term` est un compteur monotone : chaque nouvelle élection l'incrémente.
/// Un superviseur ne vote qu'UNE fois par terme (comme Raft) → empêche qu'un
/// même nœud accorde sa voix à deux candidats concurrents dans la même élection.
#[derive(Debug, Clone, Default)]
pub struct VoteLedger {
    current_term: u64,
    voted_for:    Option<String>, // node_id du candidat voté pour `current_term`
}

impl VoteLedger {
    pub fn new() -> Self {
        Self { current_term: 0, voted_for: None }
    }

    pub fn current_term(&self) -> u64 {
        self.current_term
    }

    /// Décide d'accorder ou non un vote à un candidat.
    ///
    /// Règles (toutes doivent être vraies) :
    ///   - le terme du candidat est ≥ au terme courant (pas une élection périmée) ;
    ///   - le superviseur considère lui aussi le primary comme mort
    ///     (`primary_seen_alive == false`) — on ne renverse pas un primary sain ;
    ///   - il n'a pas déjà voté pour QUELQU'UN D'AUTRE dans ce terme.
    ///
    /// Effet de bord (sur `self`) : si le terme du candidat est plus récent, le
    /// ledger avance au nouveau terme et réinitialise le vote. Le vote accordé
    /// est enregistré pour empêcher un double-vote dans le même terme.
    pub fn grant_vote(
        &mut self,
        candidate_term: u64,
        candidate_id: &str,
        primary_seen_alive: bool,
    ) -> bool {
        if candidate_term < self.current_term {
            return false; // élection périmée
        }
        if primary_seen_alive {
            return false; // ce superviseur voit le primary vivant → refus
        }
        if candidate_term > self.current_term {
            // Nouveau terme : on s'aligne et on réinitialise le vote.
            self.current_term = candidate_term;
            self.voted_for = None;
        }
        match &self.voted_for {
            Some(already) if already != candidate_id => false, // déjà voté ailleurs
            _ => {
                self.voted_for = Some(candidate_id.to_string());
                true
            }
        }
    }
}

/// Agrège les réponses de vote et tranche la promotion.
///
/// `granters` = ensemble des node_id ayant accordé leur voix (le candidat doit
/// s'y ajouter lui-même). `total` = nombre total de superviseurs du cluster.
///
/// Renvoie `true` si la promotion est autorisée (quorum atteint).
pub fn promotion_authorized(granters: &HashSet<String>, candidate_id: &str, total: usize) -> bool {
    let mut votes = granters.clone();
    votes.insert(candidate_id.to_string()); // le candidat vote toujours pour lui-même
    has_quorum(votes.len(), total)
}

/// Détecteur de panne à seuil : le primary n'est déclaré mort qu'après
/// `threshold` échecs CONSÉCUTIFS de heartbeat. Un seul succès remet à zéro.
///
/// Évite les promotions intempestives sur un micro-incident réseau.
#[derive(Debug, Clone)]
pub struct FailureDetector {
    consecutive_failures: u32,
    threshold:            u32,
}

impl FailureDetector {
    /// `threshold` doit être ≥ 1.
    pub fn new(threshold: u32) -> Self {
        Self { consecutive_failures: 0, threshold: threshold.max(1) }
    }

    /// Enregistre un heartbeat réussi → remet le compteur à zéro.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Enregistre un heartbeat raté → incrémente le compteur.
    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Le primary est-il considéré comme mort (seuil atteint) ?
    pub fn is_primary_down(&self) -> bool {
        self.consecutive_failures >= self.threshold
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majorite_cluster_impair_et_pair() {
        assert_eq!(majority(1), 1);
        assert_eq!(majority(2), 2);
        assert_eq!(majority(3), 2);
        assert_eq!(majority(4), 3);
        assert_eq!(majority(5), 3);
    }

    #[test]
    fn quorum_atteint_ou_non() {
        assert!(has_quorum(2, 3));
        assert!(has_quorum(3, 3));
        assert!(!has_quorum(1, 3));
        assert!(!has_quorum(0, 0)); // cluster vide → jamais de quorum
    }

    #[test]
    fn candidat_prefere_le_plus_petit_rang() {
        // Rang 0 : premier successeur → candidat même si d'autres standbys vivants
        assert!(is_preferred_candidate(0, "vm1", &[(1, "vm2".into())]));
        // Rang 1 : un standby de rang 0 est joignable → ne se porte PAS candidat
        assert!(!is_preferred_candidate(1, "vm2", &[(0, "vm1".into())]));
        // Aucun autre standby joignable → candidat par défaut
        assert!(is_preferred_candidate(2, "vm3", &[]));
    }

    #[test]
    fn candidat_egalite_de_rang_departagee_par_node_id() {
        // Même rang : le node_id le plus petit l'emporte (déterminisme).
        assert!(is_preferred_candidate(0, "vm1", &[(0, "vm2".into())]));
        assert!(!is_preferred_candidate(0, "vm2", &[(0, "vm1".into())]));
    }

    #[test]
    fn vote_refuse_si_primary_vu_vivant() {
        let mut ledger = VoteLedger::new();
        // Le superviseur voit encore le primary vivant → il protège le primary.
        assert!(!ledger.grant_vote(1, "vm1", true));
    }

    #[test]
    fn vote_accorde_une_seule_fois_par_terme() {
        let mut ledger = VoteLedger::new();
        // Premier candidat du terme 1 : vote accordé.
        assert!(ledger.grant_vote(1, "vm1", false));
        // Second candidat concurrent, même terme : refusé (déjà voté).
        assert!(!ledger.grant_vote(1, "vm2", false));
        // Re-demande du MÊME candidat (retransmission) : idempotent, accepté.
        assert!(ledger.grant_vote(1, "vm1", false));
    }

    #[test]
    fn vote_nouveau_terme_reinitialise_le_choix() {
        let mut ledger = VoteLedger::new();
        assert!(ledger.grant_vote(1, "vm1", false));
        // Terme 2 (nouvelle élection) : le ledger s'aligne et peut revoter.
        assert!(ledger.grant_vote(2, "vm2", false));
        assert_eq!(ledger.current_term(), 2);
    }

    #[test]
    fn vote_terme_perime_refuse() {
        let mut ledger = VoteLedger::new();
        assert!(ledger.grant_vote(5, "vm1", false));
        // Une demande d'un terme antérieur (message en retard) est rejetée.
        assert!(!ledger.grant_vote(3, "vm2", false));
    }

    #[test]
    fn promotion_autorisee_avec_quorum() {
        // Cluster de 3. Le candidat vm1 + 1 voix d'un pair = 2 = majorité.
        let mut granters = HashSet::new();
        granters.insert("vm2".to_string());
        assert!(promotion_authorized(&granters, "vm1", 3));
    }

    #[test]
    fn promotion_refusee_en_minorite() {
        // Cluster de 3, candidat isolé (partition réseau) : seul son propre vote.
        let granters = HashSet::new();
        assert!(!promotion_authorized(&granters, "vm1", 3));
    }

    #[test]
    fn promotion_compte_le_candidat_une_seule_fois() {
        // Le candidat s'auto-comptant ne doit pas gonfler artificiellement le total.
        let mut granters = HashSet::new();
        granters.insert("vm1".to_string()); // le candidat lui-même renvoyé par erreur
        // 1 voix réelle (lui-même) sur 3 → pas de quorum.
        assert!(!promotion_authorized(&granters, "vm1", 3));
    }

    #[test]
    fn detecteur_panne_seuil_consecutif() {
        let mut fd = FailureDetector::new(3);
        fd.record_failure();
        fd.record_failure();
        assert!(!fd.is_primary_down()); // 2 < 3
        fd.record_failure();
        assert!(fd.is_primary_down()); // 3 == seuil
    }

    #[test]
    fn detecteur_panne_un_succes_remet_a_zero() {
        let mut fd = FailureDetector::new(2);
        fd.record_failure();
        fd.record_failure();
        assert!(fd.is_primary_down());
        fd.record_success(); // le primary répond à nouveau
        assert!(!fd.is_primary_down());
        assert_eq!(fd.consecutive_failures(), 0);
    }

    #[test]
    fn detecteur_seuil_minimum_un() {
        // Un seuil 0 est corrigé à 1 (sinon le primary serait "mort" d'emblée).
        let fd = FailureDetector::new(0);
        assert!(!fd.is_primary_down());
    }
}
