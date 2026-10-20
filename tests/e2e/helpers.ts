import { expect, type Locator, type Page } from "@playwright/test";

export async function openMenuItem(page: Page, name: string | RegExp) {
  await page.getByRole("button", { name: "更多", exact: true }).click();
  await page.getByRole("menu", { name: "更多操作" }).getByRole("menuitem", { name }).click();
}

export async function openSettings(page: Page) {
  await openMenuItem(page, "设置");
  const dialog = page.getByRole("dialog", { name: "设置", exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
}

export async function enterWorkspace(page: Page) {
  await page.getByRole("button", { name: /继续编辑/ }).click();
  await expect(page.getByRole("region", { name: "公文编辑区" })).toBeVisible();
}

export async function expectNoHorizontalOverflow(page: Page) {
  const { scrollWidth, clientWidth } = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
  expect(scrollWidth, `页面横向溢出 ${scrollWidth} > ${clientWidth}`).toBeLessThanOrEqual(clientWidth + 1);
}

export async function expectFitsViewport(locator: Locator) {
  await expect(locator).toBeVisible();
  // Drawers animate in from outside the viewport; check their settled geometry.
  await expect(async () => {
    const bounds = await locator.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
        width: window.innerWidth,
        height: window.innerHeight,
        overflow: element.scrollWidth - element.clientWidth,
      };
    });
    expect(bounds.left).toBeGreaterThanOrEqual(-1);
    expect(bounds.right).toBeLessThanOrEqual(bounds.width + 1);
    expect(bounds.top).toBeGreaterThanOrEqual(-1);
    expect(bounds.bottom).toBeLessThanOrEqual(bounds.height + 1);
    expect(bounds.overflow, "内容应在容器内换行而非横向溢出").toBeLessThanOrEqual(1);
  }).toPass({ timeout: 5_000 });
}
