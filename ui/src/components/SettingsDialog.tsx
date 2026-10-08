import { useEffect, useState } from "react";
import { api } from "../api";
import type { ClusterConfig, ClusterNode, CtyStatus, Equipment, Fields, IntegrationStatus, Integrations, Location, Log, Settings, StationCallsign } from "../types";
import type { EntryLayout } from "../fields";
import Modal from "./Modal";
import EquipmentTree from "./EquipmentTree";
import EntryFieldsEditor from "./EntryFieldsEditor";
import { StartupAppsTab, UdpConnectionsTab } from "./ConnectionsSettings";
import BackupsTab from "./BackupsTab";

export interface GeneralPrefs { units: "km" | "mi" }

interface Props {
  logId: number;
  logs: Log[];
  callsigns: StationCallsign[];
  locations: Location[];
  onChanged: () => void;
  onSwitchLog: (id: number) => void;
  onClose: () => void;
  equipment: Equipment[];
  layout: EntryLayout;
  onLayout: (l: EntryLayout) => void;
  general: GeneralPrefs;
  onGeneral: (g: GeneralPrefs) => void;
  onWizard: () => void;
  initialTab?: string;
}

type Tab = "station" | "locations" | "equipment" | "fields" | "radios" | "udp" | "startup" | "cluster" | "logs" | "lookup" | "backups" | "general";

const TAB_NAMES: Record<Tab, string> = {
  station: "Callsigns",
  locations: "Locations",
  equipment: "Equipment",
  fields: "Entry fields",
  radios: "Radios and programs",
  udp: "UDP connections",
  startup: "Startup programs",
  cluster: "DX cluster",
  logs: "Logs",
  lookup: "Callsign lookup",
  backups: "Backups",
  general: "General",
};

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = useState<Tab>(props.initialTab && props.initialTab in TAB_NAMES ? (props.initialTab as Tab) : "station");
  const [error, setError] = useState("");
  const guard = (fn: () => Promise<unknown>) => async () => {
    setError("");
    try {
      await fn();
      props.onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <Modal title="Settings" onClose={props.onClose} wide>
      <nav className="tabs">
        {(Object.keys(TAB_NAMES) as Tab[]).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>{TAB_NAMES[t]}</button>
        ))}
      </nav>
      {error && <p className="err">{error}</p>}
      {tab === "station" && <StationTab {...props} guard={guard} />}
      {tab === "locations" && <LocationsTab {...props} guard={guard} />}
      {tab === "logs" && <LogsTab {...props} guard={guard} />}
      {tab === "equipment" && (
        <>
          <p className="muted">The radios, antennas, amplifiers and rotators at each location. Pick which ones you're using from the QSO panel; they are saved with each QSO as MY_RIG and MY_ANTENNA, and the power fills TX_PWR. Give antennas their bands and the QSO panel picks the right one for the band.</p>
          <EquipmentTree locations={props.locations} equipment={props.equipment} onChanged={props.onChanged} />
        </>
      )}
      {tab === "fields" && <EntryFieldsEditor layout={props.layout} onChange={props.onLayout} />}
      {tab === "lookup" && <LookupTab />}
      {tab === "radios" && <RadiosTab />}
      {tab === "udp" && <UdpConnectionsTab />}
      {tab === "startup" && <StartupAppsTab />}
      {tab === "cluster" && <ClusterTab />}
      {tab === "backups" && <BackupsTab />}
      {tab === "general" && (
        <div>
          <label className="block">
            <span>Distances</span>
            <select value={props.general.units} onChange={(e) => props.onGeneral({ ...props.general, units: e.target.value as "km" | "mi" })}>
              <option value="km">Kilometres</option>
              <option value="mi">Miles</option>
            </select>
          </label>
          <p className="muted">The setup wizard walks through callsigns, your home location, equipment, entry fields and importing an old log. Running it again doesn't remove anything.</p>
          <button onClick={props.onWizard}>Run the setup wizard</button>
        </div>
      )}
    </Modal>
  );
}

type Guard = (fn: () => Promise<unknown>) => () => Promise<void>;

function StationTab({ logId, callsigns, guard }: Props & { guard: Guard }) {
  const [call, setCall] = useState("");
  return (
    <div>
      <p className="muted">Callsigns you operate under in this log. Add your old calls too, so imported QSOs are labelled correctly.</p>
      <table className="list">
        <tbody>
          {callsigns.map((c) => (
            <tr key={c.id}>
              <td className="call">{c.callsign}</td>
              <td>{c.is_default ? <span className="flag info">default</span> : <button onClick={guard(() => api.defaultCallsign(c.id))}>Make default</button>}</td>
              <td><button className="danger" onClick={guard(() => api.deleteCallsign(c.id))}>Remove</button></td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row">
        <label className="f"><span>Add callsign</span><input value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} /></label>
        <div className="actions"><button disabled={!call.trim()} onClick={guard(async () => { await api.addCallsign(logId, call); setCall(""); })}>Add</button></div>
      </div>
    </div>
  );
}

const LOCATION_FIELDS: [string, string][] = [
  ["MY_GRIDSQUARE", "Grid"], ["MY_CITY", "City"], ["MY_STATE", "State"], ["MY_CNTY", "County"],
  ["MY_COUNTRY", "Country"], ["MY_DXCC", "DXCC"], ["MY_CQ_ZONE", "CQ zone"], ["MY_ITU_ZONE", "ITU zone"],
  ["MY_IOTA", "IOTA"], ["MY_SOTA_REF", "SOTA ref"], ["MY_POTA_REF", "POTA ref"], ["MY_WWFF_REF", "WWFF ref"],
  ["MY_RIG", "Rig"], ["MY_ANTENNA", "Antenna"],
];

function LocationsTab({ logId, locations, guard }: Props & { guard: Guard }) {
  const [editing, setEditing] = useState<{ id: number | null; name: string; fields: Fields } | null>(null);

  if (editing) {
    const save = guard(async () => {
      if (editing.id === null) await api.createLocation(logId, editing.name, editing.fields);
      else await api.updateLocation(editing.id, editing.name, editing.fields);
      setEditing(null);
    });
    return (
      <div>
        <div className="row">
          <label className="f wide"><span>Name</span><input value={editing.name} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus /></label>
        </div>
        <div className="field-grid">
          {LOCATION_FIELDS.map(([k, label]) => (
            <label key={k} className="f">
              <span>{label}</span>
              <input value={editing.fields[k] ?? ""} onChange={(e) => setEditing({ ...editing, fields: { ...editing.fields, [k]: e.target.value } })} />
            </label>
          ))}
        </div>
        <div className="buttons">
          <button onClick={() => setEditing(null)}>Cancel</button>
          <button className="primary" disabled={!editing.name.trim()} onClick={save}>Save location</button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <p className="muted">Where you operate from. The location's details (grid, POTA reference, …) are added to every QSO you log there.</p>
      <table className="list">
        <tbody>
          {locations.map((l) => (
            <tr key={l.id}>
              <td><strong>{l.name}</strong></td>
              <td className="muted">{[l.fields.MY_GRIDSQUARE, l.fields.MY_POTA_REF, l.fields.MY_SOTA_REF].filter(Boolean).join(" · ")}</td>
              <td>{l.is_default ? <span className="flag info">default</span> : <button onClick={guard(() => api.defaultLocation(l.id))}>Make default</button>}</td>
              <td><button onClick={() => setEditing({ id: l.id, name: l.name, fields: { ...l.fields } })}>Edit</button></td>
              <td><button className="danger" onClick={guard(async () => { if (confirm(`Remove location ${l.name}? QSOs keep their details.`)) await api.deleteLocation(l.id); })}>Remove</button></td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="buttons"><button onClick={() => setEditing({ id: null, name: "", fields: {} })}>Add location</button></div>
    </div>
  );
}

function LogsTab({ logs, logId, onSwitchLog, guard }: Props & { guard: Guard }) {
  const [name, setName] = useState("");
  const [renames, setRenames] = useState<Record<number, string>>({});
  return (
    <div>
      <p className="muted">Each log is separate, for example one per operator. Switch logs from the top bar.</p>
      <table className="list">
        <tbody>
          {logs.map((l) => (
            <tr key={l.id}>
              <td><input value={renames[l.id] ?? l.name} onChange={(e) => setRenames({ ...renames, [l.id]: e.target.value })} /></td>
              <td className="muted">{l.qso_count.toLocaleString()} QSOs</td>
              <td>{(renames[l.id] ?? l.name) !== l.name && <button onClick={guard(() => api.renameLog(l.id, renames[l.id]))}>Rename</button>}</td>
              <td>{l.id === logId ? <span className="flag info">open</span> : <button onClick={() => onSwitchLog(l.id)}>Open</button>}</td>
              <td>
                {logs.length > 1 && (
                  <button className="danger" onClick={guard(async () => {
                    const typed = prompt(`Deleting "${l.name}" removes all ${l.qso_count} QSOs in it. Type the log name to confirm.`);
                    if (typed === l.name) {
                      await api.deleteLog(l.id);
                      if (l.id === logId) onSwitchLog(logs.find((x) => x.id !== l.id)!.id);
                    }
                  })}>Delete</button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row">
        <label className="f"><span>New log</span><input value={name} onChange={(e) => setName(e.target.value)} /></label>
        <div className="actions"><button disabled={!name.trim()} onClick={guard(async () => { const l = await api.createLog(name); setName(""); onSwitchLog(l.id); })}>Create</button></div>
      </div>
    </div>
  );
}

function LookupTab() {
  const [s, setS] = useState<Settings | null>(null);
  const [password, setPassword] = useState("");
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    api.settings().then(setS).catch((e) => setMsg({ text: e.message, ok: false }));
  }, []);
  if (!s) return <p className="muted">Loading…</p>;

  const save = async () => {
    try {
      const body: { qrz_enabled: boolean; qrz_username: string; qrz_password?: string } = { qrz_enabled: s.qrz_enabled, qrz_username: s.qrz_username };
      if (password) body.qrz_password = password;
      setS(await api.saveSettings(body));
      setPassword("");
      setMsg({ text: "Saved.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };
  const test = async () => {
    setMsg({ text: "Checking…", ok: true });
    try {
      await save();
      await api.testQrz();
      setMsg({ text: "QRZ login works.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  return (
    <div>
      <p className="muted">QRZ.com XML lookups fill in name, QTH, grid and more when you leave the call field. They need a QRZ XML subscription. Results are cached for 30 days.</p>
      <label className="check"><input type="checkbox" checked={s.qrz_enabled} onChange={(e) => setS({ ...s, qrz_enabled: e.target.checked })} /> Look up callsigns on QRZ.com</label>
      <div className="row">
        <label className="f"><span>QRZ username</span><input value={s.qrz_username} onChange={(e) => setS({ ...s, qrz_username: e.target.value })} /></label>
        <label className="f">
          <span>QRZ password {s.qrz_password_set && "(saved)"}</span>
          <input type="password" value={password} placeholder={s.qrz_password_set ? "unchanged" : ""} onChange={(e) => setPassword(e.target.value)} />
        </label>
      </div>
      <p className="small muted">On Windows the password is kept in Windows Credential Manager, not in the log file.</p>
      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
      <div className="buttons">
        <button onClick={test} disabled={!s.qrz_username}>Save and test login</button>
        <button className="primary" onClick={save}>Save</button>
      </div>
    </div>
  );
}

function RadiosTab() {
  const [cfg, setCfg] = useState<Integrations | null>(null);
  const [status, setStatus] = useState<IntegrationStatus | null>(null);
  const [cty, setCty] = useState<CtyStatus | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    api.integrations().then((r) => { setCfg(r.config); setStatus(r.status); }).catch((e) => setMsg({ text: e.message, ok: false }));
    api.cty().then(setCty).catch(() => {});
  }, []);
  if (!cfg) return <p className="muted">Loading…</p>;

  const set = (patch: Partial<Integrations>) => setCfg({ ...cfg, ...patch });
  const save = async () => {
    try {
      const r = await api.saveIntegrations(cfg);
      setCfg(r.config);
      setStatus(r.status);
      setMsg({ text: "Saved.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };
  const ctyRun = async (fn: () => Promise<CtyStatus>) => {
    setMsg({ text: "Loading the country file…", ok: true });
    try {
      setCty(await fn());
      setMsg({ text: "Country file loaded.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };
  const check = (k: keyof Integrations, label: string) => (
    <label className="check"><input type="checkbox" checked={cfg[k] as boolean} onChange={(e) => set({ [k]: e.target.checked })} /> {label}</label>
  );
  const text = (k: keyof Integrations, label: string, cls = "w-m", placeholder?: string) => (
    <label className={`f ${cls}`}><span>{label}</span><input value={cfg[k] as string} placeholder={placeholder} onChange={(e) => set({ [k]: e.target.value })} /></label>
  );

  return (
    <div className="radios-tab">
      <p className="muted">
        Rigs are set up under <b>Equipment</b>: edit a radio and choose how QRZero talks to it (Hamlib, TCI, CAT or CI-V).
        Here are the programs QRZero listens to.
      </p>

      <fieldset>
        <legend>WSJT-X and JTDX</legend>
        {check("wsjtx_enabled", "Listen to WSJT-X / JTDX")}
        <div className="row">
          {text("wsjtx_listen", "Listen on (one per port, comma between)", "w-xl", "127.0.0.1:2237, 127.0.0.1:2238")}
          {text("wsjtx_multicast", "Multicast group (optional)", "w-l", "224.0.0.1")}
          <label className="f w-xl">
            <span>Pass on to (GridTracker, JTAlert …)</span>
            <input value={cfg.wsjtx_forward.join(", ")} placeholder="127.0.0.1:2238" onChange={(e) => set({ wsjtx_forward: e.target.value.split(/[,\s]+/).filter(Boolean) })} />
          </label>
        </div>
        {check("wsjtx_auto_log", "Log QSOs that WSJT-X / JTDX log")}
        <p className="small muted">
          In WSJT-X: File, Settings, Reporting. Set UDP Server to the address above (127.0.0.1, port 2237) and tick
          "Accept UDP requests" so double-clicking a decode here can call the station. Run several copies with
          different names (wsjtx --rig-name=…) and they all show in the FTx monitor.
        </p>
        {status?.wsjtx && <p className="small">Status: {status.wsjtx}</p>}
      </fieldset>

      <fieldset>
        <legend>N1MM Logger+</legend>
        {check("n1mm_enabled", "Listen to N1MM Logger+")}
        <div className="row">{text("n1mm_listen", "Listen on", "w-m", "127.0.0.1:12060")}</div>
        {check("n1mm_auto_log", "Copy QSOs logged in N1MM into this log (edits and deletes follow)")}
        <p className="small muted">In N1MM: Config, Configure Ports…, Broadcast Data. Tick Contacts and Radio and set the address to 127.0.0.1:12060.</p>
        {status?.n1mm && <p className="small">Status: {status.n1mm}</p>}
      </fieldset>

      <fieldset>
        <legend>QSOs logged by other programs</legend>
        {check("auto_log_lookup", "Look them up on QRZ and fill in name, QTH, state, grid and the rest when left blank")}
        <p className="small muted">Needs your QRZ login (Callsign lookup tab). What WSJT-X, JTDX or N1MM sent is never overwritten.</p>
      </fieldset>

      <fieldset>
        <legend>PstRotatorAz</legend>
        {check("rotator_enabled", "Turn the rotator with PstRotatorAz")}
        <div className="row">{text("rotator_addr", "PstRotatorAz UDP address", "w-m", "127.0.0.1:12000")}</div>
        <p className="small muted">In PstRotatorAz: Setup, UDP Control, port 12000. The map gets Turn SP / LP buttons.</p>
        {status?.rotator && <p className="small">Status: {status.rotator}</p>}
      </fieldset>

      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
      <div className="buttons"><button className="primary" onClick={save}>Save</button></div>

      <fieldset>
        <legend>Country file</legend>
        <p className="small muted">
          Works out the DXCC entity of a call for the FTx monitor, spots and the map, from AD1C's country files
          (country-files.com). QRZero downloads it every two weeks.
        </p>
        <p>
          {cty && cty.entities > 0
            ? `${cty.entities} entities loaded from ${cty.file}${cty.age_days !== null ? `, ${cty.age_days} day${cty.age_days === 1 ? "" : "s"} old` : ""}.`
            : "No country file loaded yet."}
        </p>
        <div className="row">
          <button onClick={() => ctyRun(api.updateCty)}>Download now</button>
          <label className="button-like">
            Load cty.csv or cty.dat…
            <input type="file" accept=".csv,.dat,text/plain" hidden onChange={(e) => e.target.files?.[0] && ctyRun(() => api.installCty(e.target.files![0]))} />
          </label>
        </div>
      </fieldset>
    </div>
  );
}

// Well-known nodes offered as a starting point.
const KNOWN_NODES: ClusterNode[] = [
  { name: "VE7CC", host: "dxc.ve7cc.net", port: 23, login: "", password: "", commands: [] },
  { name: "NC7J", host: "dxc.nc7j.com", port: 7373, login: "", password: "", commands: [] },
  { name: "RBN (CW, RTTY)", host: "telnet.reversebeacon.net", port: 7000, login: "", password: "", commands: [] },
  { name: "RBN (FT8)", host: "telnet.reversebeacon.net", port: 7001, login: "", password: "", commands: [] },
];

function ClusterTab() {
  const [cfg, setCfg] = useState<ClusterConfig | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  useEffect(() => {
    api.cluster().then((c) => setCfg(c.config)).catch((e) => setMsg({ text: e.message, ok: false }));
  }, []);
  if (!cfg) return <p className="muted">Loading…</p>;

  const setNode = (i: number, patch: Partial<ClusterNode>) => setCfg({ ...cfg, nodes: cfg.nodes.map((n, j) => (j === i ? { ...n, ...patch } : n)) });
  const move = (i: number, d: number) => {
    const nodes = [...cfg.nodes];
    const [n] = nodes.splice(i, 1);
    nodes.splice(i + d, 0, n);
    setCfg({ ...cfg, nodes });
  };
  const save = async () => {
    try {
      const r = await api.saveCluster({ ...cfg, nodes: cfg.nodes.filter((n) => n.host.trim()) });
      setCfg(r.config);
      setMsg({ text: "Saved.", ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  return (
    <div>
      <p className="muted">
        QRZero connects to the first node in the list and moves down the list if it can't get through. It logs in with
        the station callsign you've picked in the top bar unless you give a login.
      </p>
      <table className="list nodes">
        <thead>
          <tr><th>Name</th><th>Host</th><th>Port</th><th>Login</th><th>Password</th><th>Commands after login</th><th /></tr>
        </thead>
        <tbody>
          {cfg.nodes.map((n, i) => (
            <tr key={i}>
              <td><input value={n.name} onChange={(e) => setNode(i, { name: e.target.value })} aria-label="Name" /></td>
              <td><input value={n.host} onChange={(e) => setNode(i, { host: e.target.value })} aria-label="Host" /></td>
              <td><input className="w-port" value={n.port || ""} inputMode="numeric" onChange={(e) => setNode(i, { port: Number(e.target.value) || 0 })} aria-label="Port" /></td>
              <td><input value={n.login} placeholder="your call" onChange={(e) => setNode(i, { login: e.target.value.toUpperCase() })} aria-label="Login" /></td>
              <td><input type="password" value={n.password} onChange={(e) => setNode(i, { password: e.target.value })} aria-label="Password" /></td>
              <td><input value={n.commands.join("; ")} placeholder="e.g. set/skimmer; set/ft8" onChange={(e) => setNode(i, { commands: e.target.value.split(";").map((c) => c.trim()).filter(Boolean) })} aria-label="Commands" /></td>
              <td className="nowrap">
                <button className="tiny" disabled={i === 0} onClick={() => move(i, -1)} title="Try earlier">↑</button>
                <button className="tiny" disabled={i === cfg.nodes.length - 1} onClick={() => move(i, 1)} title="Try later">↓</button>
                <button className="tiny danger" onClick={() => setCfg({ ...cfg, nodes: cfg.nodes.filter((_, j) => j !== i) })}>Remove</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row">
        <button onClick={() => setCfg({ ...cfg, nodes: [...cfg.nodes, { name: "", host: "", port: 23, login: "", password: "", commands: [] }] })}>Add a node</button>
        <span className="muted small">or add</span>
        {KNOWN_NODES.filter((k) => !cfg.nodes.some((n) => n.host === k.host && n.port === k.port)).map((k) => (
          <button key={k.name} className="tiny" onClick={() => setCfg({ ...cfg, nodes: [...cfg.nodes, k] })}>{k.name}</button>
        ))}
      </div>
      <label className="check"><input type="checkbox" checked={cfg.auto_connect} onChange={(e) => setCfg({ ...cfg, auto_connect: e.target.checked })} /> Connect when QRZero starts</label>
      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
      <div className="buttons"><button className="primary" onClick={save}>Save</button></div>
    </div>
  );
}
