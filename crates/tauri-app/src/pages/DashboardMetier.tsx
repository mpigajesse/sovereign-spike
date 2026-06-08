import { useEffect, useState, useCallback } from "react";
import { api } from "../api";

// Tableau de bord du profil "métier" (poste enrôlé, standby/relais) — aucune
// information de cluster ici (le cluster reste invisible pour ces postes,
// cf. UserProfile dans App.tsx). On montre uniquement un résumé de l'activité
// commerciale, à partir des mêmes API que les pages Produits/Clients.
export default function DashboardMetier() {
  const [produitsCount, setProduitsCount] = useState<number | null>(null);
  const [clientsCount,  setClientsCount]  = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try { setProduitsCount((await api.listProduits()).length); } catch { setProduitsCount(null); }
    try { setClientsCount((await api.listClients()).length); } catch { setClientsCount(null); }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

  return (
    <>
      <div className="page-header">
        <div className="page-title">Tableau de bord</div>
        <div className="page-sub">Vue d'ensemble de votre activité</div>
      </div>

      <div className="cards">
        <div className="card">
          <div className="card-label">Produits au catalogue</div>
          <div className="card-value">{produitsCount ?? "—"}</div>
          <div className="card-sub">référencés</div>
        </div>
        <div className="card">
          <div className="card-label">Clients enregistrés</div>
          <div className="card-value">{clientsCount ?? "—"}</div>
          <div className="card-sub">au carnet</div>
        </div>
      </div>

      <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: "20px" }}>
        <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 14 }}>Bienvenue</div>
        <div style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.6 }}>
          Utilisez le menu pour gérer vos produits, vos clients et votre stock.
          Vos données sont chiffrées et synchronisées automatiquement avec le
          reste de votre entreprise.
        </div>
      </div>
    </>
  );
}
