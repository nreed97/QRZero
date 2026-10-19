import { expect, test, type Page } from "@playwright/test";

/** Right-clicks `selector` with a synthetic event and says whether the page cancelled the browser's menu. */
const blocked = (page: Page, selector: string) =>
  page.evaluate((sel) => {
    const el = document.querySelector(sel)!;
    const e = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    el.dispatchEvent(e);
    return e.defaultPrevented;
  }, selector);

test("the desktop app hides the browser's right-click menu except in text fields", async ({ page }) => {
  await page.goto("/?token=e2e&desktop=framed&os=windows");
  await expect(page.locator(".topbar")).toBeVisible();
  expect(await blocked(page, ".topbar")).toBe(true);
  expect(await blocked(page, "input[type=text], input:not([type])")).toBe(false);
});

test("a browser keeps its own right-click menu", async ({ page }) => {
  await page.goto("/?token=e2e");
  await expect(page.locator(".topbar")).toBeVisible();
  expect(await blocked(page, ".topbar")).toBe(false);
});
