import { expect, test, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const spot = (seq: number, spotter: string, khz: number, band: string, mode: string, comment: string, country: string, age: number) => ({
  seq, received: now - age, time: "0212", spotter, call: "W0NY", freq_hz: khz * 1000, band, mode, comment,
  entity: null, spotter_entity: { name: country, prefix: "", cont: "EU", dxcc: seq, lat: 0, lon: 0 }, needed: null,
});
const mine = [
  spot(1, "DL8LAS-#", 14025.2, "20m", "CW", "CQ 28 dB 22 WPM", "Germany", 5000),
  spot(2, "G4XYZ", 7012.0, "40m", "CW", "tnx qso", "England", 60),
  spot(3, "OH2BH", 14025.1, "20m", "CW", "", "Finland", 10),
];
const cluster = { config: { nodes: [], auto_connect: false, spot_max_minutes: 10, spot_comment: "" }, state: "", connected: true, lines: [], spots: [] };

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("Spotted me lists spots of my callsigns and filters them", async ({ page }) => {
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: cluster }) : r.continue()));
  await page.route("**/api/cluster/mine", (r) => r.fulfill({ json: mine }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  if (!(await page.getByRole("tab", { name: "Spotted me" }).isVisible())) {
    await page.getByRole("button", { name: /^Layout/ }).click();
    await page.getByRole("menu").getByLabel("Spotted me").check();
    await page.getByRole("button", { name: /^Layout/ }).click();
  }
  await page.getByRole("tab", { name: "Spotted me" }).click();
  const pane = page.locator(".cluster", { has: page.getByLabel("Spots of your callsigns") });
  const rows = pane.locator(".spot-row:not(.head)");
  await expect(rows).toHaveCount(3);
  // Newest first.
  await expect(rows.first()).toContainText("OH2BH");
  await pane.getByLabel("Band").selectOption("40m");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("England");
  await pane.getByLabel("Band").selectOption("");
  await pane.getByLabel("Age").selectOption("15");
  await expect(rows).toHaveCount(2);
});
