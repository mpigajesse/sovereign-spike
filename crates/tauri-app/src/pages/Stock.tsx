import { useState } from "react";
import { api } from "../api";

interface StockResult {
  item_id: string;
  active: number | null;
  passive: number | null;
}

export default function Stock() {
  const [opType,   setOpType]   = useState<"sale" | "stock_adjust">("sale");
  const [itemId,   setItemId]   = useState("PANTALON-L");
  const [quantity, setQuantity] = useState(1);
  const [alert,    setAlert]    = useState<{ type: "success" | "error"; msg: string } | null>(null);
  const [loading,  setLoading]  = useState(false);

  const [lookupId,     setLookupId]     = useState("PANTALON-L");
  const [stockResult,  setStockResult]  = useState<StockResult | null>(null);
  const [lookupLoading, setLookupLoading] = useState(false);

  const submit = async () => {
    setLoading(true);
    setAlert(null);
    try {
      const r = await api.write(opType, itemId, quantity);
      setAlert({
        type: "success",
        msg: `✓ ${opType === "sale" ? "Vente" : "Ajustement"} validé — seq=${r.seq}`,
      });
    } catch (e: unknown) {
      setAlert({ type: "error", msg: `✗ ${e instanceof Error ? e.message : "Erreur inconnue"}` });
    } finally {
      setLoading(false);
    }
  };

  const lookup = async () => {
    setLookupLoading(true);
    try {
      const [a, p] = await Promise.allSettled([
        api.getStock(lookupId),
        api.passiveStock(lookupId),
      ]);
      setStockResult({
        item_id: lookupId,
        active:  a.status === "fulfilled" ? a.value.quantity : null,
        passive: p.status === "fulfilled" ? p.value.quantity : null,
      });
    } finally {
      setLookupLoading(false);
    }
  };

  return (
    <>
      <div className="page-header">
        <div className="page-title">Gestion du stock</div>
        <div className="page-sub">Toutes les écritures passent par le nœud actif — invariant anti-survente garanti</div>
      </div>

      {/* Opération */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Nouvelle opération</div>

        {alert && (
          <div className={`alert alert-${alert.type}`}>{alert.msg}</div>
        )}

        <div className="form-row">
          <div className="form-group">
            <label>Type d'opération</label>
            <select value={opType} onChange={e => setOpType(e.target.value as "sale" | "stock_adjust")}>
              <option value="sale">Vente (−)</option>
              <option value="stock_adjust">Ajustement (+)</option>
            </select>
          </div>
          <div className="form-group">
            <label>Référence article</label>
            <input value={itemId} onChange={e => setItemId(e.target.value)} placeholder="PANTALON-L" />
          </div>
          <div className="form-group">
            <label>Quantité</label>
            <input
              type="number" min={1} value={quantity}
              onChange={e => setQuantity(Math.max(1, Number(e.target.value)))}
              style={{ width: 100 }}
            />
          </div>
          <button className="btn btn-primary" onClick={submit} disabled={loading || !itemId}>
            {loading ? <span className="spin">↻</span> : opType === "sale" ? "Enregistrer la vente" : "Ajuster le stock"}
          </button>
        </div>

        <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
          ⓘ L'opération est chiffrée (XChaCha20-Poly1305) et ajoutée au journal append-only avant confirmation.
        </div>
      </div>

      {/* Consultation */}
      <div className="card">
        <div style={{ fontWeight: 700, marginBottom: 16 }}>Consulter le stock</div>
        <div className="form-row">
          <div className="form-group">
            <label>Référence article</label>
            <input value={lookupId} onChange={e => setLookupId(e.target.value)} placeholder="PANTALON-L" />
          </div>
          <button className="btn btn-ghost" onClick={lookup} disabled={lookupLoading}>
            {lookupLoading ? <span className="spin">↻</span> : "Consulter"}
          </button>
        </div>

        {stockResult && (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Article</th>
                  <th>Nœud Actif (source de vérité)</th>
                  <th>Nœud Passif (réplique)</th>
                  <th>Cohérence</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td style={{ fontFamily: "monospace" }}>{stockResult.item_id}</td>
                  <td style={{ fontWeight: 700 }}>{stockResult.active ?? "—"}</td>
                  <td>{stockResult.passive ?? "—"}</td>
                  <td>
                    {stockResult.active !== null && stockResult.passive !== null ? (
                      stockResult.active === stockResult.passive
                        ? <span className="badge badge-green">✓ Cohérent</span>
                        : <span className="badge badge-yellow">⟳ Sync en cours</span>
                    ) : "—"}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}
