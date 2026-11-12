import { expect, test } from "@playwright/test";
import { AlertGate, compareNeeded, isBeacon, needRank, spotKey, type NeededAlertConfig } from "../src/needed";
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

test("within a need, the most wanted entity comes first, then the newest spot", () => {
  const rows = [
    { id: "unranked new", rank: 0 as const, wanted: Infinity, received: 900 },
    { id: "band, wanted 3", rank: 1 as const, wanted: 3, received: 100 },
    { id: "entity, wanted 50 old", rank: 0 as const, wanted: 50, received: 100 },
    { id: "entity, wanted 50 new", rank: 0 as const, wanted: 50, received: 200 },
    { id: "entity, wanted 5", rank: 0 as const, wanted: 5, received: 10 },
  ];
  expect(rows.sort(compareNeeded).map((r) => r.id)).toEqual(["entity, wanted 5", "entity, wanted 50 new", "entity, wanted 50 old", "unranked new", "band, wanted 3"]);
});

test("beacons are recognised by call, comment or frequency", () => {
  const b = (call: string, khz: number, comment = "") => isBeacon({ call, comment, freq_hz: khz * 1000 });
  expect(b("DK0WCY/B", 10144)).toBe(true);
  expect(b("4U1UN", 14100.2, "")).toBe(true);
  expect(b("OH2B", 28200)).toBe(true);
  expect(b("ZS6DN", 21150.0)).toBe(true);
  expect(b("VK6RBP", 14110, "NCDXF beacon")).toBe(true);
  expect(b("JA2IGY", 7040, "BCN")).toBe(true);
  expect(b("VP8LP", 14025.3, "up 2")).toBe(false);
  expect(b("K1ABC/BB", 14025)).toBe(false);
  expect(new AlertGate().check({ ...spot("DK0WCY/B", { new_dxcc: true }), comment: "", freq_hz: 10144000 } as Spot, cfg, 0)).toBe(false);
});

test("the Cluster pane's origin filters decide which spots count", async () => {
  const { passesOrigin, ORIGIN_DEFAULTS } = await import("../src/spotOriginFilter");
  const ent = (prefix: string, cont: string, dxcc: number, lat = 0, lon = 0) => ({ prefix, cont, dxcc, name: prefix, lat, lon });
  const home = ent("K", "NA", 291) as never;
  const s = (entity: ReturnType<typeof ent> | null, spotter: ReturnType<typeof ent> | null) => ({ entity, spotter_entity: spotter }) as unknown as Spot;
  const jaFromJa = s(ent("JA", "AS", 339), ent("JA", "AS", 339));
  const euFromNa = s(ent("DL", "EU", 230), ent("K", "NA", 291));
  expect(passesOrigin(jaFromJa, ORIGIN_DEFAULTS, home)).toBe(true);
  expect(passesOrigin(jaFromJa, { ...ORIGIN_DEFAULTS, hideSelf: true }, home)).toBe(false);
  expect(passesOrigin(jaFromJa, { ...ORIGIN_DEFAULTS, origin: "continent" }, home)).toBe(false);
  expect(passesOrigin(euFromNa, { ...ORIGIN_DEFAULTS, origin: "continent" }, home)).toBe(true);
  expect(passesOrigin(euFromNa, { ...ORIGIN_DEFAULTS, hideCont: ["EU"] }, home)).toBe(false);
  expect(passesOrigin(s(ent("K", "NA", 291), null), { ...ORIGIN_DEFAULTS, hideHome: true }, home)).toBe(false);
  expect(passesOrigin(s(null, null), { ...ORIGIN_DEFAULTS, hideUnknown: true }, home)).toBe(false);
  // Without a home entity the continent choice can't be applied, so spots are kept.
  expect(passesOrigin(jaFromJa, { ...ORIGIN_DEFAULTS, origin: "continent" }, null)).toBe(true);
});
