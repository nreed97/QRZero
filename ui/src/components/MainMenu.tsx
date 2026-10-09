import { Fragment } from "react";
import { usePopup } from "./usePopup";

export type MenuAction = "import" | "export" | "qsl" | "qsllookup" | "settings" | "help";

const ITEMS: { id: MenuAction; label: string; hint?: string }[] = [
  { id: "import", label: "Import ADIF…" },
  { id: "export", label: "Export ADIF…" },
  { id: "qsl", label: "QSL: LoTW, QRZ, Club Log…" },
  { id: "qsllookup", label: "QSL lookup…", hint: "Alt+Q" },
  { id: "settings", label: "Settings…" },
  { id: "help", label: "Help", hint: "F1" },
];

/** The ☰ menu at the right of the top bar. */
export default function MainMenu({ onPick }: { onPick: (a: MenuAction) => void }) {
  const { open, setOpen, box } = usePopup<HTMLDivElement>();
  return (
    <div className="layout-menu" ref={box}>
      <button className="burger" onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="menu" aria-label="Menu" title="Import, export, QSL, settings and help">
        &#9776;
      </button>
      {open && (
        <div className="layout-pop" role="menu">
          {ITEMS.map((it, i) => (
            <Fragment key={it.id}>
              {i === 4 && <hr />}
              <button role="menuitem" className="item" onClick={() => { setOpen(false); onPick(it.id); }}>
                {it.label}
                {it.hint && <span className="where">{it.hint}</span>}
              </button>
            </Fragment>
          ))}
        </div>
      )}
    </div>
  );
}
