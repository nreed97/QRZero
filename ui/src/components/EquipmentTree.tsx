import { useState } from "react";
import { api } from "../api";
import type { Equipment, EquipmentKind, Fields, Location } from "../types";
import { ANTENNA_BANDS, formatBands, parseBands, shortBands } from "../antennas";

export const KINDS: { kind: EquipmentKind; label: string; plural: string }[] = [
  { kind: "rig", label: "Radio", plural: "Radios" },
  { kind: "antenna", label: "Antenna", plural: "Antennas" },
  { kind: "amplifier", label: "Amplifier", plural: "Amplifiers" },
  { kind: "rotator", label: "Rotator", plural: "Rotators" },
  { kind: "other", label: "Other", plural: "Other gear" },
];

// Details offered per kind. All are optional.
const DETAILS: Record<EquipmentKind, [string, string, string?][]> = {
  rig: [["MODEL", "Make and model", "e.g. Elecraft K3"], ["POWER_W", "Power (W)"], ["BANDS", "Bands", "e.g. 160-6m"], ["NOTES", "Notes"]],
  antenna: [["MODEL", "Type / model", "e.g. 3-element yagi"], ["HEIGHT", "Height"], ["NOTES", "Notes"]],
  amplifier: [["MODEL", "Make and model"], ["POWER_W", "Output (W)"], ["BANDS", "Bands"], ["NOTES", "Notes"]],
  rotator: [["MODEL", "Make and model"], ["NOTES", "Notes"]],
  other: [["MODEL", "Description"], ["NOTES", "Notes"]],
};

// How QRZero talks to a radio. Settings are kept in the rig's fields.
const CONTROLS: { id: string; label: string; net?: boolean; port?: number }[] = [
  { id: "", label: "None (log only)" },
  { id: "hamlib", label: "Hamlib rigctld", net: true, port: 4532 },
  { id: "tci", label: "TCI (ExpertSDR, Flex slices, SunSDR)", net: true, port: 40001 },
  { id: "kenwood", label: "Kenwood / Elecraft / Flex CAT" },
  { id: "yaesu", label: "Yaesu CAT (FT-991A, FTDX10/101, FT-710)" },
  { id: "icom", label: "Icom CI-V" },
];

function RigControl({ fields, onChange }: { fields: Fields; onChange: (f: Fields) => void }) {
  const control = CONTROLS.find((c) => c.id === (fields.CONTROL ?? "")) ?? CONTROLS[0];
  const set = (k: string, v: string) => onChange({ ...fields, [k]: v });
  const input = (k: string, label: string, cls: string, placeholder?: string) => (
    <label className={`f ${cls}`}><span>{label}</span><input value={fields[k] ?? ""} placeholder={placeholder} onChange={(e) => set(k, e.target.value)} /></label>
  );
  return (
    <div className="row">
      <label className="f w-l">
        <span>Rig control</span>
        <select value={control.id} onChange={(e) => set("CONTROL", e.target.value)}>
          {CONTROLS.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
        </select>
      </label>
      {control.net && input("HOST", "Host", "w-m", "127.0.0.1")}
      {control.net && input("PORT", "Port", "w-s", String(control.port))}
      {control.id && !control.net && input("SERIAL_PORT", "Serial port", "w-s", "COM3")}
      {control.id && !control.net && input("BAUD", "Baud", "w-s", "38400")}
      {control.id === "icom" && input("CIV_ADDR", "CI-V address", "w-s", "94")}
    </div>
  );
}

// Which bands an antenna covers; picks it automatically when logging on them.
function BandPicker({ fields, onChange }: { fields: Fields; onChange: (f: Fields) => void }) {
  const picked = parseBands(fields.BANDS);
  const offered = [...ANTENNA_BANDS, ...picked.filter((b) => !ANTENNA_BANDS.includes(b))];
  const flip = (b: string, on: boolean) => {
    const next = formatBands(on ? [...picked, b] : picked.filter((x) => x !== b));
    const f = { ...fields };
    if (next) f.BANDS = next;
    else delete f.BANDS;
    onChange(f);
  };
  return (
    <div className="row">
      <div className="f">
        <span>Bands (picked automatically when logging on these)</span>
        <div className="band-picks" data-testid="antenna-bands">
          {offered.map((b) => (
            <label key={b}><input type="checkbox" checked={picked.includes(b)} onChange={(e) => flip(b, e.target.checked)} />{b}</label>
          ))}
        </div>
      </div>
    </div>
  );
}

interface Draft { id: number | null; location_id: number; kind: EquipmentKind; name: string; fields: Fields }

export default function EquipmentTree({ locations, equipment, onChanged }: { locations: Location[]; equipment: Equipment[]; onChanged: () => void }) {
  const [open, setOpen] = useState<Set<string>>(() => new Set(locations.map((l) => `l${l.id}`)));
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState("");

  const toggle = (k: string) => {
    const next = new Set(open);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    setOpen(next);
  };

  const run = async (fn: () => Promise<unknown>) => {
    setError("");
    try {
      await fn();
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const save = () =>
    draft &&
    run(async () => {
      if (draft.id === null) await api.createEquipment(draft.location_id, draft.kind, draft.name, draft.fields);
      else await api.updateEquipment(draft.id, draft);
      setDraft(null);
    });

  if (!locations.length) return <p className="muted">Add a location first; equipment belongs to a location.</p>;

  return (
    <div className="equipment">
      <ul className="tree">
        {locations.map((loc) => {
          const lk = `l${loc.id}`;
          const items = equipment.filter((e) => e.location_id === loc.id);
          return (
            <li key={loc.id}>
              <button className="twisty" onClick={() => toggle(lk)} aria-expanded={open.has(lk)}>{open.has(lk) ? "▾" : "▸"}</button>
              <strong>{loc.name}</strong> <span className="muted">({items.length})</span>
              {open.has(lk) && (
                <ul>
                  {KINDS.map(({ kind, plural, label }) => {
                    const list = items.filter((e) => e.kind === kind);
                    return (
                      <li key={kind}>
                        <span className="kind">{plural}</span>
                        <button className="link" onClick={() => setDraft({ id: null, location_id: loc.id, kind, name: "", fields: {} })}>add {label.toLowerCase()}</button>
                        {list.length > 0 && (
                          <ul>
                            {list.map((e, i) => (
                              <li key={e.id} className="item">
                                <span className="item-name">{e.name}</span>
                                <span className="muted">{[e.fields.MODEL, e.fields.POWER_W && `${e.fields.POWER_W} W`, (e.kind === "antenna" && shortBands(parseBands(e.fields.BANDS))) || e.fields.BANDS, e.fields.CONTROL && `control: ${CONTROLS.find((c) => c.id === e.fields.CONTROL)?.label.split(" (")[0] ?? e.fields.CONTROL}`].filter(Boolean).join(" · ")}</span>
                                <span className="item-actions">
                                  <button className="tiny" disabled={i === 0} onClick={() => run(() => api.moveEquipment(e.id, -1))} title="Move up">↑</button>
                                  <button className="tiny" disabled={i === list.length - 1} onClick={() => run(() => api.moveEquipment(e.id, 1))} title="Move down">↓</button>
                                  <button className="tiny" onClick={() => setDraft({ id: e.id, location_id: e.location_id, kind: e.kind, name: e.name, fields: { ...e.fields } })}>Edit</button>
                                  <button className="tiny danger" onClick={() => confirm(`Remove ${e.name}?`) && run(() => api.deleteEquipment(e.id))}>Remove</button>
                                </span>
                              </li>
                            ))}
                          </ul>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </li>
          );
        })}
      </ul>

      {draft && (
        <fieldset className="equipment-form">
          <legend>{draft.id === null ? "New" : "Edit"} {KINDS.find((k) => k.kind === draft.kind)?.label.toLowerCase()}</legend>
          <div className="row">
            <label className="f w-l"><span>Name (shown when logging)</span><input value={draft.name} autoFocus onChange={(e) => setDraft({ ...draft, name: e.target.value })} /></label>
            <label className="f w-m">
              <span>Location</span>
              <select value={draft.location_id} onChange={(e) => setDraft({ ...draft, location_id: Number(e.target.value) })}>
                {locations.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
              </select>
            </label>
          </div>
          <div className="row">
            {DETAILS[draft.kind].map(([k, label, hint]) => (
              <label key={k} className={`f ${k === "NOTES" ? "w-xl" : "w-m"}`}>
                <span>{label}</span>
                <input value={draft.fields[k] ?? ""} placeholder={hint} onChange={(e) => setDraft({ ...draft, fields: { ...draft.fields, [k]: e.target.value } })} />
              </label>
            ))}
          </div>
          {draft.kind === "antenna" && <BandPicker fields={draft.fields} onChange={(fields) => setDraft({ ...draft, fields })} />}
          {draft.kind === "rig" && <RigControl fields={draft.fields} onChange={(fields) => setDraft({ ...draft, fields })} />}
          <div className="buttons">
            <button onClick={() => setDraft(null)}>Cancel</button>
            <button className="primary" disabled={!draft.name.trim()} onClick={save}>Save</button>
          </div>
        </fieldset>
      )}
      {error && <p className="err">{error}</p>}
    </div>
  );
}
