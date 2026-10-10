import { useEffect, useRef, useSyncExternalStore } from "react";
import { api } from "./api";
import { SHORTCUTS, bindingFor, bindingOf, type Binding, type Overrides } from "./shortcutKeys";

export { FIXED, SHORTCUTS, bindingFor, bindingOf, problemWith, show, type Binding, type Overrides } from "./shortcutKeys";

// ---- shared state, saved with the other preferences in the log database ----

const PREF = "shortcuts";
const listeners = new Set<() => void>();
let current: Overrides = {};
let loaded = false;

function load() {
  if (loaded) return;
  loaded = true;
  api
    .getPref<Overrides>(PREF)
    .then((v) => {
      if (v && typeof v === "object") {
        current = v;
        listeners.forEach((l) => l());
      }
    })
    .catch(() => {});
}

export function setOverrides(next: Overrides) {
  current = next;
  api.setPref(PREF, next).catch(() => {});
  listeners.forEach((l) => l());
}

function subscribe(cb: () => void) {
  load();
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

export function useOverrides(): Overrides {
  return useSyncExternalStore(subscribe, () => current);
}

/** The key now assigned to an action, for showing in menus and tooltips. */
export function useBinding(id: string): Binding | null {
  const o = useOverrides();
  const s = SHORTCUTS.find((x) => x.id === id);
  return s ? bindingFor(s, o) : null;
}

/** True while the Keyboard settings are waiting for the operator to press a new key. */
export const capture = { active: false };

/** Is this key press the shortcut for an action? For handlers that already live where the key is read. */
export function isShortcut(id: string, e: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): boolean {
  load();
  const s = SHORTCUTS.find((x) => x.id === id);
  const b = s ? bindingFor(s, current) : null;
  return !!b && bindingOf(e) === b;
}

/** Runs `fn` whenever the key assigned to `id` is pressed anywhere in the window. */
export function useShortcut(id: string, fn: () => void) {
  useSubscription();
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (capture.active || e.defaultPrevented || !isShortcut(id, e)) return;
      e.preventDefault();
      ref.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [id]);
}

/** Starts loading the saved choices, which are read straight from `current` at key time. */
function useSubscription() {
  useSyncExternalStore(subscribe, () => current);
}
