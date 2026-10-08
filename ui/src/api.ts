import type { PropagationReport } from "./types";
import type { QslService, QslDownload, AwardKind, AwardTable, ClusterConfig, ClusterSnapshot, QslConfig, QslOverview, QslRun, QslSecrets, CtyStatus, Equipment, Fields, FtxDecode, FtxInstance, IntegrationStatus, Integrations, ImportReport, Location, Log, LookupResult, Note, Qso, QsoFilter, Settings, StationCallsign, WatchEntry, WatchHit, CtyEntityInfo } from "./types";

// The session token arrives once in the URL (?token=...) and is kept for this tab.
function sessionToken(): string {
  const url = new URL(window.location.href);
  const fromUrl = url.searchParams.get("token");
  if (fromUrl) {
    try { sessionStorage.setItem("qrzero.token", fromUrl); } catch { /* storage unavailable */ }
    url.searchParams.delete("token");
    window.history.replaceState(null, "", url.toString());
    return fromUrl;
  }
  try { return sessionStorage.getItem("qrzero.token") ?? ""; } catch { return ""; }
}

const token = sessionToken();

export class ApiError extends Error {}

async function request(method: string, path: string, body?: unknown, raw?: BodyInit): Promise<Response> {
  const headers: Record<string, string> = { "x-qrzero-token": token };
  let payload: BodyInit | undefined = raw;
  if (body !== undefined) {
    headers["content-type"] = "application/json";
    payload = JSON.stringify(body);
  }
  const resp = await fetch(`/api${path}`, { method, headers, body: payload });
  if (!resp.ok) {
    let msg = `${resp.status} ${resp.statusText}`;
    try { msg = (await resp.json()).error ?? msg; } catch { /* not JSON */ }
    throw new ApiError(msg);
  }
  return resp;
}

async function json<T>(method: string, path: string, body?: unknown): Promise<T> {
  return (await request(method, path, body)).json();
}

export const api = {
  token: () => token,
  hasToken: () => token !== "",
  info: () => json<{ version: string; data_dir: string }>("GET", "/info"),

  logs: () => json<Log[]>("GET", "/logs"),
  createLog: (name: string) => json<Log>("POST", "/logs", { name }),
  renameLog: (id: number, name: string) => json<null>("PUT", `/logs/${id}`, { name }),
  deleteLog: (id: number) => json<null>("DELETE", `/logs/${id}`),

  callsigns: (logId: number) => json<StationCallsign[]>("GET", `/logs/${logId}/callsigns`),
  addCallsign: (logId: number, callsign: string) => json<StationCallsign>("POST", `/logs/${logId}/callsigns`, { callsign }),
  deleteCallsign: (id: number) => json<null>("DELETE", `/callsigns/${id}`),
  defaultCallsign: (id: number) => json<null>("POST", `/callsigns/${id}/default`),

  locations: (logId: number) => json<Location[]>("GET", `/logs/${logId}/locations`),
  createLocation: (logId: number, name: string, fields: Fields) => json<Location>("POST", `/logs/${logId}/locations`, { name, fields }),
  updateLocation: (id: number, name: string, fields: Fields) => json<Location>("PUT", `/locations/${id}`, { name, fields }),
  deleteLocation: (id: number) => json<null>("DELETE", `/locations/${id}`),
  defaultLocation: (id: number) => json<null>("POST", `/locations/${id}/default`),

  logQso: (logId: number, locationId: number | null, fields: Fields) =>
    json<Qso>("POST", `/logs/${logId}/qsos`, { location_id: locationId, fields }),
  updateQso: (id: number, locationId: number | null, fields: Fields) =>
    json<Qso>("PUT", `/qsos/${id}`, { location_id: locationId, fields }),
  deleteQsos: (ids: number[]) => json<number>("POST", "/qsos/delete", { ids }),
  markQsos: (ids: number[], fields: Fields) => json<number>("POST", "/qsos/mark", { ids, fields }),
  award: (logId: number, award: AwardKind, o: { calls: string[]; lotw: boolean; paper: boolean; eqsl: boolean; unworked: boolean }) => {
    const q = new URLSearchParams({ calls: o.calls.join(","), lotw: String(o.lotw), paper: String(o.paper), eqsl: String(o.eqsl), unworked: String(o.unworked) });
    return json<AwardTable>("GET", `/logs/${logId}/awards/${award}?${q}`);
  },
  paperQueue: (logId: number) => json<Qso[]>("GET", `/logs/${logId}/paper-queue`),
  search: (logId: number, filter: QsoFilter, offset: number, limit: number, sort = "newest") =>
    json<{ total: number; rows: Qso[] }>("POST", `/logs/${logId}/qsos/search`, { filter, offset, limit, sort }),
  lookup: (logId: number, call: string) => json<LookupResult>("GET", `/logs/${logId}/lookup/${encodeURIComponent(call)}`),
  notes: (logId: number, q: string, offset: number, limit: number) =>
    json<{ total: number; rows: Note[] }>("GET", `/logs/${logId}/notes?${new URLSearchParams({ q, offset: String(offset), limit: String(limit) })}`),
  note: (logId: number, call: string) => json<Note | null>("GET", `/logs/${logId}/notes/${encodeURIComponent(call)}`),
  setNote: (logId: number, call: string, text: string) => json<Note | null>("PUT", `/logs/${logId}/notes/${encodeURIComponent(call)}`, { text }),
  deleteNote: (logId: number, call: string) => json<boolean>("DELETE", `/logs/${logId}/notes/${encodeURIComponent(call)}`),

  importAdif: async (logId: number, file: Blob, opts: Record<string, string>) => {
    const q = new URLSearchParams(opts).toString();
    return (await request("POST", `/logs/${logId}/import?${q}`, undefined, file)).json() as Promise<ImportReport>;
  },
  exportAdif: async (logId: number, filter: QsoFilter, profile: "standard" | "full") => {
    const resp = await request("POST", `/logs/${logId}/export`, { filter, profile });
    return { text: await resp.text(), count: Number(resp.headers.get("x-qso-count") ?? 0) };
  },

  equipment: (logId: number) => json<Equipment[]>("GET", `/logs/${logId}/equipment`),
  createEquipment: (locationId: number, kind: string, name: string, fields: Fields) =>
    json<Equipment>("POST", `/locations/${locationId}/equipment`, { kind, name, fields }),
  updateEquipment: (id: number, e: { location_id: number; kind: string; name: string; fields: Fields }) =>
    json<Equipment>("PUT", `/equipment/${id}`, e),
  moveEquipment: (id: number, delta: number) => json<null>("POST", `/equipment/${id}/move`, { delta }),
  deleteEquipment: (id: number) => json<null>("DELETE", `/equipment/${id}`),

  getPref: <T>(key: string) => json<T | null>("GET", `/prefs/${key}`),
  setPref: (key: string, value: unknown) => json<null>("PUT", `/prefs/${key}`, value),

  settings: () => json<Settings>("GET", "/settings"),
  saveSettings: (s: { qrz_enabled?: boolean; qrz_username?: string; qrz_password?: string }) => json<Settings>("PUT", "/settings", s),
  testQrz: () => json<{ ok: boolean }>("POST", "/settings/qrz/test"),

  setActive: (logId: number, locationId: number | null, stationCallsign: string) =>
    json<null>("POST", "/station/active", { log_id: logId, location_id: locationId, station_callsign: stationCallsign }),
  tune: (key: string, freqHz?: number, mode?: string) => json<null>("POST", "/radios/tune", { key, freq_hz: freqHz, mode }),
  integrations: () => json<{ config: Integrations; status: IntegrationStatus }>("GET", "/integrations"),
  saveIntegrations: (c: Integrations) => json<{ config: Integrations; status: IntegrationStatus }>("PUT", "/integrations", c),
  ftx: () => json<{ instances: FtxInstance[]; decodes: FtxDecode[] }>("GET", "/ftx"),
  ftxReply: (seq: number) => json<null>("POST", "/ftx/reply", { seq }),
  rotate: (azimuth: number) => json<null>("POST", "/rotator", { azimuth }),
  cty: () => json<CtyStatus>("GET", "/cty"),
  updateCty: () => json<CtyStatus>("POST", "/cty/update"),
  ctyEntities: () => json<CtyEntityInfo[]>("GET", "/cty/entities"),
  watch: () => json<WatchEntry[]>("GET", "/watch"),
  saveWatch: (entries: WatchEntry[]) => json<WatchEntry[]>("PUT", "/watch", entries),
  watchHits: () => json<WatchHit[]>("GET", "/watch/hits"),
  installCty: async (file: Blob) => (await request("POST", "/cty", undefined, file)).json() as Promise<CtyStatus>,

  cluster: () => json<ClusterSnapshot>("GET", "/cluster"),
  saveCluster: (c: ClusterConfig) => json<ClusterSnapshot>("PUT", "/cluster", c),
  clusterConnect: (connect: boolean) => json<null>("POST", "/cluster/connect", { connect }),
  clusterSend: (line: string) => json<null>("POST", "/cluster/send", { line }),
  qsl: () => json<QslOverview>("GET", "/qsl"),
  saveQsl: (config: QslConfig, secrets: QslSecrets) => json<QslOverview>("PUT", "/qsl", { config, secrets }),
  testQrzLogbook: (callsign: string) => json<{ callsign: string }>("POST", "/qsl/qrz/test", { callsign }),
  qslUpload: (service: QslService) => json<QslRun>("POST", `/qsl/upload/${service}`),
  qslDownload: (service: "lotw" | "eqsl") => json<QslDownload>("POST", `/qsl/download/${service}`),
  propagation: (refresh = false) => json<PropagationReport>("GET", `/propagation${refresh ? "?refresh=true" : ""}`),
};
