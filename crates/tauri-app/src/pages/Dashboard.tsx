import { useEffect, useState, useCallback } from "react";
import { api } from "../api";

interface NodeState {
  status: "ok" | "down" | "loading";
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

  // URLs configurées (page Configuration) — affichées telles quelles
  const urls = {
    active:  localStorage.getItem("sovereign_active_url")  ?? "http://192.168.200.1:3000",
    passive: localStorage.getItem("sovereign_passive_url") ?? "http://192.168.200.131:3001",
    relay:   localStorage.getItem("sovereign_relay_url")   ?? "http://192.168.200.132:4000",
  };
  const shortUrl = (u: string) => u.replace(/^https?:\/\//, "");

  const refresh = useCallback(async () => {
    // Nœud actif
    api.healthActive()
      .then(h => setActive({ status: h.trim() === "ok" ? "ok" : "down" }))
      .catch(() => setActive({ status: "down" }));

    // Époque + dernier seq
    api.getEpoch()
      .then(e => setStats(s => ({ ...s, epoch: e.epoch })))
      .catch(() => {});

    api.getJournal(0, 1)
      .then(j => setStats(s => ({ ...s, seq: j.length > 0 ? Math.max(...j.map(x => x.seq)) : 0 })))
      .catch(() => {});

    // Nœud passif
    api.healthPassive()
      .then(h => setPassive({ status: h.includes("passif") ? "ok" : "down", detail: h.trim() }))
      .catch(() => setPassive({ status: "down" }));

    api.syncStatus()
      .then(s => setStats(prev => ({ ...prev, lastSeq: s.last_seq })))
      .catch(() => {});

    // Relais
    api.healthRelay()
      .then(r => {
        setRelay({ status: r.status === "ok" ? "ok" : "down", detail: `${r.blob_count} blobs` });
        setStats(s => ({ ...s, blobs: r.blob_count }));
      })
      .catch(() => setRelay({ status: "down" }));
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

  const NodeCard = ({ title, url, state, sub }: { title: string; url: string; state: NodeState; sub?: string }) => (
    <div className="node-card">
      <div className="node-name">
        <span className={`dot dot-${state.status === "loading" ? "yellow" : state.status === "ok" ? "green" : "red"}`} />
        {title}
      </div>
      <div className="node-url">{url}</div>
      <span className={`badge badge-${state.status === "loading" ? "yellow" : state.status === "ok" ? "green" : "red"}`}>
        {state.status === "loading" ? "⟳ Vérification…" : state.status === "ok" ? "En ligne" : "Hors ligne"}
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
