import { useEffect, useRef, useState } from "react";
import "../workspace.css";
import { listen, post, saveGeometry, type PopContext } from "../bus";
import { paneTitle, type PaneId } from "../workspace";
import SharedPane, { type PaneActions } from "./SharedPanes";

/** One pane in its own window, kept in step with the main window. */
export default function Popout({ pane }: { pane: PaneId }) {
  const [ctx, setCtx] = useState<PopContext | null>(null);
  const closing = useRef(false);

  useEffect(() => {
    document.title = `${paneTitle(pane)} - QRZero`;
    const off = listen((m) => {
      if (m.t === "ctx") setCtx(m.ctx);
      if (m.t === "close-all") {
        closing.current = true;
        window.close();
      }
    });
    post({ t: "hello", pane });
    post({ t: "want-ctx" });
    let timer: ReturnType<typeof setTimeout> | undefined;
    const resized = () => {
      clearTimeout(timer);
      timer = setTimeout(() => saveGeometry(pane), 400);
    };
    const leave = () => {
      saveGeometry(pane);
      if (!closing.current) post({ t: "bye", pane });
    };
    window.addEventListener("resize", resized);
    window.addEventListener("beforeunload", leave);
    // Window moves fire no event; check now and then.
    const moved = setInterval(() => saveGeometry(pane), 5000);
    return () => {
      off();
      clearInterval(moved);
      window.removeEventListener("resize", resized);
      window.removeEventListener("beforeunload", leave);
    };
  }, [pane]);

  const act: PaneActions = {
    onPick: (pick) => post({ t: "pick", pick }),
    onEdit: (qso) => post({ t: "edit", qso }),
    onCopy: (fields) => post({ t: "copy", fields }),
    onShowQsos: (filter) => post({ t: "show-qsos", filter }),
    onSettings: (tab) => post({ t: "settings", tab }),
  };

  return (
    <div className="popout">
      <div className="popout-bar">
        <b>{paneTitle(pane)}</b>
        <span className="spacer" />
        <button
          className="tiny"
          onClick={() => {
            saveGeometry(pane);
            post({ t: "bye", pane });
            closing.current = true;
            window.close();
          }}
          title="Put this pane back in the main window"
        >
          Dock back
        </button>
      </div>
      <div className="popout-body">
        {ctx ? <SharedPane id={pane} ctx={ctx} act={act} /> : <p className="muted" style={{ padding: 16 }}>Waiting for the main QRZero window…</p>}
      </div>
    </div>
  );
}
