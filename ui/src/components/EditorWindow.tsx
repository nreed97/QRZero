import { useEffect, useRef, useState } from "react";
import "../workspace.css";
import { listen, post, saveGeometry, type EditPayload } from "../bus";
import QsoEditor from "./QsoEditor";

/** The QSO editor in its own window. The main window sends it the QSO to edit. */
export default function EditorWindow() {
  const [edit, setEdit] = useState<EditPayload | null>(null);
  const dirty = useRef(false);
  const current = useRef<EditPayload | null>(null);
  const closing = useRef(false);

  const close = () => {
    closing.current = true;
    saveGeometry("editor");
    post({ t: "edit-closed" });
    window.close();
  };

  useEffect(() => {
    const off = listen((m) => {
      if (m.t === "edit-open") {
        const cur = current.current;
        if (cur && cur.qso.id !== m.edit.qso.id && dirty.current && !confirm(`Discard your changes to the QSO with ${cur.qso.fields.CALL}?`)) return;
        dirty.current = false;
        current.current = m.edit;
        setEdit(m.edit);
        document.title = `Edit ${m.edit.qso.fields.CALL ?? "QSO"} - QRZero`;
        window.focus();
      }
      if (m.t === "close-all") {
        closing.current = true;
        window.close();
      }
    });
    post({ t: "hello", pane: "editor" });
    const leave = () => {
      saveGeometry("editor");
      if (!closing.current) post({ t: "edit-closed" });
    };
    let timer: ReturnType<typeof setTimeout> | undefined;
    const resized = () => {
      clearTimeout(timer);
      timer = setTimeout(() => saveGeometry("editor"), 400);
    };
    const moved = setInterval(() => saveGeometry("editor"), 5000);
    window.addEventListener("beforeunload", leave);
    window.addEventListener("resize", resized);
    return () => {
      off();
      clearInterval(moved);
      window.removeEventListener("beforeunload", leave);
      window.removeEventListener("resize", resized);
    };
  }, []);

  if (!edit) return <p className="muted" style={{ padding: 16 }}>Waiting for the main QRZero window…</p>;

  return (
    <div className="popout editor-window">
      <QsoEditor
        qso={edit.qso}
        locations={edit.locations}
        equipment={edit.equipment}
        callsigns={edit.callsigns}
        onClose={close}
        onSaved={(q) => {
          dirty.current = false;
          const next = { ...edit, qso: q };
          current.current = next;
          setEdit(next);
          post({ t: "edit-saved", qso: q });
        }}
        onDeleted={() => {
          post({ t: "edit-deleted", id: edit.qso.id });
          close();
        }}
        onStep={(dir) => {
          if (dirty.current && !confirm(`Discard your changes to the QSO with ${edit.qso.fields.CALL}?`)) return;
          dirty.current = false;
          post({ t: "edit-step", id: edit.qso.id, dir });
        }}
        onDirty={(d) => (dirty.current = d)}
      />
    </div>
  );
}
