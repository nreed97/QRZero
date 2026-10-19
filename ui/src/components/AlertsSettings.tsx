import { useEffect, useState } from "react";
import { api } from "../api";
import type { NtfyConfig, NtfyView } from "../types";

/** A topic nobody will guess: anyone who knows a topic on a public ntfy server can read it. */
function randomTopic(): string {
  const bytes = new Uint8Array(12);
  crypto.getRandomValues(bytes);
  return `qrzero-${Array.from(bytes, (b) => "abcdefghijkmnpqrstuvwxyz23456789"[b % 32]).join("")}`;
}

/** Push alerts to a phone or desktop through ntfy (ntfy.sh is free; a self-hosted server works too). */
export default function AlertsTab() {
  const [view, setView] = useState<NtfyView | null>(null);
  const [cfg, setCfg] = useState<NtfyConfig | null>(null);
  const [token, setToken] = useState("");
  const [msg, setMsg] = useState<{ text: string; err: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.alerts().then((v) => (setView(v), setCfg(v.config))).catch((e) => setMsg({ text: e.message, err: true }));
  }, []);
  if (!cfg || !view) return msg ? <p className={msg.err ? "err" : ""}>{msg.text}</p> : null;

  const set = (patch: Partial<NtfyConfig>) => setCfg({ ...cfg, ...patch });
  const dirty = JSON.stringify(cfg) !== JSON.stringify(view.config) || token !== "";
  const save = async (): Promise<boolean> => {
    try {
      const v = await api.putAlerts(cfg, token === "" ? undefined : token);
      setView(v);
      setCfg(v.config);
      setToken("");
      setMsg({ text: "Saved.", err: false });
      return true;
    } catch (e) {
      setMsg({ text: (e as Error).message, err: true });
      return false;
    }
  };
  const test = async () => {
    setBusy(true);
    setMsg(null);
    try {
      if (dirty && !(await save())) return;
      const v = await api.testAlerts();
      setView(v);
      setMsg({ text: `Test sent to ${cfg.topic}. It should be on your phone in a moment.`, err: false });
    } catch (e) {
      setMsg({ text: (e as Error).message, err: true });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="general-tab">
      <p className="muted">
        QRZero can send its alerts to your phone or another computer through <b>ntfy</b>. Install the ntfy app, subscribe to the topic below, and alerts arrive there even when you are away from the radio. ntfy.sh is free and needs no account.
      </p>
      <fieldset>
        <legend>ntfy</legend>
        <label className="check">
          <input type="checkbox" checked={cfg.enabled} onChange={(e) => set({ enabled: e.target.checked, topic: e.target.checked && !cfg.topic ? randomTopic() : cfg.topic })} /> Send alerts through ntfy
        </label>
        <div className="row">
          <label className="f">
            <span>Server</span>
            <input value={cfg.server} onChange={(e) => set({ server: e.target.value })} placeholder="https://ntfy.sh" />
          </label>
          <label className="f">
            <span>Topic</span>
            <input className="mono" value={cfg.topic} onChange={(e) => set({ topic: e.target.value.replace(/[^A-Za-z0-9_-]/g, "") })} placeholder="qrzero-..." />
          </label>
          <button onClick={() => set({ topic: randomTopic() })} title="Make up a new topic nobody will guess">New topic</button>
        </div>
        <p className="small muted">On ntfy.sh anyone who knows the topic can read it, so keep the made-up name. For a protected topic (your own server, or a reserved ntfy.sh topic), enter an access token.</p>
        <div className="row">
          <label className="f">
            <span>Access token (optional){view.has_token ? ", one is saved" : ""}</span>
            <input type="password" value={token} onChange={(e) => setToken(e.target.value)} placeholder={view.has_token ? "leave blank to keep it" : "tk_..."} autoComplete="off" />
          </label>
          {view.has_token && <button onClick={() => api.putAlerts(cfg, "").then((v) => (setView(v), setMsg({ text: "Token removed.", err: false }))).catch((e) => setMsg({ text: e.message, err: true }))}>Remove token</button>}
          <label className="f">
            <span>Priority</span>
            <select value={cfg.priority} onChange={(e) => set({ priority: Number(e.target.value) })}>
              <option value={1}>Min</option>
              <option value={2}>Low</option>
              <option value={3}>Default</option>
              <option value={4}>High</option>
              <option value={5}>Urgent</option>
            </select>
          </label>
        </div>
      </fieldset>
      <fieldset>
        <legend>What to send</legend>
        <div className="row">
          <label className="check"><input type="checkbox" checked={cfg.needed} onChange={(e) => set({ needed: e.target.checked })} /> Needed cluster spots:</label>
          <select value={cfg.level} disabled={!cfg.needed} onChange={(e) => set({ level: Number(e.target.value) })} aria-label="How needed">
            <option value={0}>Something new only (entity, zone, prefix, Marathon)</option>
            <option value={1}>New ones and new bands</option>
            <option value={2}>New ones, bands and modes</option>
            <option value={3}>All of those, and unconfirmed</option>
          </select>
          <label>
            not again for the same call, band and mode within{" "}
            <select value={cfg.repeat_min} disabled={!cfg.needed} onChange={(e) => set({ repeat_min: Number(e.target.value) })} aria-label="Repeat">
              {[10, 15, 30, 60, 120, 240].map((m) => <option key={m} value={m}>{m} min</option>)}
            </select>
          </label>
        </div>
        <label className="check"><input type="checkbox" checked={cfg.watch} onChange={(e) => set({ watch: e.target.checked })} /> Watch list and DXpedition alerts</label>
        <p className="small muted">Which spots are needed follows Settings, Awards, Spot flags (awards, bands, modes, the DX Marathon). Spots the cluster replays when it connects are not sent.</p>
      </fieldset>
      <div className="row">
        <button className="primary" disabled={!dirty} onClick={save}>Save</button>
        <button disabled={busy || !cfg.topic} onClick={test}>Send test</button>
        {msg && <span className={`small ${msg.err ? "err" : ""}`}>{msg.text}</span>}
      </div>
      <p className="small muted">
        {view.status.at ? `Last alert sent ${new Date(view.status.at * 1000).toLocaleTimeString()}; ${view.status.sent} sent since QRZero started.` : "Nothing sent since QRZero started."}
        {view.status.error ? <span className="err"> Last error: {view.status.error}</span> : null}
      </p>
    </div>
  );
}
