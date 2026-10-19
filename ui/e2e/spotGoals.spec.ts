import { expect, test, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const spot = (seq: number, call: string, khz: number, band: string, country: string, dxcc: number, age: number, extra: Record<string, unknown>) => ({
  seq, received: now - age, time: "2214", spotter: "K1TTT", call, freq_hz: khz * 1000, band, mode: "CW", comment: "",
  entity: { name: country, prefix: "", cont: "AS", dxcc, lat: 0, lon: 0 },
  needed: { new_call: true, new_call_band: true, new_dxcc: false, new_band: false, new_mode: false },
  goals: [],
  marathon: null,
  ...extra,
});
const snapshot = {
  config: { nodes: [], auto_connect: false, spot_max_minutes: 10, spot_comment: "" },
  state: "connected to test", connected: true, lines: [],
  spots: [
    spot(1, "UA9XX", 14025, "20m", "Asiatic Russia", 15, 30, { goals: [{ award: "waz", key: "18", column: "mixed", status: "new" }] }),
    spot(2, "IT9ABC", 14030, "20m", "Sicily", 248, 60, { marathon: { country: true, zone: false } }),
    spot(3, "JA1XYZ", 14035, "20m", "Japan", 339, 90, { goals: [{ award: "dxcc", key: "339", column: "20m", status: "worked" }] }),
    // Nothing needed: not listed.
    spot(4, "W1AW", 14040, "20m", "United States", 291, 20, {}),
  ],
};

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("spots are flagged for the award goals and the DX Marathon", async ({ page }) => {
  await page.route("**/api/cluster", (r) => (r.request().method() === "GET" ? r.fulfill({ json: snapshot }) : r.continue()));
  await page.route("**/api/mostwanted", (r) => r.fulfill({ json: { ranks: {}, fetched_at: now, error: null } }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.getByRole("button", { name: /^Layout/ }).click();
  await page.getByRole("menu").getByLabel("Needed now").check();
  await page.getByRole("button", { name: /^Layout/ }).click();
  const rows = page.locator(".needed-row:not(.head)");
  // Something new first (newest first among them), then the unconfirmed slot.
  await expect(rows.locator(".call")).toHaveText(["UA9XX", "IT9ABC", "JA1XYZ"]);
  await expect(rows.nth(0)).toContainText("New zone 18");
  await expect(rows.nth(1)).toContainText("Marathon country");
  await expect(rows.nth(2)).toContainText("Unconfirmed DXCC 20m");
  await page.screenshot({ path: "e2e-results/needed-goals.png" });
});

test("the DX Marathon spot flag switches on and off in Settings", async ({ page }) => {
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.locator(".topbar").getByRole("button", { name: "Menu" }).click();
  await page.locator(".topbar").getByRole("menuitem", { name: "Settings…" }).click();
  await page.getByRole("button", { name: "Awards", exact: true }).click();
  const marathon = page.getByRole("checkbox", { name: /flag countries and CQ zones/ });
  await expect(marathon).not.toBeChecked();
  await expect(page.getByRole("checkbox", { name: "DXCC", exact: true })).toBeChecked();
  const saved = page.waitForResponse((r) => r.url().endsWith("/api/award-goals") && r.request().method() === "PUT");
  await marathon.check();
  expect((await (await saved).json()).marathon).toBe(true);
  await page.getByText("Spot flags", { exact: false }).first().scrollIntoViewIfNeeded();
  await page.screenshot({ path: "e2e-results/settings-spot-flags.png" });
  const off = page.waitForResponse((r) => r.url().endsWith("/api/award-goals") && r.request().method() === "PUT");
  await marathon.uncheck();
  expect((await (await off).json()).marathon).toBe(false);
});
