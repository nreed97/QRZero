import { Fragment } from "react";
import { usePopup } from "./usePopup";
import { show, useOverrides, SHORTCUTS, bindingFor } from "../shortcuts";
import { setFrameless, useFrame } from "../frame";

export type MenuAction = "import" | "export" | "bulk" | "qsl" | "qslqueue" | "stats" | "settings" | "help" | "about";

const ITEMS: { id: MenuAction; label: string }[] = [
  { id: "import", label: "Import ADIF…" },
  { id: "export", label: "Export ADIF…" },
  { id: "bulk", label: "Bulk edit QSOs…" },
  { id: "qsl", label: "QSL: LoTW, QRZ, Club Log…" },
  { id: "qslqueue", label: "QSL Queue…" },
  { id: "stats", label: "Statistics…" },
  { id: "settings", label: "Settings…" },
  { id: "help", label: "Help" },
  { id: "about", label: "About QRZero" },
];

/** The ☰ menu at the right of the top bar. */
export default function MainMenu({ onPick }: { onPick: (a: MenuAction) => void }) {
  const { open, setOpen, box } = usePopup<HTMLDivElement>();
  const keys = useOverrides();
  const frame = useFrame();
  const hintOf = (id: string) => {
    const s = SHORTCUTS.find((x) => x.id === id);
    return s && bindingFor(s, keys) ? show(bindingFor(s, keys)) : undefined;
  };
  return (
    <div className="layout-menu" ref={box}>
      <button className="burger" onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="menu" aria-label="Menu" title="Import, export, bulk edit, QSL, settings and help">
        &#9776;
      </button>
      {open && (
        <div className="layout-pop" role="menu">
          {ITEMS.map((it, i) => (
            <Fragment key={it.id}>
              {i === 6 && <hr />}
              <button role="menuitem" className="item" onClick={() => { setOpen(false); onPick(it.id); }}>
                {it.label}
                {hintOf(it.id) && <span className="where">{hintOf(it.id)}</span>}
              </button>
            </Fragment>
          ))}
          {frame.supported && (
            <>
              <hr />
              <label className="item" title="Draw QRZero's own title bar instead of the system window frame">
                <input type="checkbox" checked={frame.saved} onChange={() => { setOpen(false); setFrameless(!frame.saved); }} />
                Frameless windows
              </label>
            </>
          )}
        </div>
      )}
    </div>
  );
}
