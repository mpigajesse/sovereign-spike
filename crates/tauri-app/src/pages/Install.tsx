// Assistant d'installation — affiché au premier lancement.
// L'utilisateur choisit le rôle de cette machine dans le cluster souverain.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Role = "primary" | "standby" | "client";

interface Props {
  onComplete: () => void;
}

export default function Install({ onComplete }: Props) {
  const [step,       setStep]       = useState<"role" | "config" | "installing" | "done">("role");
  const [role,       setRole]       = useState<Role | null>(null);
  const [primaryUrl, setPrimaryUrl] = useState("http://192.168.200.1:3000");
  const [primaryPg,  setPrimaryPg]  = useState("192.168.200.1");
  const [log,        setLog]        = useState<string[]>([]);
  const [error,      setError]      = useState<string | null>(null);

  const addLog = (msg: string) => setLog(l => [...l, msg]);

  const startInstall = async () => {
    setStep("installing");
    setError(null);
    setLog([]);

    try {
      if (role === "primary") {
        addLog("Vérification de PostgreSQL local...");
        const pgOk = await invoke<boolean>("check_pg_local");
        if (!pgOk) {
          setError("PostgreSQL non détecté sur ce poste. Installez PostgreSQL 18 avant de continuer.");
          setStep("config");
          return;
        }
        addLog("PostgreSQL détecté ✓");
        addLog("Démarrage du nœud actif...");
        await invoke("start_active_node", {
          dbUrl: "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active",
          dekHex: "",
          relayUrl: "",
          relayKey: "",
        });
        localStorage.setItem("sovereign_role", "primary");
        localStorage.setItem("sovereign_active_url", "http://127.0.0.1:3000");
        addLog("Nœud actif démarré ✓");

      } else if (role === "standby") {
        addLog("Vérification de PostgreSQL local...");
        const pgOk = await invoke<boolean>("check_pg_local");
        if (!pgOk) {
          setError("PostgreSQL non détecté. Installez PostgreSQL 18 avant de continuer.");
          setStep("config");
          return;
        }
        addLog("PostgreSQL détecté ✓");
        addLog("Lancement du script de configuration standby...");
        const result = await invoke<string>("run_standby_setup", { primaryIp: primaryPg });
        addLog(result);
        localStorage.setItem("sovereign_role", "standby");
        localStorage.setItem("sovereign_active_url", primaryUrl);

      } else {
        // Client — pas d'installation, juste la config URL
        localStorage.setItem("sovereign_role", "client");
        localStorage.setItem("sovereign_active_url", primaryUrl);
        addLog("Configuration client enregistrée ✓");
      }

      localStorage.setItem("sovereign_installed", "1");
      addLog("Installation terminée !");
      setStep("done");

    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setStep("config");
    }
  };

  // ── Écran de choix du rôle ─────────────────────────────────────────────────
  if (step === "role") {
    return (
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)", padding: 40 }}>
        <div style={{ fontSize: 48, marginBottom: 16 }}>⬡</div>
        <div style={{ fontSize: 26, fontWeight: 800, color: "var(--accent2)", marginBottom: 8 }}>Sovereign Data Agent</div>
        <div style={{ color: "var(--text-muted)", marginBottom: 48, fontSize: 14 }}>Première installation — choisissez le rôle de cette machine</div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 20, maxWidth: 900, width: "100%" }}>
          {[
            {
              id: "primary" as Role,
              icon: "◉",
              title: "Nœud Actif",
              subtitle: "Serveur principal",
              desc: "Cette machine héberge PostgreSQL et est la source de vérité. Toutes les écritures passent ici.",
              require: "PostgreSQL 18 requis",
              color: "var(--green)",
            },
            {
              id: "standby" as Role,
              icon: "◎",
              title: "Nœud Standby",
              subtitle: "Serveur de secours",
              desc: "Réplique le nœud actif. Peut prendre le relais automatiquement si le nœud actif tombe (failover).",
              require: "PostgreSQL 18 requis",
              color: "var(--accent2)",
            },
            {
              id: "client" as Role,
              icon: "○",
              title: "Poste Client",
              subtitle: "Poste opérateur",
              desc: "Se connecte au nœud actif via le réseau. Aucune installation de base de données requise.",
              require: "Aucun prérequis",
              color: "var(--text-muted)",
            },
          ].map(r => (
            <div
              key={r.id}
              onClick={() => setRole(r.id)}
              style={{
                background: "var(--surface)",
                border: `2px solid ${role === r.id ? r.color : "var(--border)"}`,
                borderRadius: 12,
                padding: 24,
                cursor: "pointer",
                transition: "all 0.15s",
                opacity: role && role !== r.id ? 0.6 : 1,
              }}
            >
              <div style={{ fontSize: 32, color: r.color, marginBottom: 12 }}>{r.icon}</div>
              <div style={{ fontWeight: 700, fontSize: 16, marginBottom: 4 }}>{r.title}</div>
              <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 12 }}>{r.subtitle}</div>
              <div style={{ fontSize: 13, color: "var(--text)", lineHeight: 1.6, marginBottom: 16 }}>{r.desc}</div>
              <div style={{ fontSize: 11, color: r.color, fontWeight: 600 }}>{r.require}</div>
            </div>
          ))}
        </div>

        <button
          className="btn btn-primary"
          style={{ marginTop: 32, minWidth: 200, fontSize: 15, padding: "12px 32px" }}
          disabled={!role}
          onClick={() => setStep("config")}
        >
          Continuer →
        </button>
      </div>
    );
  }

  // ── Écran de configuration ─────────────────────────────────────────────────
  if (step === "config") {
    return (
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)", padding: 40 }}>
        <div style={{ maxWidth: 520, width: "100%" }}>
          <div style={{ fontSize: 22, fontWeight: 700, marginBottom: 8 }}>
            {role === "primary" ? "⬡ Configuration du nœud actif" :
             role === "standby" ? "◎ Configuration du nœud standby" :
             "○ Configuration du poste client"}
          </div>
          <div style={{ color: "var(--text-muted)", marginBottom: 32, fontSize: 13 }}>
            {role === "primary" ? "Cette machine sera le serveur principal de la PME." :
             role === "standby" ? "Cette machine répliquera le nœud actif et pourra prendre le relais." :
             "Cette machine se connectera au nœud actif via le réseau."}
          </div>

          {error && <div className="alert alert-error" style={{ marginBottom: 20 }}>{error}</div>}

          <div className="card">
            {(role === "standby" || role === "client") && (
              <>
                <div style={{ marginBottom: 20 }}>
                  <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                    URL du nœud actif (API HTTP)
                  </label>
                  <input
                    value={primaryUrl}
                    onChange={e => setPrimaryUrl(e.target.value)}
                    style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                    placeholder="http://192.168.200.1:3000"
                  />
                  <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4 }}>
                    IP de la machine Windows 11 qui héberge le nœud actif
                  </div>
                </div>

                {role === "standby" && (
                  <div style={{ marginBottom: 20 }}>
                    <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                      IP PostgreSQL du primary (réplication)
                    </label>
                    <input
                      value={primaryPg}
                      onChange={e => setPrimaryPg(e.target.value)}
                      style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                      placeholder="192.168.200.1"
                    />
                    <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4 }}>
                      Utilisé par pg_basebackup pour cloner le primary
                    </div>
                  </div>
                )}
              </>
            )}

            {role === "primary" && (
              <div style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.7 }}>
                ✓ PostgreSQL sera configuré automatiquement sur <code>127.0.0.1:5432</code><br />
                ✓ La DEK sera générée et sauvegardée dans <code>shared.env</code><br />
                ✓ Le nœud actif démarrera automatiquement au lancement
              </div>
            )}
          </div>

          <div style={{ display: "flex", gap: 12, marginTop: 24 }}>
            <button className="btn btn-ghost" onClick={() => setStep("role")}>← Retour</button>
            <button className="btn btn-primary" style={{ flex: 1 }} onClick={startInstall}>
              {role === "standby" ? "Configurer le standby" :
               role === "primary" ? "Démarrer le nœud actif" :
               "Enregistrer et continuer"}
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── Installation en cours ─────────────────────────────────────────────────
  if (step === "installing") {
    return (
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)" }}>
        <div style={{ fontSize: 48 }}><span className="spin">↻</span></div>
        <div style={{ fontSize: 18, fontWeight: 700, marginTop: 16, marginBottom: 24 }}>Installation en cours…</div>
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 10, padding: 20, minWidth: 400, maxWidth: 600 }}>
          {log.map((l, i) => (
            <div key={i} style={{ fontSize: 12, fontFamily: "monospace", color: "var(--text-muted)", marginBottom: 4 }}>
              <span style={{ color: "var(--green)" }}>›</span> {l}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // ── Terminé ───────────────────────────────────────────────────────────────
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", minHeight: "100vh", background: "var(--bg)" }}>
      <div style={{ fontSize: 64, color: "var(--green)" }}>✓</div>
      <div style={{ fontSize: 22, fontWeight: 700, marginTop: 16, marginBottom: 8 }}>Installation terminée</div>
      <div style={{ color: "var(--text-muted)", marginBottom: 32, fontSize: 14 }}>
        Rôle : <strong style={{ color: "var(--accent2)" }}>
          {role === "primary" ? "Nœud Actif (Primary)" : role === "standby" ? "Nœud Standby" : "Poste Client"}
        </strong>
      </div>
      {log.map((l, i) => (
        <div key={i} style={{ fontSize: 12, color: "var(--text-muted)", fontFamily: "monospace" }}>✓ {l}</div>
      ))}
      <button className="btn btn-primary" style={{ marginTop: 32, minWidth: 200 }} onClick={onComplete}>
        Ouvrir le tableau de bord →
      </button>
    </div>
  );
}
