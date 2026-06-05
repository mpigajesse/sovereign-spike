import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import Startup  from "./pages/Startup";
import Install  from "./pages/Install";
import Dashboard from "./pages/Dashboard";
import Stock     from "./pages/Stock";
import Journal   from "./pages/Journal";
import Securite  from "./pages/Securite";
import Settings  from "./pages/Settings";

type Page = "dashboard" | "stock" | "journal" | "securite" | "settings";

const NAV: { id: Page; label: string; icon: string }[] = [
  { id: "dashboard", label: "Tableau de bord",  icon: "◈" },
  { id: "stock",     label: "Gestion du stock", icon: "⊟" },
  { id: "journal",   label: "Journal chiffré",  icon: "⊞" },
  { id: "securite",  label: "Sécurité",          icon: "⊡" },
  { id: "settings",  label: "Configuration",     icon: "⚙" },
];

type AppState = "install" | "startup" | "solo-restart" | "primary-restart" | "ready";

function getInitialState(): AppState {
  // Premier lancement : afficher l'assistant d'installation
  if (!localStorage.getItem("sovereign_installed")) return "install";
  const role = localStorage.getItem("sovereign_role");
  // Relancement en mode solo : il faut redémarrer le nœud solo embarqué
  if (role === "solo") return "solo-restart";
  // Relancement en mode primary : le nœud actif (process enfant) est mort
  // avec l'app précédente — il faut le relancer.
  if (role === "primary") return "primary-restart";
  return "startup";
}

export default function App() {
  const [appState, setAppState] = useState<AppState>(getInitialState);
  const [page,     setPage]     = useState<Page>("dashboard");
  const [nodeMode, setNodeMode] = useState<"local" | "remote" | "standby" | "solo">("remote");

  // Redémarrage automatique du nœud solo au relancement de l'app
  useEffect(() => {
    if (appState !== "solo-restart") return;
    const dek = localStorage.getItem("sovereign_dek")
      ?? "174835f0e063680d4b4652c7edf9472a1db0626388dbbe4342d84a7c9bce035b";
    invoke("start_solo_node", { dekHex: dek })
      .catch(() => { /* déjà démarré ou mode navigateur */ })
      .finally(() => { setNodeMode("solo"); setAppState("ready"); });
  }, [appState]);

  // Redémarrage automatique du nœud actif (rôle primary) au relancement.
  // Le binaire lit DATABASE_URL/DEK par défaut côté Rust (cf. start_active_node).
  useEffect(() => {
    if (appState !== "primary-restart") return;
    invoke("start_active_node", { dbUrl: "", dekHex: "", relayUrl: "", relayKey: "" })
      .catch(() => { /* déjà démarré, PG indisponible, ou mode navigateur */ })
      .finally(() => {
        localStorage.setItem("sovereign_active_url", "http://127.0.0.1:3000");
        setNodeMode("local");
        setAppState("ready");
      });
  }, [appState]);

  const handleInstallComplete = () => {
    const role = localStorage.getItem("sovereign_role") ?? "client";
    if (role === "solo") {
      setNodeMode("solo");
      setAppState("ready"); // Le nœud solo a déjà été démarré par l'assistant
    } else if (role === "standby") {
      setNodeMode("standby");
      setAppState("ready");
    } else {
      setAppState("startup");
    }
  };

  const handleStartupReady = (activeUrl: string) => {
    if (activeUrl.includes("127.0.0.1") || activeUrl.includes("localhost")) {
      localStorage.setItem("sovereign_active_url", activeUrl);
      setNodeMode("local");
    } else {
      if (!localStorage.getItem("sovereign_active_url")) {
        localStorage.setItem("sovereign_active_url", activeUrl);
      }
      setNodeMode("remote");
    }
    setAppState("ready");
  };

  // ── Écrans de démarrage ──────────────────────────────────────────────────
  if (appState === "install")  return <Install  onComplete={handleInstallComplete} />;
  if (appState === "startup")  return <Startup  onReady={handleStartupReady} />;
  if (appState === "solo-restart") return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)", gap: 20 }}>
      <div style={{ fontSize: 48 }}>⬢</div>
      <div style={{ fontSize: 18, fontWeight: 700 }}>Démarrage du nœud solo…</div>
      <div style={{ fontSize: 13, color: "var(--text-muted)" }}>SQLite local — sans serveur</div>
      <div style={{ fontSize: 32 }}><span className="spin">↻</span></div>
    </div>
  );
  if (appState === "primary-restart") return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)", gap: 20 }}>
      <div style={{ fontSize: 48 }}>◉</div>
      <div style={{ fontSize: 18, fontWeight: 700 }}>Démarrage du nœud actif…</div>
      <div style={{ fontSize: 13, color: "var(--text-muted)" }}>PostgreSQL — source de vérité</div>
      <div style={{ fontSize: 32 }}><span className="spin">↻</span></div>
    </div>
  );

  // ── Application principale ───────────────────────────────────────────────
  const roleBadge = nodeMode === "solo"    ? { label: "⬢ PME Solo",          color: "green" }
                  : nodeMode === "local"   ? { label: "✓ Actif (primary)",  color: "green" }
                  : nodeMode === "standby" ? { label: "◎ Standby",           color: "yellow" }
                  :                          { label: "○ Client",             color: "yellow" };

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-logo">
          ⬡ Sovereign
          <span>Data Agent v0.1</span>
        </div>

        {NAV.map(n => (
          <div
            key={n.id}
            className={`nav-item ${page === n.id ? "active" : ""}`}
            onClick={() => setPage(n.id)}
          >
            <span>{n.icon}</span>
            {n.label}
          </div>
        ))}

        <div style={{ flex: 1 }} />

        {/* Indicateur de rôle */}
        <div style={{ padding: "12px 20px", borderTop: "1px solid var(--border)" }}>
          <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 4 }}>Rôle de cette machine</div>
          <span className={`badge badge-${roleBadge.color}`} style={{ fontSize: 11 }}>
            {roleBadge.label}
          </span>
        </div>

        {/* Réinstaller */}
        <div
          style={{ padding: "8px 20px 16px", fontSize: 11, color: "var(--text-muted)", cursor: "pointer" }}
          onClick={() => {
            localStorage.removeItem("sovereign_installed");
            window.location.reload();
          }}
        >
          ↺ Reconfigurer l'installation
        </div>

        <div style={{ padding: "0 20px 16px", fontSize: 11, color: "var(--text-muted)", lineHeight: 1.5 }}>
          EIGSI × AL BARAA<br />
          Jesse MPIGA-ODOUMBA<br />
          Promo 2026
        </div>
      </aside>

      <main className="main">
        {page === "dashboard" && <Dashboard />}
        {page === "stock"     && <Stock />}
        {page === "journal"   && <Journal />}
        {page === "securite"  && <Securite />}
        {page === "settings"  && <Settings />}
      </main>
    </div>
  );
}
