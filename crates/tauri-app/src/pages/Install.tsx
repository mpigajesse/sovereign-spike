// Assistant d'installation — affiché au premier lancement.
// L'utilisateur choisit le rôle de cette machine dans le cluster souverain.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../api";

type Role = "solo" | "primary" | "standby" | "relais";

interface Props {
  onComplete: () => void;
}

export default function Install({ onComplete }: Props) {
  const [step,       setStep]       = useState<"role" | "config" | "installing" | "done">("role");
  const [role,       setRole]       = useState<Role | null>(null);
  // Vides par défaut : l'utilisateur saisit lui-même l'adresse du primary.
  const [primaryUrl, setPrimaryUrl] = useState("");
  const [primaryPg,  setPrimaryPg]  = useState("");
  // IP de CETTE machine sur le réseau du cluster (saisie libre, aucune IP en dur).
  const [selfIp,     setSelfIp]     = useState("");
  // Mode de réplication choisi par la PME (primary) — modifiable ensuite.
  const [syncMode,   setSyncMode]   = useState(true);
  // Création de compte (tenant) — saisie sur le nœud actif (primary).
  const [entreprise, setEntreprise] = useState("");
  const [gerant,     setGerant]     = useState("");
  const [emailPme,   setEmailPme]   = useState("");
  const [log,        setLog]        = useState<string[]>([]);
  const [error,      setError]      = useState<string | null>(null);

  const addLog = (msg: string) => setLog(l => [...l, msg]);

  // Extrait l'hôte nu : "http://192.168.200.1:5432/x" -> "192.168.200.1"
  // (le champ "IP PostgreSQL" attend une IP, pas une URL).
  const bareHost = (s: string) =>
    s.trim().replace(/^[a-z]+:\/\//i, "").replace(/[:/].*$/, "");

  const startInstall = async () => {
    // Le nœud actif crée le compte de la PME : le nom de l'entreprise est requis.
    if (role === "primary" && !entreprise.trim()) {
      setError("Saisissez le nom de votre entreprise pour créer votre compte.");
      return;
    }
    // Validation des adresses saisies manuellement (standby uniquement)
    if (role === "standby") {
      if (!primaryUrl.trim()) {
        setError("Saisissez l'URL du nœud actif (ex. http://192.168.200.1:3000).");
        return;
      }
      if (!primaryPg.trim()) {
        setError("Saisissez l'IP PostgreSQL du primary (ex. 192.168.200.1).");
        return;
      }
    }

    setStep("installing");
    setError(null);
    setLog([]);

    // Une (ré)installation repart d'une config propre : on efface les URLs
    // passif/relais (qu'aucun rôle ne configure ici) pour éviter des valeurs
    // périmées qui afficheraient "Hors ligne" au lieu de "Non configuré".
    localStorage.removeItem("sovereign_passive_url");
    localStorage.removeItem("sovereign_relay_url");

    try {
      if (role === "solo") {
        // Mode PME solo : SQLite seul, aucun PostgreSQL requis
        addLog("Mode PME Solo — aucun PostgreSQL requis");
        addLog("Génération de la clé de chiffrement (DEK)...");
        // DEK fixe pour le spike (en prod : générée + sauvegardée au coffre)
        const dek = localStorage.getItem("sovereign_dek")
          ?? "174835f0e063680d4b4652c7edf9472a1db0626388dbbe4342d84a7c9bce035b";
        localStorage.setItem("sovereign_dek", dek);
        addLog("Démarrage du nœud solo (SQLite)...");
        const status = await invoke<{ message: string; active_url: string }>("start_solo_node", { dekHex: dek });
        addLog(status.message);
        localStorage.setItem("sovereign_role", "solo");
        localStorage.setItem("sovereign_active_url", "http://127.0.0.1:3000");

      } else if (role === "primary") {
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
        // L'IP est saisie par l'utilisateur (aucune IP en dur). Vide → loopback.
        const ip = bareHost(selfIp) || "127.0.0.1";
        localStorage.setItem("sovereign_active_url", `http://${ip}:3000`);
        localStorage.setItem("sovereign_self_ip", ip);
        addLog(`Nœud actif démarré ✓ (${ip}:3000)`);

        // Création du compte de la PME (tenant) — auto-souverain, généré localement.
        addLog("Création de votre compte (génération du tenant_id local)...");
        try {
          const tenant = await api.tenantBootstrap(entreprise.trim(), gerant.trim(), emailPme.trim());
          localStorage.setItem("sovereign_tenant_id", tenant.tenant_id);
          localStorage.setItem("sovereign_tenant_nom", tenant.nom);
          addLog(`Compte « ${tenant.nom} » créé ✓ (tenant ${tenant.tenant_id.slice(0, 8)}…)`);
        } catch (e) {
          addLog(`⚠ Compte non créé : ${e instanceof Error ? e.message : String(e)}`);
        }
        // Mode de réplication choisi par la PME (modifiable ensuite dans Sécurité)
        try {
          const msg = await invoke<string>("set_replication_mode", { sync: syncMode });
          addLog(msg);
        } catch { /* standby pas encore rattaché : sans effet immédiat */ }

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
        const result = await invoke<string>("run_standby_setup", { primaryIp: bareHost(primaryPg) });
        addLog(result);
        localStorage.setItem("sovereign_role", "standby");
        localStorage.setItem("sovereign_active_url", primaryUrl.trim());

      } else {
        // Relais aveugle — lance le binaire relais (zero-knowledge, port 4000).
        // Aucune clé, aucune donnée en clair : il ne stocke que des blobs opaques.
        addLog("Démarrage du relais aveugle (zero-knowledge)...");
        const status = await invoke<{ message: string }>("start_relay_node");
        addLog(status.message);
        localStorage.setItem("sovereign_role", "relais");
        const ip = bareHost(selfIp) || "127.0.0.1";
        localStorage.setItem("sovereign_relay_url", `http://${ip}:4000`);
        localStorage.setItem("sovereign_self_ip", ip);
        addLog(`Relais en ligne ✓ (${ip}:4000)`);
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

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr 1fr", gap: 16, maxWidth: 1100, width: "100%" }}>
          {[
            {
              id: "solo" as Role,
              icon: "⬢",
              title: "PME Solo",
              subtitle: "Poste autonome",
              desc: "Tout-en-un sur cette machine, sans serveur. Idéal pour une TPE avec un seul ordinateur.",
              require: "Aucun prérequis — SQLite embarqué",
              color: "var(--accent)",
            },
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
              id: "relais" as Role,
              icon: "◇",
              title: "Relais aveugle",
              subtitle: "Zero-knowledge (éditeur)",
              desc: "Stocke les blobs chiffrés (sauvegarde hors-site). Ne détient aucune clé, ne voit jamais le clair — aveugle par construction.",
              require: "Aucune base — port 4000",
              color: "var(--accent2)",
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
            {role === "solo"    ? "⬢ Configuration PME Solo" :
             role === "primary" ? "⬡ Configuration du nœud actif" :
             role === "standby" ? "◎ Configuration du nœud standby" :
             "◇ Configuration du relais aveugle"}
          </div>
          <div style={{ color: "var(--text-muted)", marginBottom: 32, fontSize: 13 }}>
            {role === "solo"    ? "Tout fonctionne sur cette machine, sans serveur ni configuration." :
             role === "primary" ? "Cette machine sera le serveur principal de la PME." :
             role === "standby" ? "Cette machine répliquera le nœud actif et pourra prendre le relais." :
             "Cette machine hébergera le relais éditeur : il ne reçoit que du chiffré, jamais de clé."}
          </div>

          {error && <div className="alert alert-error" style={{ marginBottom: 20 }}>{error}</div>}

          <div className="card">
            {role === "primary" && (
              <div style={{ marginBottom: 24, paddingBottom: 20, borderBottom: "1px solid var(--border)" }}>
                <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 4 }}>Votre compte entreprise</div>
                <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 16 }}>
                  Généré localement sur votre machine. Aucune donnée n'est envoyée à l'éditeur — votre identité reste souveraine.
                </div>
                <div style={{ marginBottom: 14 }}>
                  <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                    Nom de l'entreprise <span style={{ color: "var(--red)" }}>*</span>
                  </label>
                  <input
                    value={entreprise}
                    onChange={e => setEntreprise(e.target.value)}
                    style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                    placeholder="ex. Boutique Salma"
                  />
                </div>
                <div style={{ display: "flex", gap: 12 }}>
                  <div style={{ flex: 1 }}>
                    <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                      Gérant <span style={{ opacity: 0.6 }}>· optionnel</span>
                    </label>
                    <input
                      value={gerant}
                      onChange={e => setGerant(e.target.value)}
                      style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                      placeholder="ex. Salma B."
                    />
                  </div>
                  <div style={{ flex: 1 }}>
                    <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                      Email <span style={{ opacity: 0.6 }}>· optionnel</span>
                    </label>
                    <input
                      value={emailPme}
                      onChange={e => setEmailPme(e.target.value)}
                      style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                      placeholder="contact@entreprise.ma"
                    />
                  </div>
                </div>
              </div>
            )}
            {role === "standby" && (
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

            {(role === "primary" || role === "relais") && (
              <div style={{ marginBottom: 20 }}>
                <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
                  IP de cette machine sur le réseau du cluster <span style={{ opacity: 0.6 }}>· optionnel</span>
                </label>
                <input
                  value={selfIp}
                  onChange={e => setSelfIp(e.target.value)}
                  style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "10px 14px", borderRadius: 8, fontSize: 14 }}
                  placeholder="ex. 192.168.200.1"
                />
                <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4 }}>
                  Adresse que les autres machines utiliseront pour joindre ce nœud. Laissez vide pour 127.0.0.1 (poste isolé).
                </div>
              </div>
            )}

            {role === "primary" && (
              <div style={{ marginBottom: 20 }}>
                <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 8 }}>
                  Mode de réplication
                </label>
                <div style={{ display: "flex", gap: 10 }}>
                  <button type="button" className={syncMode ? "btn btn-primary" : "btn btn-ghost"} style={{ flex: 1 }} onClick={() => setSyncMode(true)}>
                    Synchrone (zéro perte)
                  </button>
                  <button type="button" className={!syncMode ? "btn btn-primary" : "btn btn-ghost"} style={{ flex: 1 }} onClick={() => setSyncMode(false)}>
                    Asynchrone (rapide)
                  </button>
                </div>
                <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 6 }}>
                  Synchrone : chaque écriture attend l'accusé d'un standby (aucune perte au failover). Modifiable ensuite dans Sécurité.
                </div>
              </div>
            )}

            {role === "primary" && (
              <div style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.7 }}>
                ✓ PostgreSQL sera configuré automatiquement sur <code>127.0.0.1:5432</code><br />
                ✓ La DEK sera générée et sauvegardée dans <code>shared.env</code><br />
                ✓ Le nœud actif démarrera automatiquement au lancement
              </div>
            )}

            {role === "solo" && (
              <div style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.7 }}>
                ✓ Aucun PostgreSQL requis — base <code>SQLite</code> embarquée<br />
                ✓ Chiffrement XChaCha20-Poly1305 + journal chiffré identiques<br />
                ✓ Anti-survente et intégrité garantis (transaction SQLite unique)<br />
                ✓ Données conservées localement entre les redémarrages
              </div>
            )}

            {role === "relais" && (
              <div style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.7 }}>
                ✓ Le relais démarre sur <code>0.0.0.0:4000</code> (joignable sur le LAN)<br />
                ✓ Stocke des <strong>blobs chiffrés opaques</strong> — aucune clé, aucun clair<br />
                ✓ Zero-knowledge par construction (ne dépend pas du cœur crypto)<br />
                ✓ Le nœud actif (primary) y poussera ses sauvegardes automatiquement
              </div>
            )}
          </div>

          <div style={{ display: "flex", gap: 12, marginTop: 24 }}>
            <button className="btn btn-ghost" onClick={() => setStep("role")}>← Retour</button>
            <button className="btn btn-primary" style={{ flex: 1 }} onClick={startInstall}>
              {role === "solo"    ? "Démarrer en mode solo" :
               role === "standby" ? "Configurer le standby" :
               role === "primary" ? "Démarrer le nœud actif" :
               "Démarrer le relais aveugle"}
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
          {role === "solo" ? "PME Solo (autonome)" :
           role === "primary" ? "Nœud Actif (Primary)" :
           role === "standby" ? "Nœud Standby" : "Relais aveugle"}
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
