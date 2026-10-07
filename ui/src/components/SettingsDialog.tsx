import { useEffect, useState } from "react";
import { api } from "../api";
import type { Fields, Location, Log, Settings, StationCallsign } from "../types";
import Modal from "./Modal";

interface Props {
  logId: number;
  logs: Log[];
  callsigns: StationCallsign[];
  locations: Location[];
  onChanged: () => void;
  onSwitchLog: (id: number) => void;
  onClose: () => void;
}

type Tab = "station" | "locations" | "logs" | "lookup";

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = useState<Tab>("station");
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
        {(["station", "locations", "logs", "lookup"] as Tab[]).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
            {{ station: "Station callsigns", locations: "Locations", logs: "Logs", lookup: "Callsign lookup" }[t]}
          </button>
        ))}
      </nav>
      {error && <p className="err">{error}</p>}
      {tab === "station" && <StationTab {...props} guard={guard} />}
      {tab === "locations" && <LocationsTab {...props} guard={guard} />}
      {tab === "logs" && <LogsTab {...props} guard={guard} />}
      {tab === "lookup" && <LookupTab />}
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
