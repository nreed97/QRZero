import { expect, test, type Page } from "@playwright/test";

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("Rotator pane Stop button sends a stop request", async ({ page }) => {
  let stops = 0;
  await page.route("**/api/rotator/stop", (r) => {
    stops++;
    return r.fulfill({ json: null });
  });
  // Turn the rotator on (PstRotatorAz over UDP: nothing needs to be listening) and restore it afterwards.
  const headers = { "x-qrzero-token": "e2e" };
  const before = (await (await page.request.get("/api/integrations", { headers })).json()).config;
  await page.request.put("/api/integrations", { headers, data: { ...before, rotator_enabled: true, rotator_kind: "pst" } });
  await page.goto("/?token=e2e");
  await closeWizard(page);
  if (!(await page.getByRole("tab", { name: "Rotator" }).isVisible())) {
    await page.getByRole("button", { name: /^Layout/ }).click();
    await page.getByRole("menu").getByLabel("Rotator").check();
    await page.getByRole("button", { name: /^Layout/ }).click();
  }
  // The map has its own Stop beside the heading while rotor control is on.
  await page.getByRole("tab", { name: "Map", exact: true }).click();
  await page.locator(".map-info").getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(() => stops).toBe(1);
  await page.getByRole("tab", { name: "Rotator" }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(() => stops).toBe(2);
  await page.request.put("/api/integrations", { headers, data: before });
});
