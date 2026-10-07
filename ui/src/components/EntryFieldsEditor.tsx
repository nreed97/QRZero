import { useState } from "react";
import { CUSTOM_PREFIX, ENTRY_FIELDS, PRESETS, customKey, fieldDef, type EntryItem, type EntryLayout, type Width } from "../fields";

const WIDTHS: [Width, string][] = [["s", "Narrow"], ["m", "Medium"], ["l", "Wide"], ["xl", "Widest"]];

export default function EntryFieldsEditor({ layout, onChange }: { layout: EntryLayout; onChange: (l: EntryLayout) => void }) {
  const [adding, setAdding] = useState<Record<number, string>>({});
  const used = new Set(layout.rows.flat().map((i) => i.key));
  const rows = layout.rows.length ? layout.rows : [[]];

  const update = (r: number, i: number, patch: Partial<EntryItem>) =>
    onChange({ rows: rows.map((row, ri) => (ri !== r ? row : row.map((it, ii) => (ii === i ? { ...it, ...patch } : it)))) });

  const remove = (r: number, i: number) => onChange({ rows: rows.map((row, ri) => (ri === r ? row.filter((_, ii) => ii !== i) : row)) });

  const move = (r: number, i: number, dr: number, di: number) => {
    const next = rows.map((row) => [...row]);
    const [item] = next[r].splice(i, 1);
    const tr = Math.max(0, Math.min(next.length - 1, r + dr));
    const ti = dr ? next[tr].length : Math.max(0, Math.min(next[tr].length, i + di));
    next[tr].splice(ti, 0, item);
    onChange({ rows: next });
  };

  const add = (r: number) => {
    let key = adding[r];
    if (!key) return;
    if (key === "__custom") {
      const name = prompt("Name for your own field (for example: Club number):");
      if (!name?.trim()) return;
      key = customKey(name);
      if (used.has(key)) return;
      onChange({ rows: rows.map((row, ri) => (ri === r ? [...row, { key, label: name.trim() }] : row)) });
    } else {
      onChange({ rows: rows.map((row, ri) => (ri === r ? [...row, { key }] : row)) });
    }
    setAdding({ ...adding, [r]: "" });
  };

  return (
    <div className="fields-editor">
      <p className="muted">
        Call, reports, frequency, band and mode are always on the first line. Choose what goes on the lines below.
        Fields marked <em>keep</em> hold their value for the next QSO; a default fills a field for every new QSO.
      </p>
      <div className="row">
        <span className="muted">Start from:</span>
        {PRESETS.map((p) => (
          <button key={p.id} title={p.description} onClick={() => confirm(`Replace your field layout with "${p.name}"?`) && onChange(p.layout)}>{p.name}</button>
        ))}
      </div>
      {rows.map((row, r) => (
        <fieldset key={r}>
          <legend>Line {r + 2}</legend>
          <table className="list fields">
            <tbody>
              {row.map((item, i) => {
                const def = fieldDef(item);
                return (
                  <tr key={item.key}>
                    <td className="mono small">{item.key.startsWith(CUSTOM_PREFIX) ? "your field" : item.key}</td>
                    <td><input value={item.label ?? ""} placeholder={def.label} onChange={(e) => update(r, i, { label: e.target.value || undefined })} aria-label="Label" /></td>
                    <td>
                      <select value={def.width} onChange={(e) => update(r, i, { width: e.target.value as Width })} aria-label="Width">
                        {WIDTHS.map(([w, l]) => <option key={w} value={w}>{l}</option>)}
                      </select>
                    </td>
                    <td><label className="check"><input type="checkbox" checked={!!item.sticky} onChange={(e) => update(r, i, { sticky: e.target.checked || undefined })} /> keep</label></td>
                    <td><input className="w-default" value={item.default ?? ""} placeholder="default" onChange={(e) => update(r, i, { default: e.target.value || undefined })} aria-label="Default" /></td>
                    <td className="nowrap">
                      <button className="tiny" disabled={i === 0} onClick={() => move(r, i, 0, -1)} title="Move left">←</button>
                      <button className="tiny" disabled={i === row.length - 1} onClick={() => move(r, i, 0, 1)} title="Move right">→</button>
                      <button className="tiny" disabled={r === 0} onClick={() => move(r, i, -1, 0)} title="Move to the line above">↑</button>
                      <button className="tiny" disabled={r === rows.length - 1} onClick={() => move(r, i, 1, 0)} title="Move to the line below">↓</button>
                      <button className="tiny danger" onClick={() => remove(r, i)}>Remove</button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="row">
            <select value={adding[r] ?? ""} onChange={(e) => setAdding({ ...adding, [r]: e.target.value })} aria-label={`Add a field to line ${r + 2}`}>
              <option value="">Add a field…</option>
              {ENTRY_FIELDS.filter((f) => !used.has(f.key)).map((f) => <option key={f.key} value={f.key}>{f.label} ({f.key})</option>)}
              <option value="__custom">Your own field…</option>
            </select>
            <button disabled={!adding[r]} onClick={() => add(r)}>Add</button>
            {rows.length > 1 && row.length === 0 && <button onClick={() => onChange({ rows: rows.filter((_, ri) => ri !== r) })}>Remove line</button>}
          </div>
        </fieldset>
      ))}
      {rows.length < 4 && <button onClick={() => onChange({ rows: [...rows, []] })}>Add a line</button>}
    </div>
  );
}
