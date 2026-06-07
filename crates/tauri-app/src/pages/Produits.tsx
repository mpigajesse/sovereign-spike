import { useState, useEffect, useCallback } from "react";
import { api, type Produit } from "../api";

// CRUD Produits (LOT 2) — démontre que le moteur souverain achemine des données métier
// simples : chaque création/modification/suppression est journalisée (chiffrée),
// répliquée au passif (WAL) et sauvegardée au relais aveugle « Amane ».

type Alert = { type: "success" | "error"; msg: string } | null;
const euro = (cents: number) => (cents / 100).toLocaleString("fr-FR", { minimumFractionDigits: 2 }) + " €";

export default function Produits() {
  const role = localStorage.getItem("sovereign_role");
  const isPrimary = role === "primary";

  const [produits, setProduits] = useState<Produit[]>([]);
  const [alert, setAlert]       = useState<Alert>(null);
  const [busy, setBusy]         = useState(false);

  // Formulaire (création ou édition)
  const [editId, setEditId] = useState<string | null>(null);
  const [sku, setSku]       = useState("");
  const [nom, setNom]       = useState("");
  const [prix, setPrix]     = useState(""); // saisi en euros, converti en centimes

  const refresh = useCallback(async () => {
    try { setProduits(await api.listProduits()); }
    catch { setAlert({ type: "error", msg: "Nœud actif injoignable." }); }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const resetForm = () => { setEditId(null); setSku(""); setNom(""); setPrix(""); };

  const submit = async () => {
    if (!sku.trim() || !nom.trim()) { setAlert({ type: "error", msg: "SKU et nom requis." }); return; }
    const prixCents = Math.round(parseFloat(prix.replace(",", ".") || "0") * 100);
    if (Number.isNaN(prixCents) || prixCents < 0) { setAlert({ type: "error", msg: "Prix invalide." }); return; }
    setBusy(true); setAlert(null);
    try {
      await api.upsertProduit({ id: editId ?? undefined, sku: sku.trim(), nom: nom.trim(), prix_cents: prixCents });
      setAlert({ type: "success", msg: `✓ Produit ${editId ? "modifié" : "créé"} et journalisé (chiffré).` });
      resetForm();
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  const edit = (p: Produit) => {
    setEditId(p.id); setSku(p.sku); setNom(p.nom); setPrix((p.prix_cents / 100).toString());
  };

  const remove = async (p: Produit) => {
    if (!confirm(`Supprimer le produit « ${p.nom} » ?`)) return;
    setBusy(true); setAlert(null);
    try {
      await api.deleteProduit(p.id);
      setAlert({ type: "success", msg: "✓ Produit supprimé (suppression journalisée)." });
      if (editId === p.id) resetForm();
      await refresh();
    } catch (e) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : String(e)}` });
    } finally { setBusy(false); }
  };

  if (!isPrimary) {
    return (
      <>
        <div className="page-header">
          <div className="page-title">Produits</div>
          <div className="page-sub">Catalogue de la PME</div>
        </div>
        <div className="card" style={{ fontSize: 13, color: "var(--text-muted)" }}>
          Les écritures (création / modification) ne sont possibles que sur le <strong>nœud actif</strong>.
          Cette machine ({role ?? "client"}) est en lecture seule.
        </div>
      </>
    );
  }

  return (
    <>
      <div className="page-header">
        <div className="page-title">Produits</div>
        <div className="page-sub">Catalogue — chaque écriture est chiffrée, journalisée et répliquée</div>
      </div>

      {alert && <div className={`alert alert-${alert.type}`} style={{ marginBottom: 20 }}>{alert.msg}</div>}

      {/* Formulaire */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>{editId ? "Modifier le produit" : "Nouveau produit"}</div>
        <div className="form-row">
          <input className="input" placeholder="SKU (ex. PANTALON-L)" value={sku}
                 onChange={e => setSku(e.target.value)} disabled={!!editId} style={{ minWidth: 180 }} />
          <input className="input" placeholder="Nom du produit" value={nom}
                 onChange={e => setNom(e.target.value)} style={{ minWidth: 220 }} />
          <input className="input" placeholder="Prix (€)" value={prix}
                 onChange={e => setPrix(e.target.value)} style={{ width: 120 }} />
          <button className="btn btn-primary" disabled={busy} onClick={submit}>
            {busy ? <span className="spin">↻</span> : editId ? "Enregistrer" : "+ Créer"}
          </button>
          {editId && <button className="btn btn-ghost" disabled={busy} onClick={resetForm}>Annuler</button>}
        </div>
        {editId && <div style={{ fontSize: 11, color: "var(--text-muted)" }}>Le SKU n'est pas modifiable (clé de liaison avec le stock).</div>}
      </div>

      {/* Liste */}
      <div className="card">
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Catalogue ({produits.length})</div>
        {produits.length === 0 ? (
          <div style={{ fontSize: 13, color: "var(--text-muted)" }}>Aucun produit. Créez-en un ci-dessus.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead><tr><th>SKU</th><th>Nom</th><th>Prix</th><th></th></tr></thead>
              <tbody>
                {produits.map(p => (
                  <tr key={p.id}>
                    <td style={{ fontFamily: "monospace" }}>{p.sku}</td>
                    <td>{p.nom}</td>
                    <td style={{ fontFamily: "monospace" }}>{euro(p.prix_cents)}</td>
                    <td style={{ display: "flex", gap: 8 }}>
                      <button className="btn btn-ghost" disabled={busy} onClick={() => edit(p)}>Modifier</button>
                      <button className="btn btn-danger" disabled={busy} onClick={() => remove(p)}>Supprimer</button>
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
