import { expect, test, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const spot = (seq: number, call: string, khz: number, mode: string, band: string, country: string, dxcc: number, age: number, need: "dxcc" | "band" | "mode", comment: string) => ({
  seq, received: now - age, time: "2214", spotter: "K1TTT", call, freq_hz: khz * 1000, band, mode, comment,
  entity: { name: country, prefix: "", cont: "AS", dxcc, lat: 0, lon: 0 },
  needed: { new_call: true, new_call_band: true, new_dxcc: need === "dxcc", new_band: need === "band", new_mode: need === "mode" },
});
const snapshot = {
  config: { nodes: [], auto_connect: false, spot_max_minutes: 10, spot_comment: "" },
  state: "connected to test", connected: true, lines: [],
  spots: [
    spot(1, "JA1XYZ", 14195, "SSB", "20m", "Japan", 339, 30, "dxcc", "599 here"),
    spot(2, "P5/4L4FN", 14025, "CW", "20m", "North Korea", 344, 200, "dxcc", "up 2"),
    spot(3, "3Y0K", 10108, "CW", "30m", "Bouvet", 24, 400, "dxcc", "UP 1"),
    spot(4, "ZL7A", 21025, "CW", "15m", "Chatham Islands", 34, 100, "band", "up 3"),
    spot(5, "VK9XY", 7040, "CW", "40m", "Christmas Island", 35, 60, "mode", ""),
  ],
};
// Ranks as Club Log gives them: DXCC code to rank. JA (339) is absent.
const wanted = { ranks: { "344": 1, "24": 8, "34": 120, "35": 150 }, fetched_at: now, error: null };

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("Needed now shows the Most Wanted rank and sorts by it within each need", async ({ page }) => {
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: snapshot }) : r.continue()));
  await page.route("**/api/mostwanted", (r) => r.fulfill({ json: wanted }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menu").getByLabel("Needed now").check();
  await page.getByRole("button", { name: /^Layout/ }).click();
  const rows = page.locator(".needed-row:not(.head)");
  await expect(rows).toHaveCount(5);
  // New entities by rank (1, 8), the unranked one after them, then the band, then the mode.
  await expect(rows.locator(".call")).toHaveText(["P5/4L4FN", "3Y0K", "JA1XYZ", "ZL7A", "VK9XY"]);
  await expect(rows.nth(0)).toContainText("North Korea");
  await expect(rows.nth(0).locator("> span").nth(1)).toHaveText("1");
  await expect(rows.nth(2).locator("> span").nth(1)).toHaveText("");
  if (process.env.SHOT) await page.screenshot({ path: process.env.SHOT });
});

test("Needed now still lists spots when the Most Wanted list is not loaded", async ({ page }) => {
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: snapshot }) : r.continue()));
  await page.route("**/api/mostwanted", (r) => r.fulfill({ json: { ranks: {}, fetched_at: null, error: "offline" } }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menu").getByLabel("Needed now").check();
  await page.getByRole("button", { name: /^Layout/ }).click();
  const rows = page.locator(".needed-row:not(.head)");
  // Newest spot first within each need.
  await expect(rows.locator(".call")).toHaveText(["JA1XYZ", "P5/4L4FN", "3Y0K", "ZL7A", "VK9XY"]);
  await expect(page.getByText("Most Wanted ranks not loaded")).toBeVisible();
});

test("Needed now leaves beacons out", async ({ page }) => {
  const beacons = [
    spot(10, "4U1UN", 14100, "CW", "20m", "United Nations HQ", 289, 20, "dxcc", "NCDXF"),
    spot(11, "DK0WCY/B", 10144, "CW", "30m", "Germany", 230, 20, "dxcc", ""),
    spot(12, "OH2B", 28200, "CW", "10m", "Finland", 224, 20, "dxcc", ""),
    spot(13, "ZS6DN", 7040, "CW", "40m", "South Africa", 462, 20, "dxcc", "beacon"),
  ];
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: { ...snapshot, spots: [...snapshot.spots, ...beacons] } }) : r.continue()));
  await page.route("**/api/mostwanted", (r) => r.fulfill({ json: wanted }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menu").getByLabel("Needed now").check();
  await page.getByRole("button", { name: /^Layout/ }).click();
  const rows = page.locator(".needed-row:not(.head)");
  await expect(rows).toHaveCount(5);
  await expect(rows.locator(".call")).not.toContainText(["4U1UN"]);
  if (process.env.SHOT2) await page.screenshot({ path: process.env.SHOT2 });
});

test("Needed now follows the Cluster pane's origin filters", async ({ page }) => {
  const from = (s: ReturnType<typeof spot>, cont: string) => ({ ...s, spotter_entity: { name: "X", prefix: "", cont, dxcc: 1, lat: 0, lon: 0 } });
  const spots = [from(snapshot.spots[0], "AS"), from(snapshot.spots[1], "NA"), from(snapshot.spots[2], "NA")];
  const home = { name: "United States", prefix: "K", cont: "NA", dxcc: 291, lat: 40, lon: -100 };
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: { ...snapshot, spots, home } }) : r.continue()));
  await page.route("**/api/mostwanted", (r) => r.fulfill({ json: wanted }));
  await page.addInitScript(() => localStorage.setItem("qrzero.cluster", JSON.stringify({ origin: "continent" })));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menu").getByLabel("Needed now").check();
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("tab", { name: "Needed now" }).click();
  // JA1XYZ was spotted from Asia, so the "spotted from my continent" choice hides it here too.
  await expect(page.locator(".needed-row:not(.head) .call")).toHaveText(["P5/4L4FN", "3Y0K"]);
  await expect(page.getByText("Origin filter on")).toBeVisible();
  if (process.env.SHOT) await page.screenshot({ path: process.env.SHOT });
});
