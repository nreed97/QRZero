import { expect, test } from "@playwright/test";
import { forecastDay } from "../src/forecast";

const row = (f: ReturnType<typeof forecastDay>, band: string) => f.grid[f.bands.findIndex((b) => b.name === band)];

const ohio = { lat: 40.1, lon: -83 };
const europe = { lat: 50, lon: 8 };
const day = new Date("2026-10-08T12:00Z");

test("sensible shape: low bands at night, high bands by day", () => {
  const w = { sfi: 150, k: 1 };
  const eu = forecastDay(day, ohio, europe, w);
  // 80m closes around local noon (17Z in Ohio) and is open in the small hours.
  expect(row(eu, "80m")[17]).toBe("closed");
  expect(row(eu, "80m")[5]).not.toBe("closed");
  // 15m is open in the daytime and shut in the middle of the night on this path.
  expect(row(eu, "15m").slice(12, 18)).toContain("good");
  expect(row(eu, "10m")[6]).toBe("closed");
});

test("a low solar flux shuts the high bands, a storm hurts polar paths", () => {
  const quiet = forecastDay(day, ohio, europe, { sfi: 70, k: 1 });
  expect(row(quiet, "10m").every((c) => c === "closed")).toBe(true);
  const hi = forecastDay(day, ohio, europe, { sfi: 180, k: 1 });
  expect(row(hi, "10m").some((c) => c !== "closed")).toBe(true);
  const stormy = forecastDay(day, ohio, europe, { sfi: 180, k: 6 });
  const count = (f: typeof hi) => f.grid.flat().filter((c) => c !== "closed").length;
  expect(count(stormy)).toBeLessThanOrEqual(count(hi));
});
