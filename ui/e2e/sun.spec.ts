import { expect, test } from "@playwright/test";
import { fmtUtc, subsolar, sunTimes } from "../src/sun";

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
