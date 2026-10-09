import { expect, test } from "@playwright/test";
import { AlertGate, needRank, spotKey, type NeededAlertConfig } from "../src/needed";
import type { Spot } from "../src/types";

const flags = (o: Partial<NonNullable<Spot["needed"]>>) => ({ new_call: true, new_call_band: true, new_dxcc: false, new_band: false, new_mode: false, ...o });
const spot = (call: string, o: Partial<NonNullable<Spot["needed"]>>, band = "20m", mode = "CW") => ({ call, band, mode, needed: flags(o) }) as Spot;
const cfg: NeededAlertConfig = { sound: true, popup: true, level: 2, repeatMin: 30, listMin: 30 };

test("needed spots rank entity, then band, then mode", () => {
  expect(needRank(spot("A", { new_dxcc: true, new_band: true }))).toBe(0);
  expect(needRank(spot("B", { new_band: true, new_mode: true }))).toBe(1);
  expect(needRank(spot("C", { new_mode: true }))).toBe(2);
  expect(needRank(spot("D", {}))).toBeNull();
  expect(needRank({ needed: null })).toBeNull();
});

test("an alert is not repeated for the same call, band and mode", () => {
  const gate = new AlertGate();
  const s = spot("VP8LP", { new_dxcc: true });
  expect(gate.check(s, cfg, 1000)).toBe(true);
  expect(gate.check(s, cfg, 1000 + 29 * 60)).toBe(false);
  // Another band or mode is a different slot.
  expect(gate.check(spot("VP8LP", { new_dxcc: true }, "40m"), cfg, 1100)).toBe(true);
  expect(gate.check(spot("VP8LP", { new_dxcc: true }, "20m", "SSB"), cfg, 1100)).toBe(true);
  expect(spotKey(s)).not.toBe(spotKey(spot("VP8LP", {}, "20m", "SSB")));
  // After the window it may alert again.
  expect(gate.check(s, cfg, 1000 + 31 * 60)).toBe(true);
});

test("the alert level limits what alerts", () => {
  const gate = new AlertGate();
  const mode = spot("K1", { new_mode: true });
  expect(gate.check(mode, { ...cfg, level: 0 }, 0)).toBe(false);
  expect(gate.check(mode, { ...cfg, level: 1 }, 0)).toBe(false);
  expect(gate.check(mode, { ...cfg, level: 2 }, 0)).toBe(true);
  // A spot that did not alert is not remembered as having alerted.
  expect(gate.check(spot("K2", { new_band: true }), { ...cfg, level: 1 }, 0)).toBe(true);
  expect(gate.check(spot("K3", {}), cfg, 0)).toBe(false);
});
