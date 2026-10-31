import { expect, test, type Page } from "@playwright/test";

const now = Math.floor(Date.now() / 1000);
const entity = { name: "United States", prefix: "K", cont: "NA", dxcc: 291, lat: 0, lon: 0 };
const worked = { new_call: false, new_call_band: false, new_dxcc: false, new_band: false, new_mode: false };
const spot = (id: number, call: string, khz: number, band: string, mode: string, ref: string, park: string, needed: object | null, age: number) => ({
  id, time: now - age, call, freq_hz: khz * 1000, band, mode, reference: ref, park, location: "US-ME", grid: "FN54", spotter: "W1XYZ", comment: "", entity, needed,
});
const list = {
  fetched_at: now,
  error: null,
  spots: [
    spot(3, "N2NEW", 7032, "40m", "CW", "US-0003", "Third Park", { ...worked, new_call: true, new_call_band: true, new_mode: true }, 30),
    spot(2, "K1WRK", 14244, "20m", "SSB", "US-0002", "Second Park", worked, 300),
    spot(1, "W3OLD", 14062, "20m", "CW", "US-0001", "First Park", { ...worked, new_call: true, new_call_band: true }, 900),
  ],
};

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("POTA spots pane lists activators, flags and filters them", async ({ page }) => {
  await page.route("**/api/pota*", (r) => r.fulfill({ json: list }));
  await page.goto("/?token=e2e");
  await closeWizard(page);
  if (!(await page.getByRole("tab", { name: "POTA spots" }).isVisible())) {
    await page.getByRole("button", { name: /^Layout/ }).click();
    await page.getByRole("menu").getByLabel("POTA spots").check();
    await page.getByRole("button", { name: /^Layout/ }).click();
  }
  await page.getByRole("tab", { name: "POTA spots" }).click();
  const pane = page.locator(".cluster", { has: page.getByLabel("POTA spots", { exact: true }).and(page.locator("[role=table]")) });
  const rows = pane.locator(".spot-row:not(.head)");
  await expect(rows).toHaveCount(3);
  // Newest first, flagged and greyed.
  await expect(rows.first()).toContainText("N2NEW");
  await expect(rows.first()).toContainText("New mode");
  await expect(rows.nth(1)).toHaveClass(/worked/);
  await expect(rows.nth(1)).toContainText("Worked");
  await pane.getByLabel("Band").selectOption("20m");
  await expect(rows).toHaveCount(2);
  await pane.getByLabel("Mode").selectOption("cw");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("US-0001");
  await pane.getByLabel("Band").selectOption("");
  await pane.getByLabel("Mode").selectOption("");
  await pane.getByLabel("Hide worked").check();
  await expect(rows).toHaveCount(2);
});
