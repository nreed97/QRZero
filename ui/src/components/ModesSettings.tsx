import { useState } from "react";
import { ADIF_MODES, adifGroup, adifRst } from "../adifModes";
import { DEFAULT_MODES, MODES, setModes, type ModeChoice, type ModeGroup } from "../modes";
import SaveBar, { type SaveMsg } from "./SaveBar";

const GROUP_NAMES: Record<ModeGroup, string> = { cw: "CW", phone: "Phone", digital: "Digital" };

/** Settings, Modes: the modes offered in the QSO panel and editor. Hiding a mode never touches QSOs already logged in it. */
export default function ModesTab({ onClose }: { onClose: () => void }) {
  const [list, setList] = useState<ModeChoice[]>(() => MODES.map((m) => ({ ...m })));
  const [mode, setMode] = useState("");
  const [submode, setSubmode] = useState("");
  const [label, setLabel] = useState("");
  const [msg, setMsg] = useState<SaveMsg>(null);
  const [dirty, setDirty] = useState(false);

  const change = (next: ModeChoice[]) => { setList(next); setDirty(true); setMsg(null); };
  const patch = (i: number, p: Partial<ModeChoice>) => change(list.map((m, n) => (n === i ? { ...m, ...p } : m)));
  const move = (i: number, d: number) => {
    const next = [...list];
    [next[i], next[i + d]] = [next[i + d], next[i]];
    change(next);
  };

  const wanted = label.trim() || submode || mode;
  const clash = list.some((m) => m.label.toLowerCase() === wanted.toLowerCase());
  const add = () => {
    if (!mode || !wanted || clash) return;
    change([...list, { label: wanted, mode, ...(submode ? { submode } : {}), rst: adifRst(mode, submode), group: adifGroup(mode, submode) }]);
    setMode(""); setSubmode(""); setLabel("");
  };

  const save = async (): Promise<boolean> => {
    if (list.length === 0) { setMsg({ text: "Keep at least one mode.", ok: false }); return false; }
    try {
      // Saving exactly the default list stores nothing, so a later default change reaches this user.
      await setModes(JSON.stringify(list) === JSON.stringify(DEFAULT_MODES) ? null : list);
      setDirty(false);
      setMsg({ text: "Saved.", ok: true });
      return true;
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
      return false;
    }
  };

  return (
    <div className="general-tab modes-tab">
      <p className="muted">
        The modes offered in the QSO panel, the QSO editor and the log's mode filter. Taking a mode out only hides it from those lists: QSOs already logged in it, and ADIF imports in it, are kept and still show.
      </p>
      <fieldset>
        <legend>Modes in the list</legend>
        <table className="list">
          <thead>
            <tr><th>Shown as</th><th>ADIF mode</th><th>Submode</th><th>Report</th><th>Counts for awards as</th><th /></tr>
          </thead>
          <tbody>
            {list.map((m, i) => (
              <tr key={m.label}>
                <td>{m.label}</td>
                <td>{m.mode}</td>
                <td>{m.submode ?? ""}</td>
                <td><input className="w-s" value={m.rst} aria-label={`Report for ${m.label}`} onChange={(e) => patch(i, { rst: e.target.value })} /></td>
                <td>
                  <select value={m.group} aria-label={`Award column for ${m.label}`} onChange={(e) => patch(i, { group: e.target.value as ModeGroup })}>
                    {(Object.keys(GROUP_NAMES) as ModeGroup[]).map((g) => <option key={g} value={g}>{GROUP_NAMES[g]}</option>)}
                  </select>
                </td>
                <td className="actions">
                  <button className="tiny" onClick={() => move(i, -1)} disabled={i === 0} title="Move up">Up</button>
                  <button className="tiny" onClick={() => move(i, 1)} disabled={i === list.length - 1} title="Move down">Down</button>
                  <button className="tiny" onClick={() => change(list.filter((_, n) => n !== i))} title="Hide this mode from the lists">Remove</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="small muted">The award column decides whether DXCC, DX Marathon and the other awards count a mode as CW, Phone or Digital.</p>
      </fieldset>
      <fieldset>
        <legend>Add a mode</legend>
        <div className="row">
          <label className="f w-m">
            <span>ADIF mode</span>
            <select value={mode} onChange={(e) => { setMode(e.target.value); setSubmode(""); }}>
              <option value="" />
              {Object.keys(ADIF_MODES).map((m) => <option key={m}>{m}</option>)}
            </select>
          </label>
          <label className="f w-m">
            <span>Submode</span>
            <select value={submode} onChange={(e) => setSubmode(e.target.value)} disabled={!mode || !ADIF_MODES[mode]?.length}>
              <option value="" />
              {(ADIF_MODES[mode] ?? []).map((s) => <option key={s}>{s}</option>)}
            </select>
          </label>
          <label className="f w-m">
            <span>Shown as (optional)</span>
            <input value={label} placeholder={submode || mode} onChange={(e) => setLabel(e.target.value)} />
          </label>
          <button onClick={add} disabled={!mode || clash} title={clash ? "That name is already in the list" : undefined}>Add</button>
        </div>
        <p className="small muted">
          Modes and submodes are the ADIF ones, so what you log is valid ADIF for LoTW, Club Log and QRZ. For example JT65, HELL with FSKHELL (Feld Hell), or TOR with AMTORFEC.
        </p>
      </fieldset>
      <SaveBar msg={msg} onSave={save} onClose={onClose}>
        <button onClick={() => change(DEFAULT_MODES.map((m) => ({ ...m })))} disabled={JSON.stringify(list) === JSON.stringify(DEFAULT_MODES)}>Use the short default list</button>
        {dirty && <span className="muted">Not saved</span>}
      </SaveBar>
    </div>
  );
}
