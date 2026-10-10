// The shortcut list and key matching. No browser or server access here, so it can be tested on its own.

/** A shortcut as text: modifiers in a fixed order, then the key, e.g. "Ctrl+Shift+S", "Alt+Up", "F1". */
export type Binding = string;

export interface ShortcutDef {
  id: string;
  label: string;
  where: string;
  /** null: no key until the operator picks one. */
  def: Binding | null;
}

/** Shortcuts the operator can change. The ids are what is saved, so never reuse one. */
export const SHORTCUTS: ShortcutDef[] = [
  { id: "qsllookup", label: "QSL Detail Lookup", where: "Anywhere", def: "Alt+Q" },
  { id: "help", label: "User guide", where: "Anywhere", def: "F1" },
  { id: "settings", label: "Settings", where: "Anywhere", def: null },
  { id: "qsl", label: "QSL: LoTW, QRZ, Club Log", where: "Anywhere", def: null },
  { id: "radioswap", label: "Swap between radio 1 and 2", where: "QSO entry (SO2R)", def: "`" },
  { id: "editsave", label: "Save the QSO", where: "Edit QSO window", def: "Ctrl+S" },
  { id: "editprev", label: "Previous QSO in the log grid", where: "Edit QSO window", def: "Alt+Up" },
  { id: "editnext", label: "Next QSO in the log grid", where: "Edit QSO window", def: "Alt+Down" },
];

/** Keys that always do the same thing. Listed in the printout, and never given to a rebindable action. */
export const FIXED: { keys: Binding; label: string; where: string }[] = [
  { keys: "Enter", label: "Log the QSO", where: "QSO entry" },
  { keys: "Esc", label: "Clear the form, or close a window", where: "Anywhere" },
  { keys: "Space", label: "Jump to the sent report", where: "Call field" },
  { keys: "Tab", label: "Next field", where: "Anywhere" },
  { keys: "Alt+1", label: "Follow radio 1 (Alt+2 for radio 2, up to Alt+9)", where: "QSO entry" },
  { keys: "Up", label: "Move up a row (Down moves down)", where: "Lists and the log grid" },
  { keys: "Ctrl+click", label: "Add or remove a row from the selection", where: "Log grid" },
  { keys: "Shift+click", label: "Select a range of rows", where: "Log grid" },
  { keys: "Ctrl+Wheel", label: "Zoom", where: "Band map" },
];

/** Keys the program or Windows itself needs; binding them would break copy and paste, or the window. */
const RESERVED = ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y", "Ctrl+W", "Ctrl+R", "Ctrl+P", "Alt+F4", "F5", "F11", "F12", "Tab", "Enter", "Esc", "Space"];

const MODS = ["Ctrl", "Alt", "Shift", "Meta"] as const;

const NAMES: Record<string, string> = { ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", Escape: "Esc", " ": "Space", Delete: "Del" };

/** The binding a key press stands for, or null while only modifiers are held. */
export function bindingOf(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): Binding | null {
  if (["Control", "Alt", "Shift", "Meta", "AltGraph", "Dead", "Unidentified"].includes(e.key)) return null;
  let key = NAMES[e.key] ?? e.key;
  if (key.length === 1) key = key.toUpperCase();
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Meta"].filter(Boolean) as string[];
  // Shift only changes what a symbol key produces ("~" for "`"), so it isn't part of those bindings.
  const out = key.length === 1 && !/[A-Z0-9]/.test(key) ? mods.filter((m) => m !== "Shift") : mods;
  return [...out, key].join("+");
}

/** Why a binding can't be used, or null when it is fine. `except` is the action being changed. */
export function problemWith(b: Binding, overrides: Overrides, except: string): string | null {
  const parts = b.split("+");
  const key = parts[parts.length - 1];
  const hasMod = parts.length > 1 && parts.slice(0, -1).some((m) => (MODS as readonly string[]).includes(m));
  const isFn = /^F([1-9]|1[0-2])$/.test(key);
  if (RESERVED.includes(b)) return `${b} is used by the program or Windows.`;
  // A bare letter would stop you typing it; the one exception is the back-quote key, which no callsign uses.
  if (!hasMod && !isFn && key !== "`") return "Hold Ctrl or Alt with the key, so typing a callsign doesn't trigger it.";
  if (FIXED.some((f) => f.keys === b) || (parts.length === 2 && parts[0] === "Alt" && /^[1-9]$/.test(key))) return `${b} always does something else (see the list).`;
  const other = SHORTCUTS.find((s) => s.id !== except && bindingFor(s, overrides) === b);
  if (other) return `${b} is already used for "${other.label}". Clear that one first.`;
  return null;
}

/** Saved choices: a binding, or null for "none". Actions not listed keep their default. */
export type Overrides = Record<string, Binding | null>;

export function bindingFor(s: ShortcutDef, overrides: Overrides): Binding | null {
  return s.id in overrides ? overrides[s.id] : s.def;
}

/** "Alt+Q" as shown to people: the same, with the arrow keys spelled out. */
export function show(b: Binding | null): string {
  return b ? b.replace(/\bUp$/, "↑").replace(/\bDown$/, "↓").replace(/\bLeft$/, "←").replace(/\bRight$/, "→") : "none";
}
