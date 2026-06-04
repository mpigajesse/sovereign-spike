// Client HTTP vers le nœud actif souverain (sovereign-node-active)
// L'URL de base est configurable — par défaut localhost:3000

const BASE = "http://127.0.0.1:3000";
const PASSIVE = "http://192.168.200.130:3001";
const RELAY   = "http://192.168.200.128:4000";

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

// ── API calls ─────────────────────────────────────────────────────────────────

export const api = {
  // Santé des 3 nœuds
  async healthActive():  Promise<string>           { return fetch(`${BASE}/health`, { signal: AbortSignal.timeout(3000) }).then(r => r.text()); },
  async healthPassive(): Promise<string>           { return fetch(`${PASSIVE}/health`, { signal: AbortSignal.timeout(3000) }).then(r => r.text()); },
  async healthRelay():   Promise<{role:string; status:string; blob_count:number}> { return get(`${RELAY}/health`); },

  // Stock
  async getStock(item_id: string): Promise<StockItem> { return get(`${BASE}/stock/${item_id}`); },

  // Écriture (vente ou ajustement)
  async write(op_type: string, item_id: string, quantity: number): Promise<WriteResponse> {
    return post(`${BASE}/write`, { op_type, item_id, quantity });
  },

  // Époque / fencing
  async getEpoch(): Promise<EpochResponse> { return get(`${BASE}/epoch`); },
  async promote():  Promise<EpochResponse> { return post(`${BASE}/epoch/promote`, {}); },

  // Journal
  async getJournal(after_seq = 0, limit = 20): Promise<JournalEntry[]> {
    return get(`${BASE}/journal?after_seq=${after_seq}&limit=${limit}`);
  },

  // Sync passif
  async syncStatus(): Promise<SyncStatus> { return get(`${PASSIVE}/sync/status`); },
  async passiveStock(item_id: string): Promise<StockItem> { return get(`${PASSIVE}/stock/${item_id}`); },
};
