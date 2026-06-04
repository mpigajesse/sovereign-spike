import { useState } from "react";
import Startup from "./pages/Startup";
import Dashboard from "./pages/Dashboard";
import Stock from "./pages/Stock";
import Journal from "./pages/Journal";
import Securite from "./pages/Securite";
import Settings from "./pages/Settings";

type Page = "dashboard" | "stock" | "journal" | "securite" | "settings";

const NAV: { id: Page; label: string; icon: string }[] = [
  { id: "dashboard", label: "Tableau de bord",  icon: "◈" },
  { id: "stock",     label: "Gestion du stock", icon: "⊟" },
  { id: "journal",   label: "Journal chiffré",  icon: "⊞" },
  { id: "securite",  label: "Sécurité",          icon: "⊡" },
  { id: "settings",  label: "Configuration",     icon: "⚙" },
];

export default function App() {
  const [ready,    setReady]    = useState(false);
  const [page,     setPage]     = useState<Page>("dashboard");
  const [nodeMode, setNodeMode] = useState<"local" | "remote">("remote");

  const handleReady = (activeUrl: string) => {
    // Sauvegarder l'URL active dans localStorage si c'est local
    if (activeUrl.includes("127.0.0.1") || activeUrl.includes("localhost")) {
      localStorage.setItem("sovereign_active_url", activeUrl);
      setNodeMode("local");
    } else {
      // Mode remote — garder l'URL configurée ou utiliser celle reçue
      if (!localStorage.getItem("sovereign_active_url")) {
        localStorage.setItem("sovereign_active_url", activeUrl);
      }
      setNodeMode("remote");
    }
    setReady(true);
  };

  if (!ready) {
    return <Startup onReady={handleReady} />;
  }

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
        {/* Indicateur de mode */}
        <div style={{ padding: "12px 20px", borderTop: "1px solid var(--border)" }}>
          <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 4 }}>Mode</div>
          <span className={`badge badge-${nodeMode === "local" ? "green" : "yellow"}`} style={{ fontSize: 11 }}>
            {nodeMode === "local" ? "✓ Local (primary)" : "⟳ Client (distant)"}
          </span>
        </div>
        <div style={{ padding: "8px 20px 16px", fontSize: 11, color: "var(--text-muted)", lineHeight: 1.5 }}>
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
