import { useState } from "react";
import { api } from "../api";
import type { Equipment, EquipmentKind, Fields, Location } from "../types";

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
  antenna: [["MODEL", "Type / model", "e.g. 3-element yagi"], ["BANDS", "Bands"], ["HEIGHT", "Height"], ["NOTES", "Notes"]],
  amplifier: [["MODEL", "Make and model"], ["POWER_W", "Output (W)"], ["BANDS", "Bands"], ["NOTES", "Notes"]],
  rotator: [["MODEL", "Make and model"], ["NOTES", "Notes"]],
  other: [["MODEL", "Description"], ["NOTES", "Notes"]],
};

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
                                <span className="muted">{[e.fields.MODEL, e.fields.POWER_W && `${e.fields.POWER_W} W`, e.fields.BANDS].filter(Boolean).join(" · ")}</span>
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
