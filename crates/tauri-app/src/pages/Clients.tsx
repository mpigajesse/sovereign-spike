import { useState, useEffect, useCallback } from "react";
import { api, type Client } from "../api";

// CRUD Clients (LOT 2) — CRUD pur, journalisé/chiffré/répliqué comme le reste.

type Alert = { type: "success" | "error"; msg: string } | null;

export default function Clients() {
  const role = localStorage.getItem("sovereign_role");
  const isPrimary = role === "primary";

  const [clients, setClients] = useState<Client[]>([]);
  const [alert, setAlert]     = useState<Alert>(null);
  const [busy, setBusy]       = useState(false);

  const [editId, setEditId] = useState<string | null>(null);
  const [nom, setNom]       = useState("");
  const [email, setEmail]   = useState("");
  const [tel, setTel]       = useState("");

  const refresh = useCallback(async () => {
    try { setClients(await api.listClients()); }
    catch { setAlert({ type: "error", msg: "Nœud actif injoignable." }); }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const resetForm = () => { setEditId(null); setNom(""); setEmail(""); setTel(""); };

  const submit = async () => {
    if (!nom.trim()) { setAlert({ type: "error", msg: "Le nom est requis." }); return; }
    setBusy(true); setAlert(null);
    try {
      await api.upsertClient({ id: editId ?? undefined, nom: nom.trim(), email: email.trim(), telephone: tel.trim() });
      setAlert({ type: "success", msg: `✓ Client ${editId ? "modifié" : "créé"} et journalisé (chiffré).` });
      resetForm();
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  const edit = (c: Client) => {
    setEditId(c.id); setNom(c.nom); setEmail(c.email); setTel(c.telephone);
  };

  const remove = async (c: Client) => {
    if (!confirm(`Supprimer le client « ${c.nom} » ?`)) return;
    setBusy(true); setAlert(null);
    try {
      await api.deleteClient(c.id);
      setAlert({ type: "success", msg: "✓ Client supprimé (suppression journalisée)." });
      if (editId === c.id) resetForm();
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  if (!isPrimary) {
    return (
      <>
        <div className="page-header">
          <div className="page-title">Clients</div>
          <div className="page-sub">Répertoire de la PME</div>
        </div>
        <div className="card" style={{ fontSize: 13, color: "var(--text-muted)" }}>
          Les écritures ne sont possibles que sur le <strong>nœud actif</strong>. Cette machine
          ({role ?? "client"}) est en lecture seule.
        </div>
      </>
    );
  }

  return (
    <>
      <div className="page-header">
        <div className="page-title">Clients</div>
        <div className="page-sub">Répertoire — chaque écriture est chiffrée, journalisée et répliquée</div>
      </div>

      {alert && <div className={`alert alert-${alert.type}`} style={{ marginBottom: 20 }}>{alert.msg}</div>}

      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>{editId ? "Modifier le client" : "Nouveau client"}</div>
        <div className="form-row">
          <input className="input" placeholder="Nom" value={nom}
                 onChange={e => setNom(e.target.value)} style={{ minWidth: 200 }} />
          <input className="input" placeholder="Email" value={email}
                 onChange={e => setEmail(e.target.value)} style={{ minWidth: 220 }} />
          <input className="input" placeholder="Téléphone" value={tel}
                 onChange={e => setTel(e.target.value)} style={{ width: 160 }} />
          <button className="btn btn-primary" disabled={busy} onClick={submit}>
            {busy ? <span className="spin">↻</span> : editId ? "Enregistrer" : "+ Créer"}
          </button>
          {editId && <button className="btn btn-ghost" disabled={busy} onClick={resetForm}>Annuler</button>}
        </div>
      </div>

      <div className="card">
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Répertoire ({clients.length})</div>
        {clients.length === 0 ? (
          <div style={{ fontSize: 13, color: "var(--text-muted)" }}>Aucun client. Créez-en un ci-dessus.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead><tr><th>Nom</th><th>Email</th><th>Téléphone</th><th></th></tr></thead>
              <tbody>
                {clients.map(c => (
                  <tr key={c.id}>
                    <td>{c.nom}</td>
                    <td style={{ color: "var(--text-muted)" }}>{c.email || "—"}</td>
                    <td style={{ fontFamily: "monospace" }}>{c.telephone || "—"}</td>
                    <td style={{ display: "flex", gap: 8 }}>
                      <button className="btn btn-ghost" disabled={busy} onClick={() => edit(c)}>Modifier</button>
                      <button className="btn btn-danger" disabled={busy} onClick={() => remove(c)}>Supprimer</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}
