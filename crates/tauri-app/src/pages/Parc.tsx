import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, type EnrolledDevice } from "../api";

// Gestion du parc de machines — rend démontrables EN LIVE les critères §7.4 :
//   #8  enrôlement sans exposer la clé   (génération paire X25519 → sealed box → unwrap)
//   #9  dé-enrôlement → ne peut plus déchiffrer  (rotation DEK + preuve sur blob réel)
//   #10 retrait → alerte quorum  (failover auto sûr si ≥ 3 machines)
//   #11 perte de toutes les machines → code de récupération (Argon2id)
//
// Disponible uniquement sur le nœud actif (primary) : c'est lui qui détient la DEK
// et orchestre l'enrôlement/rotation.

type Alert = { type: "success" | "error" | "info"; msg: string } | null;

// Mémoire locale (volatile) des DEK récupérées par appareil, pour la démo de
// déchiffrement. En production, jamais affichée — ici c'est la PREUVE pédagogique.
interface DeviceProof {
  publicKey: string;
  label:     string;
  dekHex:    string;  // DEK récupérée par CET appareil via sa clé privée (#8)
  gen:       number;  // génération de DEK au moment de l'enrôlement
}

export default function Parc() {
  const role = localStorage.getItem("sovereign_role");
  const isPrimary = role === "primary";

  const [devices, setDevices]   = useState<EnrolledDevice[]>([]);
  const [count, setCount]       = useState(0);
  const [generation, setGen]    = useState(1);
  const [quorumSafe, setQuorum] = useState(false);
  const [alert, setAlert]       = useState<Alert>(null);
  const [busy, setBusy]         = useState(false);

  const [label, setLabel]       = useState("");
  const [proofs, setProofs]     = useState<DeviceProof[]>([]);

  const [passphrase, setPassphrase] = useState("");
  const [restoreResult, setRestoreResult] = useState<{ ok: boolean; matches: boolean } | null>(null);

  // Preuve critère #9 : déchiffrement du dernier blob du journal par chaque DEK connue.
  const [decryptProof, setDecryptProof] = useState<
    { gen: number; label: string; seq: number; ok: boolean }[] | null
  >(null);

  const refresh = useCallback(async () => {
    if (!isPrimary) return;
    try {
      const list = await api.listDevices();
      setDevices(list.devices);
      setCount(list.count);
      setGen(list.generation);
      setQuorum(list.auto_failover_safe);
    } catch {
      setAlert({ type: "error", msg: "Nœud actif injoignable — la gestion du parc n'est disponible que sur le primary." });
    }
  }, [isPrimary]);

  useEffect(() => { refresh(); }, [refresh]);

  // ── #8 Enrôler un nouvel appareil ────────────────────────────────────────────
  const enroll = async () => {
    setBusy(true); setAlert(null);
    try {
      // 1. L'appareil génère sa paire X25519 (clé privée gardée dans le backend Rust)
      const publicKey = await invoke<string>("dev_generate_keypair");
      // 2. Le nœud actif emballe la DEK courante pour cette clé publique (sealed box)
      const res = await api.enrollDevice(publicKey, label || `Appareil ${count + 1}`);
      // 3. PREUVE #8 : l'appareil ouvre la sealed box avec SA clé privée → récupère la DEK
      const dekHex = await invoke<string>("dev_unwrap_dek", { publicHex: publicKey, sealedHex: res.sealed_dek });

      setProofs(p => [...p, { publicKey, label: label || `Appareil ${count + 1}`, dekHex, gen: res.generation }]);
      setAlert({ type: "success", msg: `✓ Appareil enrôlé (génération ${res.generation}). La clé n'a jamais transité en clair : récupérée localement via sealed box.` });
      setLabel("");
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  // ── #9 + #10 Dé-enrôler (révocation + rotation de DEK) ────────────────────────
  const revoke = async (device_id: string, lbl: string) => {
    if (!confirm(`Dé-enrôler « ${lbl} » ?\n\nLa DEK va être tournée et re-scellée pour les machines restantes. Cet appareil ne pourra plus déchiffrer les écritures suivantes.`)) return;
    setBusy(true); setAlert(null); setDecryptProof(null);
    try {
      const res = await api.revokeDevice(device_id);
      let msg = `✓ Dé-enrôlé. DEK tournée → génération ${res.new_generation}. ${res.remaining} machine(s) restante(s).`;
      if (res.quorum_warning) msg += ` ⚠ ${res.quorum_warning}`;
      setAlert({ type: res.quorum_warning ? "info" : "success", msg });
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  // ── #9 PREUVE : qui peut déchiffrer le dernier blob du journal ? ──────────────
  const proveDecryption = async () => {
    setBusy(true); setAlert(null);
    try {
      // Récupère la dernière entrée du journal (blob chiffré réel, écrit par le nœud actif)
      const journal = await api.getJournal(0, 1000);
      if (journal.length === 0) {
        setAlert({ type: "info", msg: "Journal vide — faites d'abord une vente (page Stock) pour produire un blob à déchiffrer." });
        return;
      }
      const last = journal[journal.length - 1];
      // Teste chaque DEK connue (une par génération d'enrôlement) contre ce blob
      const results = await Promise.all(
        proofs.map(async pr => ({
          gen:   pr.gen,
          label: pr.label,
          seq:   last.seq,
          ok:    await invoke<boolean>("try_decrypt_blob", {
            dekHex: pr.dekHex, nonceHex: last.blob_nonce, ctHex: last.blob_ciphertext,
          }),
        }))
      );
      setDecryptProof(results);
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  // ── #11 Code de récupération ──────────────────────────────────────────────────
  const setupRecovery = async () => {
    if (passphrase.trim().length < 8) { setAlert({ type: "error", msg: "Passphrase : 8 caractères minimum." }); return; }
    setBusy(true); setAlert(null);
    try {
      await api.recoverySetup(passphrase);
      setAlert({ type: "success", msg: "✓ Code de récupération configuré. La DEK est emballée sous cette passphrase (Argon2id) — à imprimer et mettre au coffre." });
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  const restoreRecovery = async () => {
    setBusy(true); setAlert(null); setRestoreResult(null);
    try {
      const res = await api.recoveryRestore(passphrase);
      setRestoreResult({ ok: true, matches: res.matches_current });
      setAlert({ type: "success", msg: `✓ DEK restaurée depuis la passphrase. ${res.matches_current ? "Elle correspond à la DEK courante." : "Elle correspond à une génération antérieure (rotation depuis le dernier setup)."}` });
    } catch (e) {
      setRestoreResult({ ok: false, matches: false });
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  if (!isPrimary) {
    return (
      <>
        <div className="page-header">
          <div className="page-title">Gestion du parc</div>
          <div className="page-sub">Enrôlement, rotation de clé et récupération</div>
        </div>
        <div className="card" style={{ fontSize: 13, color: "var(--text-muted)" }}>
          Cette page n'est disponible que sur le <strong>nœud actif (primary)</strong> — c'est lui qui
          détient la DEK et orchestre l'enrôlement des appareils. Rôle actuel : <code>{role ?? "client"}</code>.
        </div>
      </>
    );
  }

  return (
    <>
      <div className="page-header">
        <div className="page-title">Gestion du parc</div>
        <div className="page-sub">Enrôlement (#8), rotation de clé au retrait (#9), quorum (#10), récupération (#11)</div>
      </div>

      {alert && <div className={`alert alert-${alert.type}`} style={{ marginBottom: 20 }}>{alert.msg}</div>}

      {/* État du parc + quorum (#10) */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>État du parc</div>
        <div style={{ display: "flex", gap: 28, flexWrap: "wrap" }}>
          <div><div className="card-label">Appareils enrôlés</div><div className="card-value">{count}</div></div>
          <div><div className="card-label">Génération DEK</div><div className="card-value">{generation}</div></div>
          <div>
            <div className="card-label">Failover automatique</div>
            <span className={`badge badge-${quorumSafe ? "green" : "yellow"}`} style={{ marginTop: 8 }}>
              {quorumSafe ? "✓ Quorum sûr (≥ 3)" : `⚠ Quorum insuffisant (${count}/3)`}
            </span>
          </div>
        </div>
        {!quorumSafe && (
          <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 12, lineHeight: 1.6 }}>
            Le failover automatique par quorum exige au moins 3 machines (§4.5). En dessous, restez en
            <strong> bascule manuelle</strong> (sûre à 2 machines car un humain décide — pas de split-brain).
          </div>
        )}
      </div>

      {/* #8 Enrôlement */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 4 }}>Enrôler un appareil</div>
        <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 16 }}>
          L'appareil génère sa paire X25519 ; le nœud actif emballe la DEK pour sa clé publique (sealed box).
          La clé n'est <strong>jamais</strong> exposée en clair — ni au réseau, ni au relais.
        </div>
        <div className="form-row">
          <input className="input" placeholder="Nom de l'appareil (ex. Poste de Karim)"
                 value={label} onChange={e => setLabel(e.target.value)} style={{ minWidth: 260 }} />
          <button className="btn btn-primary" disabled={busy} onClick={enroll}>
            {busy ? <span className="spin">↻</span> : "+ Enrôler (générer clé + sceller DEK)"}
          </button>
        </div>

        {proofs.length > 0 && (
          <div style={{ marginTop: 16 }}>
            <div className="card-label" style={{ marginBottom: 8 }}>Preuve #8 — DEK récupérée localement par chaque appareil</div>
            <div className="table-wrap">
              <table>
                <thead><tr><th>Appareil</th><th>Clé publique (QR)</th><th>DEK récupérée (gén.)</th></tr></thead>
                <tbody>
                  {proofs.map(p => (
                    <tr key={p.publicKey}>
                      <td>{p.label}</td>
                      <td style={{ fontFamily: "monospace", fontSize: 11 }}>{p.publicKey.slice(0, 16)}…</td>
                      <td style={{ fontFamily: "monospace", fontSize: 11, color: "var(--green)" }}>
                        ✓ {p.dekHex.slice(0, 12)}… (gén. {p.gen})
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>

      {/* Liste + révocation (#9, #10) */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Appareils enrôlés</div>
        {devices.length === 0 ? (
          <div style={{ fontSize: 13, color: "var(--text-muted)" }}>Aucun appareil enrôlé pour l'instant.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead><tr><th>Nom</th><th>ID</th><th>DEK scellée (opaque)</th><th>Enrôlé le</th><th></th></tr></thead>
              <tbody>
                {devices.map(d => (
                  <tr key={d.device_id}>
                    <td>{d.label}</td>
                    <td style={{ fontFamily: "monospace", fontSize: 11 }}>{d.device_id.slice(0, 8)}</td>
                    <td style={{ fontFamily: "monospace", fontSize: 11, color: "var(--text-muted)" }}>{d.sealed_dek.slice(0, 16)}…</td>
                    <td style={{ fontSize: 12, color: "var(--text-muted)" }}>{d.enrolled_at.slice(0, 19).replace("T", " ")}</td>
                    <td>
                      <button className="btn btn-danger" disabled={busy} onClick={() => revoke(d.device_id, d.label)}>
                        Dé-enrôler + rotation
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* #9 Preuve de déchiffrement */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 4 }}>Preuve #9 — qui peut déchiffrer le dernier blob ?</div>
        <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 16, lineHeight: 1.6 }}>
          Teste la dernière écriture réelle du journal contre chaque DEK enrôlée. Après une rotation, la DEK
          d'un appareil dé-enrôlé (génération antérieure) <strong>échoue</strong> ; la DEK courante réussit.
          <br />Scénario : enrôler 2 appareils → faire une vente (page Stock) → dé-enrôler l'un → refaire une vente → cliquer ci-dessous.
        </div>
        <button className="btn btn-ghost" disabled={busy} onClick={proveDecryption}>
          {busy ? <span className="spin">↻</span> : "Tester le déchiffrement du dernier blob"}
        </button>
        {decryptProof && (
          <div className="table-wrap" style={{ marginTop: 16 }}>
            <table>
              <thead><tr><th>Appareil (génération DEK)</th><th>Blob seq</th><th>Déchiffrement</th></tr></thead>
              <tbody>
                {decryptProof.map((r, i) => (
                  <tr key={i}>
                    <td>{r.label} (gén. {r.gen})</td>
                    <td style={{ fontFamily: "monospace" }}>#{r.seq}</td>
                    <td style={{ color: r.ok ? "var(--green)" : "var(--red)", fontWeight: 700 }}>
                      {r.ok ? "✓ réussi (DEK valide)" : "✗ refusé (DEK périmée)"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* #11 Code de récupération */}
      <div className="card">
        <div style={{ fontWeight: 700, marginBottom: 4 }}>Code de récupération (#11)</div>
        <div style={{ fontSize: 12, color: "var(--text-muted)", marginBottom: 16, lineHeight: 1.6 }}>
          Emballe la DEK courante sous une passphrase à haute entropie (Argon2id). Si toutes les machines
          sont perdues, cette passphrase seule permet de restaurer l'accès aux données.
        </div>
        <div className="form-row">
          <input className="input" type="password" placeholder="Passphrase de récupération (≥ 8 car.)"
                 value={passphrase} onChange={e => setPassphrase(e.target.value)} style={{ minWidth: 280 }} />
          <button className="btn btn-primary" disabled={busy} onClick={setupRecovery}>Configurer</button>
          <button className="btn btn-ghost"   disabled={busy} onClick={restoreRecovery}>Restaurer (preuve)</button>
        </div>
        {restoreResult?.ok && (
          <div style={{ marginTop: 12, fontSize: 13, color: "var(--green)" }}>
            ✓ Restauration réussie {restoreResult.matches ? "— DEK identique à la courante." : "— génération antérieure (une rotation a eu lieu depuis le setup)."}
          </div>
        )}
      </div>
    </>
  );
}
