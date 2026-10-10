import { useEffect, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import type { Location, QslConfig, QslDownload, QslOverview, QslRun, QslService, StationCallsign } from "../types";
import SaveBar from "./SaveBar";
import Modal from "./Modal";
import PaperQsl from "./PaperQsl";
import QslLookup from "./QslLookup";
import ReplyList from "./ReplyList";
import { newConfirmLines } from "../newConfirms";

interface Props {
  logId: number;
  callsigns: StationCallsign[];
  locations: Location[];
  onClose: () => void;
}

/** What the last download counted toward the awards. */
function NewAwards({ d, service }: { d: QslDownload; service: "lotw" | "eqsl" }) {
  const lines = newConfirmLines(d, service);
  if (!d.confirmed) return null;
  if (!lines.length) return <p className="small muted">Nothing new toward your awards.</p>;
  const shown = lines.slice(0, 200);
  return (
    <div className="new-confirms">
      <b>New toward your awards ({lines.length})</b>
      <table className="mono small">
        <tbody>
          {shown.map((l, i) => (
            <tr key={i}>
              <td>{l.awardName}</td>
              <td>{l.name}</td>
              <td>{l.what}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {lines.length > shown.length && <p className="small muted">… and {lines.length - shown.length} more.</p>}
    </div>
  );
}

function DownloadLine({ d }: { d?: QslDownload }) {
  if (!d) return <span className="muted">Not downloaded yet this session.</span>;
  if (d.running) return <span>Downloading…</span>;
  if (d.error) return <span className="err">{d.error}</span>;
  const when = new Date(d.at * 1000).toISOString().slice(11, 16);
  return (
    <span>
      Last check {when}Z{d.auto ? " (automatic)" : ""}: {d.received} confirmation{d.received === 1 ? "" : "s"}, {d.confirmed} new.
      {d.unmatched_count > 0 && (
        <details className="small">
          <summary>Review {d.unmatched_count} not found in the log</summary>
          <p className="muted">These are on the service but match no QSO in your log. Nothing was added to the log.</p>
          <pre className="mono">{d.unmatched.join("\n")}{d.unmatched_count > d.unmatched.length ? `\n… and ${d.unmatched_count - d.unmatched.length} more` : ""}</pre>
        </details>
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

type Tab = "lookup" | "cards" | "reply" | "online";

export default function QslDialog(props: Props & { tab?: Tab }) {
  const [tab, setTab] = useState<Tab>(props.tab ?? "lookup");
  const tabs: [Tab, string][] = [["lookup", "QSL Detail Lookup"], ["cards", "Cards to send"], ["reply", "To reply to"], ["online", "Online services"]];
  return (
    <Modal title="QSL" onClose={props.onClose} wide>
      <nav className="tabs">
        {tabs.map(([k, label]) => <button key={k} className={tab === k ? "active" : ""} onClick={() => setTab(k)}>{label}</button>)}
      </nav>
      {tab === "lookup" ? <QslLookup logId={props.logId} /> : tab === "cards" ? <PaperQsl logId={props.logId} /> : tab === "reply" ? <ReplyList logId={props.logId} /> : <Online {...props} />}
    </Modal>
  );
}

function Online({ callsigns, locations, onClose }: Props) {
  const [o, setO] = useState<QslOverview | null>(null);
  const [cfg, setCfg] = useState<QslConfig | null>(null);
  const [qrzKeys, setQrzKeys] = useState<Record<string, string>>({});
  const [clPassword, setClPassword] = useState("");
  const [lotwPassword, setLotwPassword] = useState("");
  const [eqslPassword, setEqslPassword] = useState("");
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState("");
  const [svc, setSvc] = useState<"lotw" | "qrz" | "clublog" | "eqsl">("lotw");

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


  const [rangeFrom, setRangeFrom] = useState("");
  const [rangeTo, setRangeTo] = useState(() => new Date().toISOString().slice(0, 10));
  const [waiting, setWaiting] = useState<number | null>(null);
  const rangeOk = !!rangeFrom && !!rangeTo && rangeFrom <= rangeTo;
  const mapped = (cfg?.lotw ?? []).map((m) => m.location_id).join(",");
  useEffect(() => {
    setWaiting(null);
    if (!rangeOk) return;
    let live = true;
    api.lotwWaiting(rangeFrom, rangeTo).then((w) => live && setWaiting(w.locations.reduce((n, l) => n + l.waiting, 0)), () => {});
    return () => {
      live = false;
    };
  }, [rangeFrom, rangeTo, rangeOk, mapped, o?.runs.lotw]);
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
        ...(lotwPassword ? { lotw_password: lotwPassword } : {}),
        ...(eqslPassword ? { eqsl_password: eqslPassword } : {}),
      };
      const r = await api.saveQsl(cfg, secrets);
      setO(r);
      setCfg(r.config);
      setQrzKeys({});
      setClPassword("");
      setLotwPassword("");
      setEqslPassword("");
      setMsg({ text: "Saved.", ok: true });
      return true;
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
      return false;
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
  const uploadRange = async () => {
    setBusy("lotw");
    setMsg(null);
    try {
      await save();
      await api.lotwUploadRange(rangeFrom, rangeTo);
      await load();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };
  const lotwPending = o.pending.lotw.reduce((n, p) => n + p.pending, 0);

  return (
      <div className="qsl">
        <nav className="tabs sub" aria-label="Service">
          {([["lotw", "LoTW"], ["qrz", "QRZ Logbook"], ["clublog", "Club Log"], ["eqsl", "eQSL"]] as const).map(([k, label]) => (
            <button key={k} className={svc === k ? "active" : ""} onClick={() => setSvc(k)}>{label}</button>
          ))}
        </nav>
        {svc === "lotw" && (
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
          <details className="more">
            <summary>Upload by date range, moving from another logger</summary>
          <div className="row">
            <label className="f w-m"><span>Upload needed, from</span><input type="date" value={rangeFrom} onChange={(e) => setRangeFrom(e.target.value)} /></label>
            <label className="f w-m"><span>to</span><input type="date" value={rangeTo} onChange={(e) => setRangeTo(e.target.value)} /></label>
            <button disabled={!!busy || !cfg.lotw.length || !rangeOk || !waiting} onClick={uploadRange}>
              {busy === "lotw" ? "Signing and uploading…" : `Sign and upload ${waiting ?? 0} QSO${waiting === 1 ? "" : "s"} in this range`}
            </button>
          </div>
          <p className="small muted">Picks every QSO in the range whose LoTW sent status is N or blank, whatever the "QSOs from" date says.</p>
          <p className="small muted"><b>Moving from another logger?</b> Download confirmations before your first upload. Matching QSOs are marked sent and confirmed, so they aren't uploaded again.</p>
          </details>
          <p className="small muted">To download confirmations (for awards), enter your LoTW website login. It's not your TQSL password.</p>
          <div className="row">
            <label className="f w-m"><span>LoTW username</span><input value={cfg.lotw_username} onChange={(e) => set({ lotw_username: e.target.value })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={lotwPassword} placeholder={o.secrets.lotw_password ? "saved" : ""} onChange={(e) => setLotwPassword(e.target.value)} /></label>
            <button disabled={!!busy || !cfg.lotw_username} onClick={() => download("lotw")}>{busy === "lotw-rcvd" ? "Downloading…" : "Download confirmations"}</button>
            <DownloadLine d={o.downloads.lotw} />
          </div>
          {o.downloads.lotw && !o.downloads.lotw.running && !o.downloads.lotw.error && <NewAwards d={o.downloads.lotw} service="lotw" />}
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.lotw_download_enabled} onChange={(e) => set({ lotw_download_enabled: e.target.checked })} /> Download confirmations automatically</label>
            <label className="f w-s"><span>Every (min)</span><input value={cfg.lotw_download_interval_min} disabled={!cfg.lotw_download_enabled} inputMode="numeric" onChange={(e) => set({ lotw_download_interval_min: Math.max(1, Number(e.target.value) || 1440) })} /></label>
          </div>
        </fieldset>
        )}

        {svc === "qrz" && (
        <fieldset>
          <legend>QRZ Logbook</legend>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.qrz_enabled} onChange={(e) => set({ qrz_enabled: e.target.checked })} /> Upload new QSOs automatically</label>
            <label className="f w-s"><span>Every (min)</span><input value={cfg.qrz_interval_min} disabled={!cfg.qrz_enabled} inputMode="numeric" onChange={(e) => set({ qrz_interval_min: Math.max(1, Number(e.target.value) || 15) })} /></label>
          </div>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.qrz_live} onChange={(e) => set({ qrz_live: e.target.checked })} /> Upload shortly after a QSO is logged or edited</label>
            <label className="f w-s"><span>Wait (min)</span><input value={cfg.qrz_live_delay_min} disabled={!cfg.qrz_live} inputMode="numeric" onChange={(e) => set({ qrz_live_delay_min: Math.min(60, Math.max(1, Number(e.target.value) || 2)) })} /></label>
          </div>
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
        )}

        {svc === "clublog" && (
        <fieldset>
          <legend>Club Log</legend>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.clublog_enabled} onChange={(e) => set({ clublog_enabled: e.target.checked })} /> Upload new QSOs automatically</label>
            <label className="f w-s"><span>Every (min)</span><input value={cfg.clublog_interval_min} disabled={!cfg.clublog_enabled} inputMode="numeric" onChange={(e) => set({ clublog_interval_min: Math.max(1, Number(e.target.value) || 15) })} /></label>
          </div>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.clublog_live} onChange={(e) => set({ clublog_live: e.target.checked })} /> Upload shortly after a QSO is logged or edited</label>
            <label className="f w-s"><span>Wait (min)</span><input value={cfg.clublog_live_delay_min} disabled={!cfg.clublog_live} inputMode="numeric" onChange={(e) => set({ clublog_live_delay_min: Math.min(60, Math.max(1, Number(e.target.value) || 2)) })} /></label>
          </div>
          <div className="row">
            <label className="f w-l"><span>Club Log email</span><input value={cfg.clublog_email} onChange={(e) => set({ clublog_email: e.target.value })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={clPassword} placeholder={o.secrets.clublog_password ? "saved" : ""} onChange={(e) => setClPassword(e.target.value)} /></label>
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
        )}

        {svc === "eqsl" && (
        <fieldset>
          <legend>eQSL</legend>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.eqsl_enabled} onChange={(e) => set({ eqsl_enabled: e.target.checked })} /> Upload new QSOs automatically</label>
            <label className="f w-s"><span>Every (min)</span><input value={cfg.eqsl_interval_min} disabled={!cfg.eqsl_enabled} inputMode="numeric" onChange={(e) => set({ eqsl_interval_min: Math.max(1, Number(e.target.value) || 15) })} /></label>
          </div>
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.eqsl_live} onChange={(e) => set({ eqsl_live: e.target.checked })} /> Upload shortly after a QSO is logged or edited</label>
            <label className="f w-s"><span>Wait (min)</span><input value={cfg.eqsl_live_delay_min} disabled={!cfg.eqsl_live} inputMode="numeric" onChange={(e) => set({ eqsl_live_delay_min: Math.min(60, Math.max(1, Number(e.target.value) || 2)) })} /></label>
          </div>
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
          {o.downloads.eqsl && !o.downloads.eqsl.running && !o.downloads.eqsl.error && <NewAwards d={o.downloads.eqsl} service="eqsl" />}
          <div className="row">
            <label className="check"><input type="checkbox" checked={cfg.confirm_daily} onChange={(e) => set({ confirm_daily: e.target.checked })} /> Download eQSL confirmations once a day</label>
          </div>
        </fieldset>
        )}

        <SaveBar msg={msg} onSave={save} onClose={onClose} />
      </div>
  );
}
