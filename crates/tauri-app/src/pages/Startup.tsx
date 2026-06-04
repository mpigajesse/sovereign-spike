// Écran de démarrage affiché pendant le lancement automatique du nœud actif.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface StartupStatus {
  mode:       string;
  message:    string;
  active_url: string;
}

interface Props {
  onReady: (activeUrl: string) => void;
}

export default function Startup({ onReady }: Props) {
  const [status, setStatus]  = useState<StartupStatus | null>(null);
  const [step,   setStep]    = useState(0);

  const steps = [
    "Vérification de PostgreSQL…",
    "Démarrage du nœud actif…",
    "Connexion au cluster…",
    "Prêt",
  ];

  useEffect(() => {
    // Animer les étapes
    const t = setInterval(() => setStep(s => Math.min(s + 1, steps.length - 1)), 800);

    // Lire le statut de démarrage (le backend a peut-être déjà démarré)
    const poll = setInterval(async () => {
      try {
        const s = await invoke<StartupStatus>("get_startup_status");
        setStatus(s);

        if (s.mode === "local" || s.mode === "remote") {
          clearInterval(poll);
          clearInterval(t);
          // Attendre 1s pour que l'UI soit visible puis passer au dashboard
          setTimeout(() => onReady(s.active_url), 1000);
        }
      } catch {
        // Mode navigateur (dev sans Tauri) — passer directement
        clearInterval(poll);
        clearInterval(t);
        onReady("http://192.168.200.1:3000");
      }
    }, 500);

    return () => { clearInterval(t); clearInterval(poll); };
  }, []);

  return (
    <div style={{
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      minHeight: "100vh", background: "var(--bg)", gap: 32
    }}>
      {/* Logo */}
      <div style={{ fontSize: 64 }}>⬡</div>
      <div style={{ textAlign: "center" }}>
        <div style={{ fontSize: 28, fontWeight: 800, color: "var(--accent2)", marginBottom: 8 }}>
          Sovereign Data Agent
        </div>
        <div style={{ color: "var(--text-muted)", fontSize: 13 }}>
          Framework Coffre-Fort Data Souverain — EIGSI × AL BARAA CONSULTING
        </div>
      </div>

      {/* Étapes de démarrage */}
      <div style={{ display: "flex", flexDirection: "column", gap: 10, minWidth: 320 }}>
        {steps.map((s, i) => (
          <div key={s} style={{ display: "flex", alignItems: "center", gap: 12, opacity: i <= step ? 1 : 0.3 }}>
            <span style={{ width: 20, textAlign: "center" }}>
              {i < step ? <span style={{ color: "var(--green)" }}>✓</span>
               : i === step ? <span className="spin" style={{ color: "var(--accent2)" }}>↻</span>
               : <span style={{ color: "var(--border)" }}>○</span>}
            </span>
            <span style={{ fontSize: 13, color: i === step ? "var(--text)" : "var(--text-muted)" }}>{s}</span>
          </div>
        ))}
      </div>

      {/* Message de statut */}
      {status && (
        <div style={{
          background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 10,
          padding: "12px 20px", fontSize: 13, maxWidth: 400, textAlign: "center"
        }}>
          <span className={`dot dot-${status.mode === "local" ? "green" : status.mode === "remote" ? "yellow" : "yellow"}`} />
          {status.message}
        </div>
      )}
    </div>
  );
}
