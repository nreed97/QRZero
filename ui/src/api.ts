import type { Fields, ImportReport, Location, Log, LookupResult, Qso, QsoFilter, Settings, StationCallsign } from "./types";

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
  search: (logId: number, filter: QsoFilter, offset: number, limit: number, sort = "newest") =>
    json<{ total: number; rows: Qso[] }>("POST", `/logs/${logId}/qsos/search`, { filter, offset, limit, sort }),
  lookup: (logId: number, call: string) => json<LookupResult>("GET", `/logs/${logId}/lookup/${encodeURIComponent(call)}`),

  importAdif: async (logId: number, file: Blob, opts: Record<string, string>) => {
    const q = new URLSearchParams(opts).toString();
    return (await request("POST", `/logs/${logId}/import?${q}`, undefined, file)).json() as Promise<ImportReport>;
  },
  exportAdif: async (logId: number, filter: QsoFilter, profile: "standard" | "full") => {
    const resp = await request("POST", `/logs/${logId}/export`, { filter, profile });
    return { text: await resp.text(), count: Number(resp.headers.get("x-qso-count") ?? 0) };
  },

  settings: () => json<Settings>("GET", "/settings"),
  saveSettings: (s: { qrz_enabled?: boolean; qrz_username?: string; qrz_password?: string }) => json<Settings>("PUT", "/settings", s),
  testQrz: () => json<{ ok: boolean }>("POST", "/settings/qrz/test"),
};
