import { useState } from "react";
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
  const [page, setPage] = useState<Page>("dashboard");

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
        <div style={{ padding: "0 20px", fontSize: 11, color: "var(--text-muted)", lineHeight: 1.5 }}>
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
