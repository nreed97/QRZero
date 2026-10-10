import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { FIXED, SHORTCUTS, bindingFor, bindingOf, capture, problemWith, setOverrides, show, useOverrides } from "../shortcuts";

/** Settings > Keyboard: change the shortcut keys, and print the list. */
export default function KeyboardTab() {
  const overrides = useOverrides();
  const [picking, setPicking] = useState<string | null>(null);
  const [msg, setMsg] = useState("");
  const [printing, setPrinting] = useState(false);

  // While a row waits for a key, take every key press so the window doesn't act on it.
  useEffect(() => {
    if (!picking) return;
    capture.active = true;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") return setPicking(null);
      const b = bindingOf(e);
      if (!b) return;
      const why = problemWith(b, overrides, picking);
      if (why) return setMsg(why);
      setOverrides({ ...overrides, [picking]: b });
      setMsg("");
      setPicking(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      capture.active = false;
      window.removeEventListener("keydown", onKey, true);
    };
  }, [picking, overrides]);

  useEffect(() => {
    if (!printing) return;
    document.body.classList.add("printing-keys");
    const done = () => setPrinting(false);
    window.addEventListener("afterprint", done, { once: true });
    const t = setTimeout(() => window.print(), 50);
    return () => {
      clearTimeout(t);
      window.removeEventListener("afterprint", done);
      document.body.classList.remove("printing-keys");
    };
  }, [printing]);

  const set = (id: string, b: string | null) => {
    setOverrides({ ...overrides, [id]: b });
    setMsg("");
  };
  const changed = Object.keys(overrides).length > 0;

  return (
    <div className="general-tab keys-tab">
      <p className="muted">Click a key, then press the new combination. Esc cancels. Keys that need Ctrl or Alt keep callsign typing safe; F1 to F12 work on their own.</p>
      <table className="list">
        <thead>
          <tr><th>Action</th><th>Where</th><th>Key</th><th /></tr>
        </thead>
        <tbody>
          {SHORTCUTS.map((s) => {
            const b = bindingFor(s, overrides);
            return (
              <tr key={s.id}>
                <td>{s.label}</td>
                <td className="muted">{s.where}</td>
                <td>
                  <button className="key" onClick={() => { setMsg(""); setPicking(picking === s.id ? null : s.id); }} aria-label={`Change the key for ${s.label}`}>
                    {picking === s.id ? "Press a key…" : show(b)}
                  </button>
                </td>
                <td className="row-actions">
                  <button onClick={() => set(s.id, null)} disabled={b === null}>Clear</button>
                  <button onClick={() => { const { [s.id]: _drop, ...rest } = overrides; void _drop; setOverrides(rest); setMsg(""); }} disabled={!(s.id in overrides)}>Default</button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {msg && <p className="error" role="alert">{msg}</p>}
      <div className="row">
        <button onClick={() => { setOverrides({}); setMsg(""); }} disabled={!changed}>Reset all to defaults</button>
        <button onClick={() => setPrinting(true)}>Print the list…</button>
      </div>
      {printing && createPortal(<Sheet overrides={overrides} />, document.body)}
    </div>
  );
}

/** The printed page: every shortcut, set against its action. Only visible when printing. */
function Sheet({ overrides }: { overrides: Record<string, string | null> }) {
  const rows = [
    ...SHORTCUTS.filter((s) => bindingFor(s, overrides)).map((s) => ({ keys: show(bindingFor(s, overrides)), label: s.label, where: s.where })),
    ...FIXED.map((f) => ({ keys: show(f.keys), label: f.label, where: f.where })),
  ];
  return (
    <div className="key-print">
      <h1>QRZero keyboard shortcuts</h1>
      <table>
        <thead><tr><th>Key</th><th>Action</th><th>Where</th></tr></thead>
        <tbody>
          {rows.map((r) => <tr key={r.label}><td><b>{r.keys}</b></td><td>{r.label}</td><td>{r.where}</td></tr>)}
        </tbody>
      </table>
    </div>
  );
}
