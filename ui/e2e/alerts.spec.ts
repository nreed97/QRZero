import { expect, test, type Page } from "@playwright/test";

async function closeWizard(page: Page) {
  const wizard = page.getByRole("dialog", { name: "QRZero setup" });
  if (await wizard.waitFor({ timeout: 3000 }).then(() => true, () => false)) await wizard.getByRole("button", { name: "Close", exact: true }).click();
}

test("ntfy alerts are set up in Settings, Alerts", async ({ page }) => {
  await page.goto("/?token=e2e");
  await closeWizard(page);
  await page.locator(".topbar").getByRole("button", { name: "Menu" }).click();
  await page.locator(".topbar").getByRole("menuitem", { name: "Settings…" }).click();
  await page.getByRole("button", { name: "Alerts", exact: true }).click();
  const on = page.getByRole("checkbox", { name: "Send alerts through ntfy" });
  await expect(on).not.toBeChecked();
  // Switching on makes up a topic nobody will guess.
  await on.check();
  const topic = page.getByPlaceholder("qrzero-...");
  await expect(topic).toHaveValue(/^qrzero-[a-z0-9]{12}$/);
  const saved = page.waitForResponse((r) => r.url().endsWith("/api/alerts") && r.request().method() === "PUT");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const view = await (await saved).json();
  expect(view.config.enabled).toBe(true);
  expect(view.config.topic).toMatch(/^qrzero-/);
  await expect(page.getByText("Saved.")).toBeVisible();
  await page.screenshot({ path: "e2e-results/settings-alerts.png" });
  // Off again, so nothing in other tests tries to reach ntfy.sh.
  await on.uncheck();
  const off = page.waitForResponse((r) => r.url().endsWith("/api/alerts") && r.request().method() === "PUT");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  expect((await (await off).json()).config.enabled).toBe(false);
});
