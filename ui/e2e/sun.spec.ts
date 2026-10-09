import { expect, test } from "@playwright/test";
import { fmtUtc, greylineOverlap, subsolar, sunTimes } from "../src/sun";

// Checked against NOAA's solar calculator.
test("sunrise and sunset", () => {
  const boulder = sunTimes(new Date("2026-06-21T12:00Z"), { lat: 40.0, lon: -105.27 });
  expect([fmtUtc(boulder.rise!), fmtUtc(boulder.set!)]).toEqual(["11:32Z", "02:33Z"]);
  const greenwich = sunTimes(new Date("2026-10-08T00:00Z"), { lat: 51.48, lon: 0 });
  expect([fmtUtc(greenwich.rise!), fmtUtc(greenwich.set!)]).toEqual(["06:12Z", "17:22Z"]);
  expect(sunTimes(new Date("2026-12-21T12:00Z"), { lat: 78.2, lon: 15.6 }).polar).toBe("night");
  expect(sunTimes(new Date("2026-06-21T12:00Z"), { lat: 78.2, lon: 15.6 }).polar).toBe("day");
});

test("sub-solar point", () => {
  // June solstice: the sun is over the Tropic of Cancer; noon at Greenwich is near 0 degrees.
  const p = subsolar(new Date("2026-06-21T12:00Z"));
  expect(p.lat).toBeCloseTo(23.44, 1);
  expect(Math.abs(p.lon)).toBeLessThan(1);
});

test("grey line overlap", () => {
  const day = new Date("2026-10-08T12:00Z");
  const east = { lat: 40, lon: -75 };
  const west = { lat: 35, lon: 139 };
  const w = greylineOverlap(day, east, west);
  for (const x of w) expect(x.to.getTime()).toBeGreaterThan(x.from.getTime());
  // Same place: every sunrise and sunset window overlaps itself in full (a whole hour each).
  const same = greylineOverlap(day, east, east);
  expect(same.length).toBe(2);
  expect((same[0].to.getTime() - same[0].from.getTime()) / 6e4).toBe(60);
  // The poles in midwinter have no sunrise, so no window.
  expect(greylineOverlap(new Date("2026-12-21T12:00Z"), { lat: 78, lon: 15 }, east)).toEqual([]);
});
