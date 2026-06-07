// Client HTTP vers le nœud actif souverain (sovereign-node-active)
// Les URLs sont lues depuis le localStorage (configurables depuis la page Settings)

// Extrait l'hôte nu d'une saisie (retire schéma, port, chemin).
export function hostOf(u: string): string {
  return u.trim().replace(/^[a-z]+:\/\//i, "").replace(/[:/].*$/, "");
}

// Force le PORT CANONIQUE du service : l'actif tourne toujours sur 3000, le
// relais toujours sur 4000 (ports fixés par les binaires). On normalise donc
// la saisie utilisateur pour éliminer toute erreur de port (.134:3000 → .134:4000).
export function canonicalUrl(u: string, port: number): string {
  const h = hostOf(u);
  return h ? `http://${h}:${port}` : "";
}

// Aucune adresse par défaut : la config est vide tant que l'utilisateur n'a
// pas saisi/découvert ses propres adresses. Les ports sont normalisés.
function getConfig() {
  return {
    activeUrl:  canonicalUrl(localStorage.getItem("sovereign_active_url") ?? "", 3000),
    passiveUrl: localStorage.getItem("sovereign_passive_url") ?? "",
    relayUrl:   canonicalUrl(localStorage.getItem("sovereign_relay_url") ?? "", 4000),
  };
}

const BASE    = () => getConfig().activeUrl;
const PASSIVE = () => getConfig().passiveUrl;
const RELAY   = () => getConfig().relayUrl;

async function get<T>(url: string): Promise<T> {
  const r = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

async function post<T>(url: string, body: unknown): Promise<T> {
  const r = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(5000),
  });
  if (!r.ok) {
    const err = await r.json().catch(() => ({ error: `HTTP ${r.status}` }));
    throw new Error(err.error ?? `HTTP ${r.status}`);
  }
  return r.json();
}

async function del<T>(url: string): Promise<T> {
  const r = await fetch(url, { method: "DELETE", signal: AbortSignal.timeout(5000) });
  if (!r.ok) {
    const err = await r.json().catch(() => ({ error: `HTTP ${r.status}` }));
    throw new Error(err.error ?? `HTTP ${r.status}`);
  }
  return r.json();
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface HealthStatus {
  active:  "ok" | "down" | "loading";
  passive: "ok (passif)" | "down" | "loading";
  relay:   { role: string; status: string; blob_count: number } | null | "loading";
}

export interface StockItem {
  item_id:  string;
  quantity: number;
}

export interface WriteResponse {
  seq:    number;
  op_id:  string;
  status: string;
}

export interface EpochResponse {
  epoch:        number;
  primary_host: string;
}

export interface SyncStatus {
  last_seq: number;
}

export interface JournalEntry {
  seq:             number;
  op_id:           string;
  blob_nonce:      string;
  blob_ciphertext: string;
}

// ── Gestion du parc (enrôlement / rotation / récupération) ──────────────────────

export interface EnrolledDevice {
  device_id:   string;
  label:       string;
  sealed_dek:  string; // sealed box hex (opaque)
  enrolled_at: string;
}

export interface DeviceList {
  devices:            EnrolledDevice[];
  count:              number;
  generation:         number;
  auto_failover_safe: boolean;
}

export interface EnrollResponse {
  device_id:  string;
  sealed_dek: string;
  generation: number;
}

export interface RevokeResponse {
  revoked:            string;
  new_generation:     number;
  remaining:          number;
  auto_failover_safe: boolean;
  quorum_warning:     string | null;
}

// ── Tenant (création de compte) ─────────────────────────────────────────────────

export interface Tenant {
  tenant_id:  string;
  nom:        string;
  gerant:     string;
  email:      string;
  created_at: string;
}

export interface Produit {
  id:         string;
  sku:        string;
  nom:        string;
  prix_cents: number;
}

export interface Client {
  id:        string;
  nom:       string;
  email:     string;
  telephone: string;
}

// ── API calls ─────────────────────────────────────────────────────────────────

export const api = {
  // Santé des 3 nœuds
  async healthActive():  Promise<string>           { return fetch(`${BASE()}/health`, { signal: AbortSignal.timeout(3000) }).then(r => r.text()); },
  async healthPassive(): Promise<string>           { return fetch(`${PASSIVE()}/health`, { signal: AbortSignal.timeout(3000) }).then(r => r.text()); },
  async healthRelay():   Promise<{role:string; status:string; blob_count:number; tenant_count?:number}> { return get(`${RELAY()}/health`); },

  // Stock
  async getStock(item_id: string): Promise<StockItem> { return get(`${BASE()}/stock/${item_id}`); },

  // Écriture (vente ou ajustement)
  async write(op_type: string, item_id: string, quantity: number): Promise<WriteResponse> {
    return post(`${BASE()}/write`, { op_type, item_id, quantity });
  },

  // Époque / fencing
  async getEpoch(): Promise<EpochResponse> { return get(`${BASE()}/epoch`); },
  async promote():  Promise<EpochResponse> { return post(`${BASE()}/epoch/promote`, {}); },

  // Journal
  async getJournal(after_seq = 0, limit = 20): Promise<JournalEntry[]> {
    return get(`${BASE()}/journal?after_seq=${after_seq}&limit=${limit}`);
  },

  // Sync passif
  async syncStatus(): Promise<SyncStatus> { return get(`${PASSIVE()}/sync/status`); },
  async passiveStock(item_id: string): Promise<StockItem> { return get(`${PASSIVE()}/stock/${item_id}`); },

  // Gestion du parc (sur le nœud actif)
  async listDevices(): Promise<DeviceList> { return get(`${BASE()}/devices`); },
  async enrollDevice(public_key: string, label: string): Promise<EnrollResponse> {
    return post(`${BASE()}/devices/enroll`, { public_key, label });
  },
  async revokeDevice(device_id: string): Promise<RevokeResponse> {
    return post(`${BASE()}/devices/revoke`, { device_id });
  },
  async recoverySetup(passphrase: string): Promise<{ status: string }> {
    return post(`${BASE()}/recovery/setup`, { passphrase });
  },
  async recoveryRestore(passphrase: string): Promise<{ dek_hex: string; matches_current: boolean }> {
    return post(`${BASE()}/recovery/restore`, { passphrase });
  },

  // Tenant (création de compte / identité)
  async tenantBootstrap(nom: string, gerant: string, email: string): Promise<Tenant> {
    return post(`${BASE()}/tenant/bootstrap`, { nom, gerant, email });
  },
  async getTenant(): Promise<Tenant> { return get(`${BASE()}/tenant`); },

  // Métier — Produits
  async listProduits(): Promise<Produit[]> { return get(`${BASE()}/produits`); },
  async upsertProduit(p: { id?: string; sku: string; nom: string; prix_cents: number }): Promise<Produit> {
    return post(`${BASE()}/produits`, p);
  },
  async deleteProduit(id: string): Promise<{ deleted: string }> {
    return del(`${BASE()}/produits/${id}`);
  },

  // Métier — Clients
  async listClients(): Promise<Client[]> { return get(`${BASE()}/clients`); },
  async upsertClient(c: { id?: string; nom: string; email: string; telephone: string }): Promise<Client> {
    return post(`${BASE()}/clients`, c);
  },
  async deleteClient(id: string): Promise<{ deleted: string }> {
    return del(`${BASE()}/clients/${id}`);
  },
};
