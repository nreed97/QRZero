import { expect, test } from "@playwright/test";
import { SHORTCUTS, bindingFor, bindingOf, problemWith } from "../src/shortcutKeys";

const press = (key: string, o: { ctrl?: boolean; alt?: boolean; shift?: boolean } = {}) =>
  bindingOf({ key, ctrlKey: !!o.ctrl, altKey: !!o.alt, shiftKey: !!o.shift, metaKey: false });

test("key presses become bindings", () => {
  expect(press("q", { alt: true })).toBe("Alt+Q");
  expect(press("ArrowUp", { alt: true })).toBe("Alt+Up");
  expect(press("s", { ctrl: true, shift: true })).toBe("Ctrl+Shift+S");
  expect(press("F1")).toBe("F1");
  expect(press("`", { shift: false })).toBe("`");
  expect(press("Alt", { alt: true })).toBeNull();
});

test("the defaults match the keys QRZero always had", () => {
  const def = (id: string) => bindingFor(SHORTCUTS.find((s) => s.id === id)!, {});
  expect(def("qsllookup")).toBe("Alt+Q");
  expect(def("help")).toBe("F1");
  expect(def("editsave")).toBe("Ctrl+S");
  expect(def("radioswap")).toBe("`");
});

test("conflicts and unsafe keys are refused", () => {
  expect(problemWith("Alt+Q", {}, "settings")).toMatch(/QSL Detail Lookup/);
  expect(problemWith("Alt+Q", {}, "qsllookup")).toBeNull();
  expect(problemWith("Alt+Q", { qsllookup: null }, "settings")).toBeNull();
  expect(problemWith("Q", {}, "settings")).toMatch(/Ctrl or Alt/);
  expect(problemWith("Ctrl+C", {}, "settings")).toMatch(/Windows/);
  expect(problemWith("Alt+3", {}, "settings")).toMatch(/always does something else/);
  expect(problemWith("Ctrl+Shift+K", {}, "settings")).toBeNull();
});
