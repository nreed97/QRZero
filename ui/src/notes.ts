// Station notes: shared helpers for the Notes pane and the Station pane.

import { useEffect, useState } from "react";
import { api } from "./api";

/** The home call inside a portable form, like the server's base_call ("EA8/DL1ABC/P" gives "DL1ABC"). */
export function baseCall(call: string): string {
  return call
    .trim()
    .toUpperCase()
    .split("/")
    .reduce((best, p) => (p.length > best.length ? p : best), "");
}

/** A note was saved (text) or deleted (null). */
export interface NoteChange {
  logId: number;
  call: string;
  text: string | null;
}

// Changes reach this window's listeners directly and other windows over a channel.
const local = new EventTarget();
const channel: BroadcastChannel | null = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("qrzero-notes");
channel?.addEventListener("message", (e: MessageEvent<NoteChange>) => local.dispatchEvent(new CustomEvent("change", { detail: e.data })));

export function publishNote(c: NoteChange) {
  local.dispatchEvent(new CustomEvent("change", { detail: c }));
  channel?.postMessage(c);
}

export function onNoteChange(f: (c: NoteChange) => void): () => void {
  const h = (e: Event) => f((e as CustomEvent<NoteChange>).detail);
  local.addEventListener("change", h);
  return () => local.removeEventListener("change", h);
}

/**
 * The note to show for a call: `initial` (from the lookup) until the note is
 * edited somewhere, then the edited text.
 */
export function useLiveNote(call: string, initial: string | null): string | null {
  const base = baseCall(call);
  const [edited, setEdited] = useState<{ call: string; text: string | null } | null>(null);
  useEffect(() => setEdited(null), [initial]);
  useEffect(() => onNoteChange((c) => setEdited({ call: c.call, text: c.text })), []);
  return edited && edited.call === base ? edited.text : initial;
}

/** Saves while the window is closing, when an ordinary request could be cut off. */
export function saveNoteOnExit(logId: number, call: string, text: string) {
  try {
    void fetch(`/api/logs/${logId}/notes/${encodeURIComponent(call)}`, {
      method: "PUT",
      headers: { "x-qrzero-token": api.token(), "content-type": "application/json" },
      body: JSON.stringify({ text }),
      keepalive: true,
    });
  } catch {
    /* nothing more can be done while closing */
  }
}
