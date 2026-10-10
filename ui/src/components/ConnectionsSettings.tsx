import { Fragment, useEffect, useState } from "react";
import { api } from "../api";
import SaveBar from "./SaveBar";
import type { RunStatus, StartupApp, UdpConnection, UdpEvent, UdpFormat } from "../types";

const EVENTS: [UdpEvent, string][] = [
  ["qso_logged", "QSO logged"],
  ["radio", "Radio frequency / mode change"],
  ["rotator", "Rotator turn request"],
  ["lookup", "Call entered"],
  ["relay_wsjtx", "Relay WSJT-X / JTDX packets"],
  ["relay_n1mm", "Relay N1MM packets"],
];

const FORMATS: [UdpFormat, string][] = [
  ["n1mm_radio", "N1MM RadioInfo XML"],
  ["n1mm_contact", "N1MM contactinfo XML"],
  ["adif", "ADIF record"],
  ["json", "JSON"],
  ["pst", "PstRotatorAz azimuth"],
  ["template", "Custom text"],
];

/** The format that suits an event best, picked when the event changes. */
const SUGGESTED: Record<UdpEvent, UdpFormat> = {
  qso_logged: "adif",
  radio: "n1mm_radio",
  rotator: "pst",
  lookup: "json",
  relay_wsjtx: "json",
  relay_n1mm: "json",
};

const isRelay = (e: UdpEvent) => e === "relay_wsjtx" || e === "relay_n1mm";

const blank = (patch: Partial<UdpConnection>): UdpConnection => ({
  id: 0, name: "", enabled: true, host: "127.0.0.1", port: 12060, event: "qso_logged", format: "adif", template: "", radio: "", ...patch,
});

const PRESETS: [string, Partial<UdpConnection>][] = [
  ["Antenna switch / band decoder", { name: "Antenna switch", event: "radio", format: "n1mm_radio", port: 12060 }],
  ["Rotator program", { name: "Rotator", event: "rotator", format: "pst", port: 12000 }],
  ["Relay WSJT-X", { name: "WSJT-X relay", event: "relay_wsjtx", port: 2238 }],
  ["Logged QSO as ADIF", { name: "Logged QSOs", event: "qso_logged", format: "adif", port: 2333 }],
];

type Msg = { text: string; ok: boolean } | null;

export function UdpConnectionsTab({ onClose }: { onClose: () => void }) {
  const [list, setList] = useState<UdpConnection[] | null>(null);
  const [status, setStatus] = useState<Record<string, RunStatus>>({});
  const [msg, setMsg] = useState<Msg>(null);
  const [sent, setSent] = useState("");

  useEffect(() => {
    api.udpConnections().then((r) => { setList(r.connections); setStatus(r.status); }).catch((e) => setMsg({ text: e.message, ok: false }));
  }, []);
  if (!list) return <p className="muted">Loading…</p>;

  const nextId = () => list.reduce((m, c) => Math.max(m, c.id), 0) + 1;
  const add = (patch: Partial<UdpConnection>) => setList([...list, blank({ ...patch, id: nextId() })]);
  const setRow = (i: number, patch: Partial<UdpConnection>) => setList(list.map((c, j) => (j === i ? { ...c, ...patch } : c)));
  const save = async () => {
    try {
      const r = await api.saveUdpConnections(list);
      setList(r.connections);
      setStatus(r.status);
      setMsg({ text: "Saved.", ok: true });
      return true;
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
      return false;
    }
  };
  const test = async (c: UdpConnection) => {
    setSent("");
    try {
      const r = await api.testUdpConnection(c);
      setMsg({ text: `Test sent to ${c.host}:${c.port}${c.name ? ` (${c.name})` : ""}. What was sent is shown above.`, ok: true });
      setSent(r.sent);
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  return (
    <div>
      <p className="muted">
        Send UDP messages to programs QRZero doesn't talk to directly: antenna switches, band decoders, rotator
        programs, your own scripts. Each connection sends when something happens, in a format the other program
        reads. Relay connections pass on everything WSJT-X / JTDX or N1MM send to QRZero, unchanged, so other
        programs can share the stream.
      </p>
      <table className="list nodes udp-conns">
        <thead>
          <tr><th>On</th><th>Name</th><th>Sends when</th><th>Format</th><th>Host</th><th>Port</th><th>Status</th><th /></tr>
        </thead>
        <tbody>
          {list.map((c, i) => {
            const st = status[String(c.id)];
            const extra = (c.format === "template" && !isRelay(c.event)) || c.event === "radio";
            return (
              <Fragment key={c.id || `new${i}`}>
                <tr>
                  <td><input type="checkbox" checked={c.enabled} onChange={(e) => setRow(i, { enabled: e.target.checked })} aria-label="On" /></td>
                  <td><input value={c.name} onChange={(e) => setRow(i, { name: e.target.value })} aria-label="Name" /></td>
                  <td>
                    <select value={c.event} aria-label="Sends when" onChange={(e) => { const ev = e.target.value as UdpEvent; setRow(i, { event: ev, format: SUGGESTED[ev] }); }}>
                      {EVENTS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                    </select>
                  </td>
                  <td>
                    {isRelay(c.event) ? (
                      <span className="muted small">Packets as received</span>
                    ) : (
                      <select value={c.format} aria-label="Format" onChange={(e) => setRow(i, { format: e.target.value as UdpFormat })}>
                        {FORMATS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                      </select>
                    )}
                  </td>
                  <td><input value={c.host} onChange={(e) => setRow(i, { host: e.target.value })} aria-label="Host" /></td>
                  <td><input className="w-port" value={c.port || ""} inputMode="numeric" onChange={(e) => setRow(i, { port: Number(e.target.value) || 0 })} aria-label="Port" /></td>
                  <td className={`small ${st ? (st.ok ? "" : "err") : "muted"}`}>{st ? st.text : "—"}</td>
                  <td className="nowrap">
                    <button className="tiny" onClick={() => test(c)} title="Send a made-up example now">Send test</button>
                    <button className="tiny danger" onClick={() => setList(list.filter((_, j) => j !== i))}>Remove</button>
                  </td>
                </tr>
                {extra && (
                  <tr className="sub">
                    <td />
                    <td colSpan={7}>
                      <div className="row bottom">
                        {c.event === "radio" && (
                          <label className="f w-l">
                            <span>Only this radio (name as in the QSO panel)</span>
                            <input value={c.radio} placeholder="all radios" onChange={(e) => setRow(i, { radio: e.target.value })} />
                          </label>
                        )}
                        {c.format === "template" && !isRelay(c.event) && (
                          <label className="f w-xl">
                            <span>Message ({"{CALL} {FREQ_HZ} {FREQ_KHZ} {FREQ_MHZ} {BAND} {MODE} {AZ} {GRID} {RADIO}"}, any ADIF field; \r \n for line ends)</span>
                            <input className="mono" value={c.template} placeholder="FA{FREQ_HZ};" onChange={(e) => setRow(i, { template: e.target.value })} />
                          </label>
                        )}
                      </div>
                    </td>
                  </tr>
                )}
              </Fragment>
            );
          })}
          {list.length === 0 && <tr><td colSpan={8} className="muted">No connections yet.</td></tr>}
        </tbody>
      </table>
      <div className="row">
        <button onClick={() => add({})}>Add a connection</button>
        <span className="muted small">or add</span>
        {PRESETS.map(([label, p]) => <button key={label} className="tiny" onClick={() => add(p)}>{label}</button>)}
      </div>
      <p className="small muted">
        Radio changes are sent only when the frequency, mode or TX changes, at most four times a second per radio.
        For WSJT-X relays, also see "Pass on to" under Radios and programs; both work.
      </p>
      {sent && <pre className="udp-sent">{sent}</pre>}
      <SaveBar msg={msg} onSave={save} onClose={onClose} />
    </div>
  );
}

export function StartupAppsTab({ onClose }: { onClose: () => void }) {
  const [list, setList] = useState<StartupApp[] | null>(null);
  const [status, setStatus] = useState<Record<string, RunStatus>>({});
  const [msg, setMsg] = useState<Msg>(null);

  useEffect(() => {
    api.startupApps().then((r) => { setList(r.apps); setStatus(r.status); }).catch((e) => setMsg({ text: e.message, ok: false }));
  }, []);
  if (!list) return <p className="muted">Loading…</p>;

  const setRow = (i: number, patch: Partial<StartupApp>) => setList(list.map((a, j) => (j === i ? { ...a, ...patch } : a)));
  const add = () => setList([...list, { id: list.reduce((m, a) => Math.max(m, a.id), 0) + 1, enabled: true, path: "", args: "", skip_if_running: true }]);
  const save = async () => {
    try {
      const r = await api.saveStartupApps(list);
      setList(r.apps);
      setStatus(r.status);
      setMsg({ text: "Saved.", ok: true });
      return true;
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
      return false;
    }
  };
  const launch = async (a: StartupApp) => {
    try {
      const st = await api.launchStartupApp(a);
      setStatus({ ...status, [String(a.id)]: st });
      setMsg(null);
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  return (
    <div>
      <p className="muted">
        Programs QRZero starts when it starts: PstRotatorAz, a rig control server, a band decoder program, anything
        else you always run with it. Give the full path of the program.
      </p>
      <table className="list nodes startup-apps">
        <thead>
          <tr><th>On</th><th>Program</th><th>Arguments</th><th title="Windows only: checks the running programs by file name">Not if running</th><th>Last start</th><th /></tr>
        </thead>
        <tbody>
          {list.map((a, i) => {
            const st = status[String(a.id)];
            return (
              <tr key={a.id || i}>
                <td><input type="checkbox" checked={a.enabled} onChange={(e) => setRow(i, { enabled: e.target.checked })} aria-label="On" /></td>
                <td><input className="mono" value={a.path} placeholder="C:\Program Files (x86)\PstRotatorAz\PstRotatorAz.exe" onChange={(e) => setRow(i, { path: e.target.value })} aria-label="Program" /></td>
                <td><input className="mono" value={a.args} onChange={(e) => setRow(i, { args: e.target.value })} aria-label="Arguments" /></td>
                <td><input type="checkbox" checked={a.skip_if_running} onChange={(e) => setRow(i, { skip_if_running: e.target.checked })} aria-label="Not if already running" /></td>
                <td className={`small ${st ? (st.ok ? "" : "err") : "muted"}`}>{st ? st.text : "—"}</td>
                <td className="nowrap">
                  <button className="tiny" disabled={!a.path.trim()} onClick={() => launch(a)}>Launch now</button>
                  <button className="tiny danger" onClick={() => setList(list.filter((_, j) => j !== i))}>Remove</button>
                </td>
              </tr>
            );
          })}
          {list.length === 0 && <tr><td colSpan={6} className="muted">No programs yet.</td></tr>}
        </tbody>
      </table>
      <div className="row"><button onClick={add}>Add a program</button></div>
      <p className="small muted">
        Programs start in the background, in their own folder, without a console window. "Not if running" leaves a
        program alone when one with the same file name is already running (Windows only). A program that fails to
        start shows why under Last start.
      </p>
      <SaveBar msg={msg} onSave={save} onClose={onClose} />
    </div>
  );
}
