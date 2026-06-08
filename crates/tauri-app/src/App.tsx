import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import Startup  from "./pages/Startup";
import Install  from "./pages/Install";
import Dashboard from "./pages/Dashboard";
import DashboardMetier from "./pages/DashboardMetier";
import Stock     from "./pages/Stock";
import Produits  from "./pages/Produits";
import Clients   from "./pages/Clients";
import Journal   from "./pages/Journal";
import Securite  from "./pages/Securite";
import Parc      from "./pages/Parc";
import Settings  from "./pages/Settings";

type Page = "dashboard" | "produits" | "clients" | "stock" | "journal" | "securite" | "parc" | "settings";

// Profil d'utilisateur du poste — distinct du RÔLE technique du nœud
// (sovereign_role = primary/standby/relais, qui décrit la réplication).
// "admin"  : a CRÉÉ le compte tenant (rôle primary à l'installation) → pilote
//            le cluster (parc, sécurité, journal, configuration).
// "metier" : a REJOINT un cluster existant (standby/relais, enrôlement) →
//            n'utilise que le logiciel métier ; le cluster lui reste invisible,
//            conformément au principe "zéro administrateur" du cadrage
//            (appartenir au cluster = avoir été enrôlé, pas administrer).
type UserProfile = "admin" | "metier";

function getUserProfile(): UserProfile {
  return localStorage.getItem("sovereign_role") === "primary" ? "admin" : "metier";
}

// Section d'appartenance de l'item — sert à regrouper visuellement le menu
// de l'admin-tenant en deux blocs distincts ("logiciel métier" que tout le
// monde utilise, vs "pilotage du cluster" réservé au créateur du compte) :
// la séparation devient visible, pas seulement un filtre silencieux.
type NavSection = "metier" | "cluster";

const NAV: { id: Page; label: string; icon: string; section: NavSection }[] = [
  { id: "dashboard", label: "Tableau de bord",  icon: "◈", section: "metier"  },
  { id: "produits",  label: "Produits",          icon: "⊠", section: "metier"  },
  { id: "clients",   label: "Clients",           icon: "☻", section: "metier"  },
  { id: "stock",     label: "Gestion du stock", icon: "⊟", section: "metier"  },
  { id: "journal",   label: "Journal chiffré",  icon: "⊞", section: "cluster" },
  { id: "securite",  label: "Sécurité",          icon: "⊡", section: "cluster" },
  { id: "parc",      label: "Gestion du parc",   icon: "⊕", section: "cluster" },
  { id: "settings",  label: "Configuration",     icon: "⚙", section: "cluster" },
];

const SECTION_LABEL: Record<NavSection, string> = {
  metier:  "Logiciel métier",
  cluster: "Pilotage du cluster",
};

type AppState = "install" | "startup" | "primary-restart" | "relais-restart" | "ready";

function getInitialState(): AppState {
  // Premier lancement : afficher l'assistant d'installation
  if (!localStorage.getItem("sovereign_installed")) return "install";
  const role = localStorage.getItem("sovereign_role");
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
  const [nodeMode, setNodeMode] = useState<"local" | "remote" | "standby">("remote");
  const [version,  setVersion]  = useState("0.1.12");

  // Récupère la version réelle du bundle Tauri (source de vérité = tauri.conf.json)
  useEffect(() => {
    getVersion().then(setVersion).catch(() => { /* mode navigateur : garde le fallback */ });
  }, []);

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
    if (role === "standby") {
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
  const roleBadge = storedRole === "primary" ? { label: "✓ Actif (primary)",  color: "green" }
                  : storedRole === "standby" ? { label: "◎ Standby",           color: "yellow" }
                  : storedRole === "relais"  ? { label: "◇ Relais aveugle",    color: "green" }
                  :                            { label: "○ Client",            color: "yellow" };

  // Filtrage + regroupement du menu selon le profil : un poste enrôlé
  // (métier) ne voit que la section "Logiciel métier" — le cluster lui reste
  // invisible. L'admin-tenant (créateur du compte, primary) voit les deux
  // sections, regroupées sous des en-têtes visibles : la séparation devient
  // explicite à l'écran, pas seulement un filtre silencieux. Cf. UserProfile.
  const profile = getUserProfile();
  const visibleSections: NavSection[] = profile === "admin" ? ["metier", "cluster"] : ["metier"];
  const navGroups = visibleSections.map(section => ({
    section,
    items: NAV.filter(n => n.section === section),
  }));

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-logo">
          ⬡ Sovereign
          <span>Data Agent v{version}</span>
        </div>

        {navGroups.map(group => (
          <div key={group.section}>
            {/* En-tête affiché seulement quand l'admin voit plusieurs
                sections — pour un poste métier (une seule section), un
                en-tête répétant "Logiciel métier" n'apporterait rien. */}
            {navGroups.length > 1 && (
              <div className="nav-section-label" style={{ padding: "12px 20px 4px", fontSize: 11, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--text-muted)" }}>
                {SECTION_LABEL[group.section]}
              </div>
            )}
            {group.items.map(n => (
              <div
                key={n.id}
                className={`nav-item ${page === n.id ? "active" : ""}`}
                onClick={() => setPage(n.id)}
              >
                <span>{n.icon}</span>
                {n.label}
              </div>
            ))}
          </div>
        ))}

        <div style={{ flex: 1 }} />

        {/* Compte entreprise (tenant) — visible par tous les profils, c'est
            une info métier légitime. Le rôle de la machine et l'action de
            réinstallation, eux, exposeraient l'existence du cluster à un
            poste métier (ex. badge "Standby") : réservés au profil admin. */}
        <div style={{ padding: "12px 20px", borderTop: "1px solid var(--border)" }}>
          {localStorage.getItem("sovereign_tenant_nom") && (
            <div style={{ marginBottom: profile === "admin" ? 12 : 0 }}>
              <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 4 }}>Compte</div>
              <div style={{ fontSize: 13, fontWeight: 700, color: "var(--accent2)" }}>
                ⬡ {localStorage.getItem("sovereign_tenant_nom")}
              </div>
            </div>
          )}
          {profile === "admin" && (
            <>
              <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 4 }}>Rôle de cette machine</div>
              <span className={`badge badge-${roleBadge.color}`} style={{ fontSize: 11 }}>
                {roleBadge.label}
              </span>
            </>
          )}
        </div>

        {/* Réinstaller — remise à zéro COMPLÈTE de la config locale, sinon un
            rôle/URL résiduel survit et fausse le tableau de bord (ex. badge
            "primary" coincé). On garde la DEK pour ne pas perdre l'accès aux
            données chiffrées déjà répliquées localement. Réservé à l'admin :
            une réinstallation touche le rôle cluster, invisible côté métier. */}
        {profile === "admin" && (
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
        )}

        <div style={{ padding: "0 20px 16px", fontSize: 11, color: "var(--text-muted)", lineHeight: 1.5 }}>
          EIGSI × AL BARAA<br />
          Jesse MPIGA-ODOUMBA<br />
          Promo 2026
        </div>
      </aside>

      <main className="main">
        {/* Le tableau de bord lui-même suit le profil : l'admin-tenant pilote
            le cluster (NodeCards, fencing, sync, blobs relais) tandis que le
            poste métier ne voit qu'un résumé de son activité commerciale —
            le cluster reste invisible, cf. UserProfile et navGroups. */}
        {page === "dashboard" && (profile === "admin" ? <Dashboard /> : <DashboardMetier />)}
        {page === "produits"  && <Produits />}
        {page === "clients"   && <Clients />}
        {page === "stock"     && <Stock />}
        {page === "journal"   && <Journal />}
        {page === "securite"  && <Securite />}
        {page === "parc"      && <Parc />}
        {page === "settings"  && <Settings />}
      </main>
    </div>
  );
}
