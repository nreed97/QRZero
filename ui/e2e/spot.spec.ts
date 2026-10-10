import { expect, test } from "@playwright/test";
import { adifToUnix, expandSpotComment, qsoEnd, spotBlocked } from "../src/spotCheck";

const on = { connected: true, maxMinutes: 10, comment: "" };
const now = Date.UTC(2026, 9, 9, 23, 0, 0) / 1000;

test("a QSO can only be spotted within the time limit", () => {
  expect(spotBlocked(on, "JA1XYZ", now - 9 * 60, now)).toBeNull();
  expect(spotBlocked(on, "JA1XYZ", now - 11 * 60, now)).toMatch(/Too old/);
  expect(spotBlocked({ ...on, maxMinutes: 30 }, "JA1XYZ", now - 11 * 60, now)).toBeNull();
});

test("spotting is refused with no cluster, no call or no valid time", () => {
  expect(spotBlocked({ ...on, connected: false }, "JA1XYZ", now, now)).toMatch(/Not connected/);
  expect(spotBlocked(on, "", now, now)).toMatch(/callsign/);
  expect(spotBlocked(on, "JA1XYZ", null, now)).toMatch(/date and time/);
});

test("the QSO time is read from its off time, else its start", () => {
  expect(adifToUnix("20261009", "2250")).toBe(Date.UTC(2026, 9, 9, 22, 50, 0) / 1000);
  expect(qsoEnd({ QSO_DATE: "20261009", TIME_ON: "225000", TIME_OFF: "225230" })).toBe(Date.UTC(2026, 9, 9, 22, 52, 30) / 1000);
  expect(qsoEnd({ QSO_DATE: "20261009", TIME_ON: "2250" })).toBe(Date.UTC(2026, 9, 9, 22, 50, 0) / 1000);
  expect(qsoEnd({})).toBeNull();
});

test("spot comment placeholders are filled from the QSO", () => {
  const q = { CALL: "JA1XYZ", MODE: "CW", RST_RCVD: "579", MY_GRIDSQUARE: "EN34" };
  expect(expandSpotComment("spotted with QRZero", q)).toBe("spotted with QRZero");
  expect(expandSpotComment("{mode} {rst_rcvd} via QRZero", q)).toBe("CW 579 via QRZero");
  expect(expandSpotComment("{name} {my_grid}  {bogus}!", q)).toBe("EN34 !");
  expect(expandSpotComment("{call} {mode} {rst_rcvd} {my_grid} spotted with QRZero", q)).toHaveLength(30);
});
