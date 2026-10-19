// Frameless windows in the desktop app: QRZero draws the title bar, and asks the shell to move,
// minimize, maximize or close the window by setting the page title (no IPC; the same UI runs in a browser).

import { useEffect, useState } from "react";
import type React from "react";

/** on: this window is frameless now; saved: the choice for the next start (macOS applies it then). */
interface FrameInfo { supported: boolean; on: boolean; saved: boolean; os: string }

type Global = { __qrzeroFrame?: Partial<FrameInfo> };

// The shell also puts the frame in the page address (?desktop=frameless|framed&os=...): its start-up
// script can lose the race with the main window's first page load.
const params = new URLSearchParams(window.location.search);
const fromUrl: FrameInfo | null = ((d) => (d === "frameless" || d === "framed" ? { supported: true, on: d === "frameless", saved: d === "frameless", os: params.get("os") ?? "" } : null))(params.get("desktop"));

function info(): FrameInfo {
  const g = (window as unknown as Global).__qrzeroFrame;
  const base = fromUrl ?? { supported: false, on: false, saved: false, os: "" };
  return g ? { ...base, ...g, saved: g.saved ?? g.on ?? base.saved } : base;
}

/** macOS keeps its own window buttons over the bar; the bar leaves room for them. */
function markRoot(f: FrameInfo) {
  document.documentElement.dataset.frame = f.on ? (f.os === "macos" ? "macos" : "drawn") : "";
}
markRoot(info());

/** Input types that hold text, where the webview's own menu (cut, copy, paste) is useful. */
const NO_TEXT = ["checkbox", "radio", "button", "submit", "reset", "range", "color", "file", "image"];

/**
 * In the desktop app, right-clicking where QRZero has no menu of its own would open the webview's
 * browser menu (Back, Reload...). Hide it there, but keep it in text fields and over selected text.
 * In a browser nothing changes.
 */
export function hideBrowserMenu() {
  if (!info().supported) return;
  window.addEventListener("contextmenu", (e) => {
    const t = e.target;
    const text = t instanceof HTMLTextAreaElement || (t instanceof HTMLInputElement && !NO_TEXT.includes(t.type)) || (t instanceof HTMLElement && t.isContentEditable);
    if (text || window.getSelection()?.toString()) return;
    e.preventDefault();
  });
}

/** The address parameters that give a new window this window's frame. */
export function frameParams(): [string, string][] {
  const f = info();
  return f.supported ? [["desktop", f.on ? "frameless" : "framed"], ["os", f.os]] : [];
}

let shown = "";
let seq = 0;
let restore: ReturnType<typeof setTimeout> | undefined;

/** Sends the desktop shell a command. Each one gets its own number, so a repeat still changes the title. */
export function shell(cmd: string) {
  if (!document.title.startsWith("qrzero:")) shown = document.title;
  document.title = `qrzero:${cmd}:${++seq}`;
  clearTimeout(restore);
  restore = setTimeout(() => {
    if (document.title.startsWith("qrzero:")) document.title = shown;
  }, 100);
}

/** This window's frame, and whether it is maximized. */
export function useFrame() {
  const [state, setState] = useState(() => ({ ...info(), max: false }));
  useEffect(() => {
    const h = (e: Event) => {
      const d = (e as CustomEvent<Partial<FrameInfo> & { max: boolean }>).detail;
      (window as unknown as Global).__qrzeroFrame = { ...info(), ...d };
      const f = info();
      markRoot(f);
      setState({ ...f, max: d.max });
    };
    window.addEventListener("qrzero-frame", h);
    // The shell may have said so between the first render and now.
    setState((s) => ({ ...s, ...info() }));
    return () => window.removeEventListener("qrzero-frame", h);
  }, []);
  return state;
}

/** Switches between QRZero's title bar and the system frame. */
export const setFrameless = (on: boolean) => shell(`frame:${on ? 1 : 0}`);

/** On a title bar: drag moves the window, double-click maximizes or restores. Controls in the bar keep their clicks. */
export function barMouseDown(e: React.MouseEvent) {
  if (e.button !== 0 || !info().on) return;
  if ((e.target as HTMLElement).closest("button, input, select, textarea, a, label, [role=menu], .vfo-bar, .notice")) return;
  e.preventDefault();
  shell(e.detail === 2 ? "max" : "drag");
}
