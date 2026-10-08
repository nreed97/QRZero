import { useEffect, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import type { Location, QslConfig, QslDownload, QslOverview, QslRun, QslService, StationCallsign } from "../types";
import Modal from "./Modal";
import PaperQsl from "./PaperQsl";

interface Props {
  logId: number;
  callsigns: StationCallsign[];
  locations: Location[];
  onClose: () => void;
}

function DownloadLine({ d }: { d?: QslDownload }) {
  if (!d) return <span className="muted">Not downloaded yet this session.</span>;
  if (d.running) return <span>Downloading…</span>;
  if (d.error) return <span className="err">{d.error}</span>;
  const when = new Date(d.at * 1000).toISOString().slice(11, 16);
  return (
    <span>
      Last check {when}Z: {d.received} confirmation{d.received === 1 ? "" : "s"}, {d.confirmed} new.
      {d.unmatched_count > 0 && (
        <span className="muted small" title={d.unmatched.join("\n")}> {d.unmatched_count} not found in the log.</span>
      )}
    </span>
  );
}

function RunLine({ run }: { run?: QslRun }) {
  if (!run) return <span className="muted">Not uploaded yet this session.</span>;
  if (run.running) return <span>Uploading…</span>;
  const when = new Date(run.at * 1000).toISOString().slice(11, 16);
  const parts = [`${run.uploaded} uploaded`];
  if (run.duplicates) parts.push(`${run.duplicates} already there`);
  if (run.rejected.length) parts.push(`${run.rejected.length} refused`);
  return (
    <span>
      Last run {when}Z: {parts.join(", ")}.
      {run.error && <span className="err"> {run.error}</span>}
      {run.rejected.length > 0 && <span className="muted small" title={run.rejected.join("\n")}> ({run.rejected.slice(0, 3).join("; ")}{run.rejected.length > 3 ? " …" : ""})</span>}
    </span>
  );
}

export default function QslDialog(props: Props) {
  const [tab, setTab] = useState<"online" | "paper">("online");
  return (
    <Modal title="QSL" onClose={props.onClose} wide>
      <nav className="tabs">
        <button className={tab === "online" ? "active" : ""} onClick={() => setTab("online")}>LoTW, QRZ, Club Log, eQSL</button>
        <button className={tab === "paper" ? "active" : ""} onClick={() => setTab("paper")}>Paper cards</button>
      </nav>
      {tab === "online" ? <Online {...props} /> : <PaperQsl logId={props.logId} />}
    </Modal>
  );
}

function Online({ callsigns, locations }: Props) {
  const [o, setO] = useState<QslOverview | null>(null);
  const [cfg, setCfg] = useState<QslConfig | null>(null);
  const [qrzKeys, setQrzKeys] = useState<Record<string, string>>({});
  const [clPassword, setClPassword] = useState("");
  const [clKey, setClKey] = useState("");
  const [lotwPassword, setLotwPassword] = useState("");
  const [eqslPassword, setEqslPassword] = useState("");
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState("");

  const load = () =>
    api.qsl().then((r) => {
      setO(r);
      setCfg(r.config);
    });

  useEffect(() => {
    load().catch((e) => setMsg({ text: e.message, ok: false }));
    return onLive((e) => {
      if (e.type === "qsl") setO((cur) => (cur ? { ...cur, runs: { ...cur.runs, [e.service]: e.run } } : cur));
      if (e.type === "qsl_download") setO((cur) => (cur ? { ...cur, downloads: { ...cur.downloads, [e.service]: e.run } } : cur));
    });
  }, []);

  if (!o || !cfg) return <p className="muted">Loading…</p>;

  const calls = callsigns.map((c) => c.callsign);
  const set = (patch: Partial<QslConfig>) => setCfg({ ...cfg, ...patch });
  const toggleCall = (list: "qrz_calls" | "clublog_calls" | "eqsl_calls", call: string, on: boolean) =>
    set({ [list]: on ? [...cfg[list].filter((c) => c !== call), call] : cfg[list].filter((c) => c !== call) });

  const save = async () => {
    setMsg(null);
    try {
      const secrets = {
        qrz_keys: Object.fromEntries(Object.entries(qrzKeys).filter(([, v]) => v.trim())),
        ...(clPassword ? { clublog_password: clPassword } : {}),
        ...(clKey ? { clublog_app_key: clKey } : {}),
        ...(lotwPassword ? { lotw_password: lotwPassword } : {}),
        ...(eqslPassword ? { eqsl_password: eqslPassword } : {}),
      };
      const r = await api.saveQsl(cfg, secrets);
      setO(r);
      setCfg(r.config);
      setQrzKeys({});
      setClPassword("");
      setClKey("");
      setLotwPassword("");
      setEqslPassword("");
      setMsg({ text: "Saved.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  const download = async (service: "lotw" | "eqsl") => {
    setBusy(`${service}-rcvd`);
    setMsg(null);
    try {
      await save();
      await api.qslDownload(service);
      await load();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };

  const upload = async (service: QslService) => {
    setBusy(service);
    setMsg(null);
    try {
      await save();
      await api.qslUpload(service);
      await load();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };

  const testKey = async (call: string) => {
    try {
      await save();
      const r = await api.testQrzLogbook(call);
      setMsg({ text: `The key works: it's for the ${r.callsign} logbook.`, ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  const mapping = (call: string, locId: number) => cfg.lotw.find((m) => m.callsign === call && m.location_id === locId);
  const setMapping = (call: string, locId: number, name: string) => {
    const rest = cfg.lotw.filter((m) => !(m.callsign === call && m.location_id === locId));
    set({ lotw: name ? [...rest, { callsign: call, location_id: locId, station_location: name }] : rest });
  };
  const lotwPending = o.pending.lotw.reduce((n, p) => n + p.pending, 0);

  return (
      <div className="qsl">
        <fieldset>
          <legend>LoTW</legend>
          <p className="small muted">
            QRZero signs with your TQSL and uploads when you press the button. Pick the TQSL station location that matches each
            callsign and location.
          </p>
          <div className="row">
            <label className="f w-xl">
              <span>TQSL {o.tqsl.found ? "(found)" : "(not found)"}</span>
              <input value={cfg.tqsl_path} placeholder={o.tqsl.path ?? "C:\\Program Files (x86)\\TrustedQSL\\tqsl.exe"} onChange={(e) => set({ tqsl_path: e.target.value })} />
            </label>
            <label className="f w-m"><span>QSOs from</span><input type="date" value={cfg.lotw_since} onChange={(e) => set({ lotw_since: e.target.value })} /></label>
          </div>
          <table className="list">
            <thead><tr><th>Callsign</th><th>Location</th><th>TQSL station location</th><th>Waiting</th></tr></thead>
            <tbody>
              {calls.flatMap((call) =>
                locations.map((loc) => {
                  const m = mapping(call, loc.id);
                  const pending = o.pending.lotw.find((p) => p.mapping.callsign === call && p.mapping.location_id === loc.id)?.pending;
                  return (
                    <tr key={`${call}-${loc.id}`}>
                      <td className="call">{call}</td>
                      <td>{loc.name}</td>
                      <td>
                        {o.tqsl.locations.length ? (
                          <select value={m?.station_location ?? ""} onChange={(e) => setMapping(call, loc.id, e.target.value)} aria-label={`TQSL location for ${call} at ${loc.name}`}>
                            <option value="">(don't upload)</option>
                            {o.tqsl.locations.map((n) => <option key={n}>{n}</option>)}
                          </select>
                        ) : (
                          <input value={m?.station_location ?? ""} placeholder="name in TQSL" onChange={(e) => setMapping(call, loc.id, e.target.value)} aria-label={`TQSL location for ${call} at ${loc.name}`} />
                        )}
                      </td>
                      <td className="mono">{m ? pending ?? "save to count" : ""}</td>
                    </tr>
                  );
                }),
              )}
            </tbody>
          </table>
          <div className="row">
            <button className="primary" disabled={!!busy || !cfg.lotw.length} onClick={() => upload("lotw")}>
              {busy === "lotw" ? "Signing and uploading…" : `Sign and upload ${lotwPending} QSO${lotwPending === 1 ? "" : "s"} to LoTW`}
            </button>
            <RunLine run={o.runs.lotw} />
          </div>
          <p className="small muted">To download confirmations (for awards), enter your LoTW website login. It's not your TQSL password.</p>
          <div className="row">
            <label className="f w-m"><span>LoTW username</span><input value={cfg.lotw_username} onChange={(e) => set({ lotw_username: e.target.value })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={lotwPassword} placeholder={o.secrets.lotw_password ? "saved" : ""} onChange={(e) => setLotwPassword(e.target.value)} /></label>
            <button disabled={!!busy || !cfg.lotw_username} onClick={() => download("lotw")}>{busy === "lotw-rcvd" ? "Downloading…" : "Download confirmations"}</button>
            <DownloadLine d={o.downloads.lotw} />
          </div>
        </fieldset>

        <fieldset>
          <legend>QRZ Logbook</legend>
          <label className="check"><input type="checkbox" checked={cfg.qrz_enabled} onChange={(e) => set({ qrz_enabled: e.target.checked })} /> Upload new QSOs every {cfg.interval_min} minutes</label>
          <p className="small muted">Each logbook on QRZ.com has its own API key (QRZ Logbook, Settings, API). QRZ needs an XML subscription for the API.</p>
          <table className="list">
            <tbody>
              {calls.map((call) => (
                <tr key={call}>
                  <td><label className="check"><input type="checkbox" checked={cfg.qrz_calls.includes(call)} onChange={(e) => toggleCall("qrz_calls", call, e.target.checked)} /> {call}</label></td>
                  <td>
                    <input
                      type="password"
                      value={qrzKeys[call] ?? ""}
                      placeholder={o.secrets.qrz_calls.includes(call) ? "API key saved" : "API key"}
                      onChange={(e) => setQrzKeys({ ...qrzKeys, [call]: e.target.value })}
                      aria-label={`QRZ API key for ${call}`}
                    />
                  </td>
                  <td>{(o.secrets.qrz_calls.includes(call) || qrzKeys[call]) && <button className="tiny" onClick={() => testKey(call)}>Test</button>}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="row">
            <label className="f w-m"><span>QSOs from</span><input type="date" value={cfg.qrz_since} onChange={(e) => set({ qrz_since: e.target.value })} /></label>
            <button disabled={!!busy || !cfg.qrz_calls.length} onClick={() => upload("qrz")}>{busy === "qrz" ? "Uploading…" : `Upload ${o.pending.qrz} now`}</button>
            <RunLine run={o.runs.qrz} />
          </div>
        </fieldset>

        <fieldset>
          <legend>Club Log</legend>
          <label className="check"><input type="checkbox" checked={cfg.clublog_enabled} onChange={(e) => set({ clublog_enabled: e.target.checked })} /> Upload new QSOs every {cfg.interval_min} minutes</label>
          <div className="row">
            <label className="f w-l"><span>Club Log email</span><input value={cfg.clublog_email} onChange={(e) => set({ clublog_email: e.target.value })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={clPassword} placeholder={o.secrets.clublog_password ? "saved" : ""} onChange={(e) => setClPassword(e.target.value)} /></label>
            <label className="f w-l"><span>Club Log API key</span><input type="password" value={clKey} placeholder={o.secrets.clublog_app_key ? "saved" : "from clublog.org"} onChange={(e) => setClKey(e.target.value)} /></label>
          </div>
          <div className="row">
            <span className="muted small">Callsigns:</span>
            {calls.map((call) => (
              <label key={call} className="check"><input type="checkbox" checked={cfg.clublog_calls.includes(call)} onChange={(e) => toggleCall("clublog_calls", call, e.target.checked)} /> {call}</label>
            ))}
          </div>
          <div className="row">
            <label className="f w-m"><span>QSOs from</span><input type="date" value={cfg.clublog_since} onChange={(e) => set({ clublog_since: e.target.value })} /></label>
            <button disabled={!!busy || !cfg.clublog_calls.length} onClick={() => upload("clublog")}>{busy === "clublog" ? "Uploading…" : `Upload ${o.pending.clublog} now`}</button>
            <RunLine run={o.runs.clublog} />
          </div>
        </fieldset>

        <fieldset>
          <legend>eQSL</legend>
          <label className="check"><input type="checkbox" checked={cfg.eqsl_enabled} onChange={(e) => set({ eqsl_enabled: e.target.checked })} /> Upload new QSOs every {cfg.interval_min} minutes</label>
          <div className="row">
            <label className="f w-m"><span>eQSL username</span><input value={cfg.eqsl_username} onChange={(e) => set({ eqsl_username: e.target.value.toUpperCase() })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={eqslPassword} placeholder={o.secrets.eqsl_password ? "saved" : ""} onChange={(e) => setEqslPassword(e.target.value)} /></label>
            <label className="f w-m"><span>QTH nickname (optional)</span><input value={cfg.eqsl_nickname} onChange={(e) => set({ eqsl_nickname: e.target.value })} /></label>
          </div>
          <div className="row">
            <span className="muted small">Callsigns:</span>
            {calls.map((call) => (
              <label key={call} className="check"><input type="checkbox" checked={cfg.eqsl_calls.includes(call)} onChange={(e) => toggleCall("eqsl_calls", call, e.target.checked)} /> {call}</label>
            ))}
          </div>
          <div className="row">
            <label className="f w-m"><span>QSOs from</span><input type="date" value={cfg.eqsl_since} onChange={(e) => set({ eqsl_since: e.target.value })} /></label>
            <button disabled={!!busy || !cfg.eqsl_calls.length} onClick={() => upload("eqsl")}>{busy === "eqsl" ? "Uploading…" : `Upload ${o.pending.eqsl} now`}</button>
            <RunLine run={o.runs.eqsl} />
          </div>
          <div className="row">
            <button disabled={!!busy || !cfg.eqsl_username} onClick={() => download("eqsl")}>{busy === "eqsl-rcvd" ? "Downloading…" : "Download confirmations"}</button>
            <DownloadLine d={o.downloads.eqsl} />
          </div>
        </fieldset>

        <div className="row">
          <label className="f w-m">
            <span>Upload every (min)</span>
            <input value={cfg.interval_min} inputMode="numeric" onChange={(e) => set({ interval_min: Math.max(1, Number(e.target.value) || 15) })} />
          </label>
          <label className="check"><input type="checkbox" checked={cfg.confirm_daily} onChange={(e) => set({ confirm_daily: e.target.checked })} /> Download LoTW and eQSL confirmations once a day</label>
          <span className="spacer" />
          {msg && <span className={msg.ok ? "ok" : "err"}>{msg.text}</span>}
          <button className="primary" onClick={save}>Save</button>
        </div>
      </div>
  );
}
