import { useState } from "react";
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

type AppState = "install" | "startup" | "ready";

function getInitialState(): AppState {
  // Premier lancement : afficher l'assistant d'installation
  if (!localStorage.getItem("sovereign_installed")) return "install";
  return "startup";
}

export default function App() {
  const [appState, setAppState] = useState<AppState>(getInitialState);
  const [page,     setPage]     = useState<Page>("dashboard");
  const [nodeMode, setNodeMode] = useState<"local" | "remote" | "standby">("remote");

  const handleInstallComplete = () => {
    const role = localStorage.getItem("sovereign_role") ?? "client";
    if (role === "standby") {
      setNodeMode("standby");
      setAppState("ready"); // Standby n'a pas besoin de démarrer le nœud actif
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

  // ── Application principale ───────────────────────────────────────────────
  const roleBadge = nodeMode === "local"   ? { label: "✓ Actif (primary)",  color: "green" }
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
