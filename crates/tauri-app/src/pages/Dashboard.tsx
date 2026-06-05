import { useEffect, useState, useCallback } from "react";
import { api } from "../api";

interface NodeState {
  status: "ok" | "down" | "loading" | "unset";
  detail?: string;
}

interface Stats {
  seq:    number | null;
  epoch:  number | null;
  lastSeq: number | null;
  blobs:  number | null;
}

export default function Dashboard() {
  const [active,  setActive]  = useState<NodeState>({ status: "loading" });
  const [passive, setPassive] = useState<NodeState>({ status: "loading" });
  const [relay,   setRelay]   = useState<NodeState>({ status: "loading" });
  const [stats,   setStats]   = useState<Stats>({ seq: null, epoch: null, lastSeq: null, blobs: null });

  // URLs configurées (page Configuration) — vides tant que non saisies
  const urls = {
    active:  localStorage.getItem("sovereign_active_url")  ?? "",
    passive: localStorage.getItem("sovereign_passive_url") ?? "",
    relay:   localStorage.getItem("sovereign_relay_url")   ?? "",
  };
  const shortUrl = (u: string) => u ? u.replace(/^https?:\/\//, "") : "non configuré";

  const refresh = useCallback(async () => {
    // On relit les URLs à chaque tick (l'utilisateur peut les changer sans reload)
    const cfg = {
      active:  localStorage.getItem("sovereign_active_url")  ?? "",
      passive: localStorage.getItem("sovereign_passive_url") ?? "",
      relay:   localStorage.getItem("sovereign_relay_url")   ?? "",
    };

    // Nœud actif — sauté si non configuré
    if (!cfg.active) {
      setActive({ status: "unset" });
    } else {
      api.healthActive()
        .then(h => setActive({ status: h.trim() === "ok" ? "ok" : "down" }))
        .catch(() => setActive({ status: "down" }));

      api.getEpoch()
        .then(e => setStats(s => ({ ...s, epoch: e.epoch })))
        .catch(() => {});

      api.getJournal(0, 1)
        .then(j => setStats(s => ({ ...s, seq: j.length > 0 ? Math.max(...j.map(x => x.seq)) : 0 })))
        .catch(() => {});
    }

    // Nœud passif — sauté si non configuré
    if (!cfg.passive) {
      setPassive({ status: "unset" });
    } else {
      api.healthPassive()
        .then(h => setPassive({ status: h.includes("passif") ? "ok" : "down", detail: h.trim() }))
        .catch(() => setPassive({ status: "down" }));

      api.syncStatus()
        .then(s => setStats(prev => ({ ...prev, lastSeq: s.last_seq })))
        .catch(() => {});
    }

    // Relais — sauté si non configuré
    if (!cfg.relay) {
      setRelay({ status: "unset" });
    } else {
      api.healthRelay()
        .then(r => {
          setRelay({ status: r.status === "ok" ? "ok" : "down", detail: `${r.blob_count} blobs` });
          setStats(s => ({ ...s, blobs: r.blob_count }));
        })
        .catch(() => setRelay({ status: "down" }));
    }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

  const dotColor = (s: NodeState["status"]) =>
    s === "ok" ? "green" : s === "down" ? "red" : "yellow"; // loading + unset => jaune/gris
  const badgeLabel = (s: NodeState["status"]) =>
    s === "loading" ? "⟳ Vérification…" : s === "ok" ? "En ligne" : s === "unset" ? "Non configuré" : "Hors ligne";

  const NodeCard = ({ title, url, state, sub }: { title: string; url: string; state: NodeState; sub?: string }) => (
    <div className="node-card">
      <div className="node-name">
        <span className={`dot dot-${dotColor(state.status)}`} />
        {title}
      </div>
      <div className="node-url">{url}</div>
      <span className={`badge badge-${dotColor(state.status)}`}>
        {badgeLabel(state.status)}
      </span>
      {sub && <div className="node-stat">{sub}</div>}
    </div>
  );

  return (
    <>
      <div className="page-header">
        <div className="page-title">Tableau de bord</div>
        <div className="page-sub">État du cluster souverain en temps réel — actualisation toutes les 5s</div>
      </div>

      {/* Statut des 3 nœuds — URLs lues depuis la configuration */}
      <div className="nodes-grid">
        <NodeCard title="Nœud Actif"  url={shortUrl(urls.active)}  state={active}  sub={stats.epoch !== null ? `Époque ${stats.epoch}` : undefined} />
        <NodeCard title="Nœud Passif" url={shortUrl(urls.passive)} state={passive} sub={stats.lastSeq !== null ? `last_seq = ${stats.lastSeq}` : undefined} />
        <NodeCard title="Relais"      url={shortUrl(urls.relay)}   state={relay}   sub={stats.blobs !== null ? `${stats.blobs} blobs stockés` : undefined} />
      </div>

      {/* Métriques */}
      <div className="cards">
        <div className="card">
          <div className="card-label">Opérations journalisées</div>
          <div className="card-value">{stats.seq ?? "—"}</div>
          <div className="card-sub">séquence courante (actif)</div>
        </div>
        <div className="card">
          <div className="card-label">Époque de fencing</div>
          <div className="card-value">{stats.epoch ?? "—"}</div>
          <div className="card-sub">protection anti-split-brain</div>
        </div>
        <div className="card">
          <div className="card-label">Sync passif</div>
          <div className="card-value">{stats.lastSeq ?? "—"}</div>
          <div className="card-sub">dernier seq répliqué</div>
        </div>
        <div className="card">
          <div className="card-label">Blobs relais</div>
          <div className="card-value">{stats.blobs ?? "—"}</div>
          <div className="card-sub">stockés (opaques, chiffrés)</div>
        </div>
      </div>

      {/* Propriétés de sécurité */}
      <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: "20px" }}>
        <div style={{ fontWeight: 700, marginBottom: 14, fontSize: 14 }}>Propriétés de souveraineté actives</div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          {[
            ["XChaCha20-Poly1305", "Chiffrement authentifié AEAD"],
            ["Hash chaîné SHA-256", "Intégrité journal inviolable"],
            ["Fencing par époque", "Anti-split-brain garanti"],
            ["Relais zéro-knowledge", "Éditeur aveugle par construction"],
            ["Réplication WAL", "PostgreSQL streaming synchrone"],
            ["DEK sealed box X25519", "Distribution clé sans exposition"],
          ].map(([k, v]) => (
            <div key={k} style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
              <span style={{ color: "var(--green)", marginTop: 1 }}>✓</span>
              <div>
                <div style={{ fontSize: 13, fontWeight: 600 }}>{k}</div>
                <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{v}</div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
