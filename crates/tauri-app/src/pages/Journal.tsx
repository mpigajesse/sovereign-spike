import { useEffect, useState } from "react";
import { api, JournalEntry } from "../api";

export default function Journal() {
  const [entries,  setEntries]  = useState<JournalEntry[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [afterSeq, setAfterSeq] = useState(0);

  const load = async () => {
    setLoading(true);
    try {
      const j = await api.getJournal(afterSeq, 20);
      setEntries(j);
    } catch {
      setEntries([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, [afterSeq]);

  return (
    <>
      <div className="page-header">
        <div className="page-title">Journal chiffré</div>
        <div className="page-sub">
          Blobs XChaCha20-Poly1305 — opaques sur le réseau. Le relais ne voit que ces octets.
        </div>
      </div>

      {/* Explication */}
      <div style={{
        background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius)",
        padding: "16px 20px", marginBottom: 20, fontSize: 13, lineHeight: 1.7
      }}>
        <strong>Propriété zéro-knowledge :</strong> chaque ligne ci-dessous est ce que le relais éditeur voit.
        Le <code style={{ background: "var(--surface2)", padding: "1px 6px", borderRadius: 4 }}>blob_nonce</code> et
        le <code style={{ background: "var(--surface2)", padding: "1px 6px", borderRadius: 4 }}>blob_ciphertext</code> sont
        des octets aléatoires sans la DEK. Le relais ne peut rien en déduire.
      </div>

      <div style={{ display: "flex", gap: 12, marginBottom: 16, alignItems: "center" }}>
        <div className="form-group">
          <label>Afficher depuis seq</label>
          <input
            type="number" min={0} value={afterSeq}
            onChange={e => setAfterSeq(Math.max(0, Number(e.target.value)))}
            style={{ width: 100 }}
          />
        </div>
        <button className="btn btn-ghost" onClick={load} style={{ marginTop: 22 }}>
          {loading ? <span className="spin">↻</span> : "↻ Actualiser"}
        </button>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>seq</th>
              <th>op_id (UUID)</th>
              <th>blob_nonce (24 oct. hex)</th>
              <th>blob_ciphertext (extrait)</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr><td colSpan={4} style={{ textAlign: "center", color: "var(--text-muted)" }}>Chargement…</td></tr>
            ) : entries.length === 0 ? (
              <tr><td colSpan={4} style={{ textAlign: "center", color: "var(--text-muted)" }}>Aucune entrée après seq={afterSeq}</td></tr>
            ) : entries.map(e => (
              <tr key={e.seq}>
                <td style={{ fontWeight: 700 }}>{e.seq}</td>
                <td style={{ fontFamily: "monospace", fontSize: 11 }}>{e.op_id.substring(0, 18)}…</td>
                <td style={{ fontFamily: "monospace", fontSize: 11, color: "var(--text-muted)" }}>
                  {e.blob_nonce.substring(0, 24)}…
                </td>
                <td style={{ fontFamily: "monospace", fontSize: 11, color: "var(--text-muted)" }}>
                  {e.blob_ciphertext.substring(0, 32)}…
                  <span style={{ color: "var(--accent2)", marginLeft: 6, fontSize: 10 }}>opaque</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 8 }}>
        {entries.length > 0 && `${entries.length} entrée(s) affichée(s) — hash chaîné SHA-256 entre chaque entrée`}
      </div>
    </>
  );
}
