import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
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

type AppState = "install" | "startup" | "solo-restart" | "primary-restart" | "relais-restart" | "ready";

function getInitialState(): AppState {
  // Premier lancement : afficher l'assistant d'installation
  if (!localStorage.getItem("sovereign_installed")) return "install";
  const role = localStorage.getItem("sovereign_role");
  // Relancement en mode solo : il faut redémarrer le nœud solo embarqué
  if (role === "solo") return "solo-restart";
  // Relancement en mode primary : le nœud actif (process enfant) est mort
  // avec l'app précédente — il faut le relancer.
  if (role === "primary") return "primary-restart";
  // Relancement en mode relais : le binaire relais (process enfant) est mort
  // avec l'app précédente — il faut le relancer.
  if (role === "relais") return "relais-restart";
  return "startup";
}

export default function App() {
  const [appState, setAppState] = useState<AppState>(getInitialState);
  const [page,     setPage]     = useState<Page>("dashboard");
  const [nodeMode, setNodeMode] = useState<"local" | "remote" | "standby" | "solo">("remote");
  const [version,  setVersion]  = useState("0.1.7");

  // Récupère la version réelle du bundle Tauri (source de vérité = tauri.conf.json)
  useEffect(() => {
    getVersion().then(setVersion).catch(() => { /* mode navigateur : garde le fallback */ });
  }, []);

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
    // URL relais configurée (Configuration) prioritaire ; sinon défaut côté Rust
    // (relais co-localisé http://127.0.0.1:4000) → push automatique permanent.
    const relayUrl = localStorage.getItem("sovereign_relay_url") ?? "";
    invoke<{ mode: string }>("start_active_node", { dbUrl: "", dekHex: "", relayUrl, relayKey: "" })
      .then(status => {
        // Le nœud n'est "local" que s'il a vraiment démarré (PostgreSQL présent).
        if (status?.mode === "local") {
          // Ne pas écraser l'adresse LAN fixée à l'installation (ex. 192.168.200.1).
          if (!localStorage.getItem("sovereign_active_url")) {
            localStorage.setItem("sovereign_active_url", "http://127.0.0.1:3000");
          }
          setNodeMode("local");
        } else {
          setNodeMode("remote"); // PG indisponible : on ne se prétend pas primary
        }
      })
      .catch(() => setNodeMode("remote") /* mode navigateur ou erreur */)
      .finally(() => setAppState("ready"));
  }, [appState]);

  // Redémarrage automatique du relais aveugle (rôle relais) au relancement.
  useEffect(() => {
    if (appState !== "relais-restart") return;
    invoke("start_relay_node")
      .catch(() => { /* déjà démarré ou mode navigateur */ })
      .finally(() => { setNodeMode("remote"); setAppState("ready"); });
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
  if (appState === "relais-restart") return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)", gap: 20 }}>
      <div style={{ fontSize: 48 }}>◇</div>
      <div style={{ fontSize: 18, fontWeight: 700 }}>Démarrage du relais aveugle…</div>
      <div style={{ fontSize: 13, color: "var(--text-muted)" }}>Zero-knowledge — stockage de blobs chiffrés</div>
      <div style={{ fontSize: 32 }}><span className="spin">↻</span></div>
    </div>
  );

  // ── Application principale ───────────────────────────────────────────────
  // Le badge reflète le RÔLE choisi à l'installation (source de vérité unique
  // = sovereign_role), et non l'état d'exécution nodeMode qui peut diverger.
  const storedRole = localStorage.getItem("sovereign_role");
  const roleBadge = storedRole === "solo"    ? { label: "⬢ PME Solo",         color: "green" }
                  : storedRole === "primary" ? { label: "✓ Actif (primary)",  color: "green" }
                  : storedRole === "standby" ? { label: "◎ Standby",           color: "yellow" }
                  : storedRole === "relais"  ? { label: "◇ Relais aveugle",    color: "green" }
                  :                            { label: "○ Client",            color: "yellow" };

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-logo">
          ⬡ Sovereign
          <span>Data Agent v{version}</span>
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

        {/* Réinstaller — remise à zéro COMPLÈTE de la config locale, sinon un
            rôle/URL résiduel survit et fausse le tableau de bord (ex. badge
            "primary" coincé). On garde la DEK pour ne pas perdre l'accès aux
            données chiffrées en mode solo. */}
        <div
          style={{ padding: "8px 20px 16px", fontSize: 11, color: "var(--text-muted)", cursor: "pointer" }}
          onClick={() => {
            ["sovereign_installed", "sovereign_role",
             "sovereign_active_url", "sovereign_passive_url", "sovereign_relay_url"]
              .forEach(k => localStorage.removeItem(k));
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
