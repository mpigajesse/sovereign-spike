import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../api";

interface ClusterStatus {
  role:              string;
  in_recovery:       boolean;
  sync_enabled:      boolean;
  sync_state:        string;
  standby_connected: boolean;
}

export default function Securite() {
  const [epoch,     setEpoch]     = useState<{ epoch: number; primary_host: string } | null>(null);
  const [promoting, setPromoting] = useState(false);
  const [alert,     setAlert]     = useState<{ type: "success" | "error"; msg: string } | null>(null);

  // Administration cluster (machines avec PostgreSQL : primary / standby)
  const role = localStorage.getItem("sovereign_role");
  const hasPg = role === "primary" || role === "standby";
  const [cluster, setCluster] = useState<ClusterStatus | null>(null);
  const [busy,    setBusy]    = useState(false);

  const loadCluster = useCallback(async () => {
    if (!hasPg) return;
    try { setCluster(await invoke<ClusterStatus>("cluster_status")); }
    catch { setCluster(null); }
  }, [hasPg]);

  useEffect(() => {
    loadCluster();
    const t = setInterval(loadCluster, 5000);
    return () => clearInterval(t);
  }, [loadCluster]);

  const toggleSync = async (sync: boolean) => {
    setBusy(true); setAlert(null);
    try {
      const msg = await invoke<string>("set_replication_mode", { sync });
      setAlert({ type: "success", msg: `✓ ${msg}` });
      await loadCluster();
    } catch (err: unknown) {
      setAlert({ type: "error", msg: `✗ ${err instanceof Error ? err.message : String(err)}` });
    } finally { setBusy(false); }
  };

  const doPromote = async () => {
    if (!confirm("Promouvoir cette machine en PRIMARY ?\n\nÀ faire uniquement si le serveur actif est tombé. L'époque de fencing sera incrémentée pour neutraliser l'ancien primary.")) return;
    setBusy(true); setAlert(null);
    try {
      const msg = await invoke<string>("promote_node");
      setAlert({ type: "success", msg });
      await loadCluster();
    } catch (err: unknown) {
      setAlert({ type: "error", msg: `✗ ${err instanceof Error ? err.message : String(err)}` });
    } finally { setBusy(false); }
  };

  const loadEpoch = async () => {
    try { setEpoch(await api.getEpoch()); } catch { setEpoch(null); }
  };

  const promote = async () => {
    if (!confirm("Incrémenter l'époque ? L'ancien nœud actif sera fencé (bloqué).")) return;
    setPromoting(true);
    setAlert(null);
    try {
      const e = await api.promote();
      setEpoch(e);
      setAlert({ type: "success", msg: `✓ Époque promue à ${e.epoch} — nœud actif : ${e.primary_host}` });
    } catch (err: unknown) {
      setAlert({ type: "error", msg: `✗ ${err instanceof Error ? err.message : "Erreur"}` });
    } finally {
      setPromoting(false);
    }
  };

  return (
    <>
      <div className="page-header">
        <div className="page-title">Sécurité & Fencing</div>
        <div className="page-sub">Gestion de l'époque de fencing — protection contre le split-brain</div>
      </div>

      {/* Administration du cluster — uniquement sur les machines avec PostgreSQL */}
      {hasPg && (
        <div className="card" style={{ marginBottom: 24 }}>
          <div style={{ fontWeight: 700, marginBottom: 4 }}>Administration du cluster</div>
          <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 16 }}>
            Bascule et mode de réplication — opérations manuelles, sur décision de la PME.
          </div>

          {alert && <div className={`alert alert-${alert.type}`} style={{ marginBottom: 16 }}>{alert.msg}</div>}

          {cluster ? (
            <>
              <div style={{ display: "flex", gap: 24, marginBottom: 20, flexWrap: "wrap" }}>
                <div>
                  <div className="card-label">Rôle de la base</div>
                  <span className={`badge badge-${cluster.in_recovery ? "yellow" : "green"}`} style={{ marginTop: 6 }}>
                    {cluster.in_recovery ? "◎ Standby (réplique)" : "✓ Primary (source de vérité)"}
                  </span>
                </div>
                <div>
                  <div className="card-label">Réplication</div>
                  <span className={`badge badge-${cluster.sync_enabled ? "green" : "yellow"}`} style={{ marginTop: 6 }}>
                    {cluster.sync_enabled ? "Synchrone" : "Asynchrone"}
                  </span>
                </div>
                <div>
                  <div className="card-label">Standby connecté</div>
                  <div style={{ fontSize: 14, marginTop: 8, fontFamily: "monospace" }}>
                    {cluster.in_recovery ? "—" : (cluster.standby_connected ? `oui (${cluster.sync_state})` : "aucun")}
                  </div>
                </div>
              </div>

              {/* Bouton mode de réplication (uniquement pertinent sur le primary) */}
              {!cluster.in_recovery && (
                <div style={{ marginBottom: 20 }}>
                  <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 8 }}>Mode de réplication</div>
                  <div style={{ display: "flex", gap: 10 }}>
                    <button
                      className={cluster.sync_enabled ? "btn btn-primary" : "btn btn-ghost"}
                      disabled={busy || cluster.sync_enabled}
                      onClick={() => toggleSync(true)}
                    >
                      Passer en SYNCHRONE (zéro perte)
                    </button>
                    <button
                      className={!cluster.sync_enabled ? "btn btn-primary" : "btn btn-ghost"}
                      disabled={busy || !cluster.sync_enabled}
                      onClick={() => toggleSync(false)}
                    >
                      Passer en ASYNCHRONE (rapide)
                    </button>
                  </div>
                </div>
              )}

              {/* Bouton de promotion (uniquement pertinent sur un standby) */}
              {cluster.in_recovery && (
                <div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 8 }}>
                    Bascule (failover) — à utiliser si le nœud actif est tombé
                  </div>
                  <button className="btn btn-danger" disabled={busy} onClick={doPromote}>
                    {busy ? <span className="spin">↻</span> : "⬆ Promouvoir cette machine en PRIMARY"}
                  </button>
                </div>
              )}
            </>
          ) : (
            <div style={{ fontSize: 13, color: "var(--text-muted)" }}>
              PostgreSQL local injoignable — impossible de lire l'état du cluster.
            </div>
          )}
        </div>
      )}

      {/* Explication fencing */}
      <div style={{
        background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius)",
        padding: "20px", marginBottom: 24, lineHeight: 1.7, fontSize: 13
      }}>
        <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 14 }}>Comment fonctionne le fencing</div>
        <p>Chaque écriture vérifie l'<strong>époque</strong> stockée en base avant de committer.
        Si un failover a eu lieu, le nouveau primary incrémente l'époque.
        L'ancien primary détecte alors le mismatch et retourne <code>503 SERVICE_UNAVAILABLE</code> —
        il est <strong>fencé automatiquement</strong>, sans intervention humaine.</p>
        <div style={{ marginTop: 12, display: "flex", gap: 20 }}>
          <div style={{ textAlign: "center" }}>
            <div style={{ fontSize: 24 }}>→</div>
            <div style={{ fontSize: 11, color: "var(--text-muted)" }}>écriture normale<br />epoch local = epoch DB</div>
          </div>
          <div style={{ textAlign: "center" }}>
            <div style={{ fontSize: 24, color: "var(--red)" }}>✗</div>
            <div style={{ fontSize: 11, color: "var(--text-muted)" }}>après failover<br />epoch local &lt; epoch DB → 503</div>
          </div>
        </div>
      </div>

      {/* Époque courante */}
      <div className="card" style={{ marginBottom: 20 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Époque courante</div>
        {alert && <div className={`alert alert-${alert.type}`}>{alert.msg}</div>}
        <div className="form-row">
          <button className="btn btn-ghost" onClick={loadEpoch}>Lire l'époque</button>
          <button className="btn btn-danger" onClick={promote} disabled={promoting}>
            {promoting ? <span className="spin">↻</span> : "⬆ Incrémenter l'époque (failover simulé)"}
          </button>
        </div>
        {epoch && (
          <div style={{ marginTop: 16, display: "flex", gap: 24 }}>
            <div>
              <div className="card-label">Époque</div>
              <div className="card-value">{epoch.epoch}</div>
            </div>
            <div>
              <div className="card-label">Hôte primary</div>
              <div style={{ fontSize: 16, fontFamily: "monospace", marginTop: 8 }}>{epoch.primary_host}</div>
            </div>
          </div>
        )}
      </div>

      {/* Propriétés crypto */}
      <div className="card">
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Stack cryptographique</div>
        <div className="table-wrap">
          <table>
            <thead><tr><th>Primitive</th><th>Rôle</th><th>Implémentation</th></tr></thead>
            <tbody>
              {[
                ["XChaCha20-Poly1305", "Chiffrement données + journal (AEAD)", "libsodium via sodiumoxide"],
                ["X25519",            "Paire de clés par appareil", "libsodium sealed box"],
                ["Argon2id",          "Dérivation depuis passphrase (code récupération)", "libsodium — lent par construction"],
                ["SHA-256",           "Hash chaîné entre entrées journal", "sha2 crate (Rust)"],
                ["SERIALIZABLE PG",   "Isolation transaction anti-survente", "PostgreSQL 18"],
              ].map(([p, r, i]) => (
                <tr key={p}>
                  <td style={{ fontFamily: "monospace", color: "var(--accent2)" }}>{p}</td>
                  <td>{r}</td>
                  <td style={{ color: "var(--text-muted)", fontSize: 12 }}>{i}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
