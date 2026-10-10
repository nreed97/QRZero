import { useRef } from "react";

export type SaveMsg = { text: string; ok: boolean } | null;

/** Save confirmation that fades out after a couple of seconds, so the next save shows again. */
export function SaveNote({ msg }: { msg: SaveMsg }) {
  // Each new message object restarts the fade, even when the text is the same.
  const last = useRef<SaveMsg>(null);
  const n = useRef(0);
  if (last.current !== msg) {
    last.current = msg;
    n.current += 1;
  }
  if (!msg) return null;
  const fades = msg.ok && msg.text === "Saved.";
  return (
    <span key={n.current} className={`${msg.ok ? "ok" : "err"}${fades ? " fades" : ""}`}>
      {msg.text}
    </span>
  );
}

/** The message plus Save and Save and close, shared by every settings page. `onSave` resolves true when it worked. */
export default function SaveBar({ msg, onSave, onClose, children }: { msg: SaveMsg; onSave: () => Promise<boolean>; onClose?: () => void; children?: React.ReactNode }) {
  return (
    <div className="buttons">
      <SaveNote msg={msg} />
      <span className="spacer" />
      {children}
      <button className="primary" onClick={() => void onSave()}>Save</button>
      {onClose && <button onClick={async () => { if (await onSave()) onClose(); }}>Save and close</button>}
    </div>
  );
}
