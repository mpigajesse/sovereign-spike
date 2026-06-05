import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

const DEFAULTS = {
  active:  "http://192.168.200.1:3000",
  passive: "http://192.168.200.130:3001",
  relay:   "http://192.168.200.128:4000",
};

interface DiscoveredNode {
  ip:   string;
  port: number;
  role: string;
  url:  string;
}

export default function Settings() {
  const [active,  setActive]  = useState(localStorage.getItem("sovereign_active_url")  ?? DEFAULTS.active);
  const [passive, setPassive] = useState(localStorage.getItem("sovereign_passive_url") ?? DEFAULTS.passive);
  const [relay,   setRelay]   = useState(localStorage.getItem("sovereign_relay_url")   ?? DEFAULTS.relay);
  const [saved,   setSaved]   = useState(false);

  const [scanning,   setScanning]   = useState(false);
  const [discovered, setDiscovered] = useState<DiscoveredNode[] | null>(null);

  const save = () => {
    localStorage.setItem("sovereign_active_url",  active);
    localStorage.setItem("sovereign_passive_url", passive);
    localStorage.setItem("sovereign_relay_url",   relay);
    setSaved(true);
    setTimeout(() => setSaved(false), 3000);
  };

  const reset = () => {
    setActive(DEFAULTS.active);
    setPassive(DEFAULTS.passive);
    setRelay(DEFAULTS.relay);
  };

  // Découverte réseau automatique (scan VMnet1)
  const discover = async () => {
    setScanning(true);
    setDiscovered(null);
    try {
      // Déduire le sous-réseau depuis l'URL active courante (ex: 192.168.200)
      const m = active.match(/(\d+\.\d+\.\d+)\.\d+/);
      const subnet = m ? m[1] : "192.168.200";
      const nodes = await invoke<DiscoveredNode[]>("discover_nodes", { subnet });
      setDiscovered(nodes);
      // Pré-remplir automatiquement les champs depuis les nœuds trouvés
      for (const n of nodes) {
        if (n.role === "actif")  setActive(n.url);
        if (n.role === "passif") setPassive(n.url);
        if (n.role === "relais") setRelay(n.url);
      }
    } catch {
      setDiscovered([]); // mode navigateur ou erreur
    } finally {
      setScanning(false);
    }
  };

  return (
    <>
      <div className="page-header">
        <div className="page-title">Configuration</div>
        <div className="page-sub">Adresses du cluster souverain — découverte automatique ou saisie manuelle</div>
      </div>

      {/* Découverte réseau automatique */}
      <div className="card" style={{ maxWidth: 560, marginBottom: 16 }}>
        <div style={{ fontWeight: 700, marginBottom: 8 }}>🔎 Découverte réseau automatique</div>
        <div style={{ fontSize: 13, color: "var(--text-muted)", marginBottom: 16, lineHeight: 1.6 }}>
          Scanne le réseau local pour trouver les nœuds souverains installés
          (actif, passif, relais) — sans saisir d'adresse IP.
        </div>
        <button className="btn btn-primary" onClick={discover} disabled={scanning}>
          {scanning ? <><span className="spin">↻</span> Scan en cours…</> : "Découvrir les nœuds du réseau"}
        </button>

        {discovered !== null && (
          <div style={{ marginTop: 16 }}>
            {discovered.length === 0 ? (
              <div style={{ fontSize: 13, color: "var(--text-muted)" }}>
                Aucun nœud trouvé. Vérifiez que les autres machines sont démarrées,
                ou saisissez les adresses manuellement ci-dessous.
              </div>
            ) : (
              <div className="table-wrap">
                <table>
                  <thead><tr><th>Rôle</th><th>Adresse</th><th>Port</th></tr></thead>
                  <tbody>
                    {discovered.map(n => (
                      <tr key={n.url}>
                        <td><span className="badge badge-green" style={{ fontSize: 11 }}>{n.role}</span></td>
                        <td style={{ fontFamily: "monospace", fontSize: 12 }}>{n.ip}</td>
                        <td style={{ fontFamily: "monospace", fontSize: 12 }}>{n.port}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div style={{ fontSize: 12, color: "var(--green)", marginTop: 10, padding: "0 4px" }}>
                  ✓ Champs pré-remplis automatiquement — cliquez "Sauvegarder" ci-dessous.
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="card" style={{ maxWidth: 560 }}>
        <div style={{ fontWeight: 700, marginBottom: 20 }}>Saisie manuelle des URLs</div>

        {saved && <div className="alert alert-success">✓ Configuration sauvegardée — rechargez le tableau de bord</div>}

        <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
          <div>
            <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
              Nœud ACTIF (Windows 11 — primary)
            </label>
            <input
              value={active} onChange={e => setActive(e.target.value)}
              style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "9px 12px", borderRadius: 7, fontSize: 13 }}
              placeholder="http://192.168.200.1:3000"
            />
            <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4 }}>
              IP de la machine Windows 11 sur VMnet1
            </div>
          </div>

          <div>
            <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
              Nœud PASSIF (Ubuntu — standby)
            </label>
            <input
              value={passive} onChange={e => setPassive(e.target.value)}
              style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "9px 12px", borderRadius: 7, fontSize: 13 }}
              placeholder="http://192.168.200.130:3001"
            />
          </div>

          <div>
            <label style={{ display: "block", fontSize: 12, color: "var(--text-muted)", marginBottom: 6 }}>
              Relais aveugle (Kali — zero-knowledge)
            </label>
            <input
              value={relay} onChange={e => setRelay(e.target.value)}
              style={{ width: "100%", background: "var(--surface2)", border: "1px solid var(--border)", color: "var(--text)", padding: "9px 12px", borderRadius: 7, fontSize: 13 }}
              placeholder="http://192.168.200.128:4000"
            />
          </div>
        </div>

        <div style={{ display: "flex", gap: 12, marginTop: 24 }}>
          <button className="btn btn-primary" onClick={save}>Sauvegarder</button>
          <button className="btn btn-ghost" onClick={reset}>Réinitialiser (défauts)</button>
        </div>
      </div>

      {/* Guide réseau */}
      <div className="card" style={{ maxWidth: 560, marginTop: 16 }}>
        <div style={{ fontWeight: 700, marginBottom: 14 }}>Guide réseau — VMnet1 (192.168.200.0/24)</div>
        <div className="table-wrap">
          <table>
            <thead><tr><th>Machine</th><th>IP VMnet1</th><th>Port</th><th>Rôle</th></tr></thead>
            <tbody>
              {[
                ["Windows 11 (physique)", "192.168.200.1",   "3000", "Nœud actif (primary)"],
                ["Ubuntu 26.04 VM",       "192.168.200.130", "3001", "Nœud passif (standby)"],
                ["Kali Linux VM",         "192.168.200.128", "4000", "Relais aveugle"],
                ["Windows 10 VM",         "192.168.200.x",   "—",    "Client (cette machine)"],
              ].map(([m, ip, port, role]) => (
                <tr key={ip}>
                  <td style={{ fontSize: 12 }}>{m}</td>
                  <td style={{ fontFamily: "monospace", fontSize: 12 }}>{ip}</td>
                  <td style={{ fontFamily: "monospace", fontSize: 12 }}>{port}</td>
                  <td style={{ fontSize: 12, color: "var(--text-muted)" }}>{role}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 10 }}>
          Vérifiez avec : <code style={{ background: "var(--surface2)", padding: "2px 6px", borderRadius: 4 }}>Test-NetConnection 192.168.200.1 -Port 3000</code>
        </div>
      </div>
    </>
  );
}
