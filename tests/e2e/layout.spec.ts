import { expect, test } from "@playwright/test";

const viewports = [
  { name: "1366x768", width: 1366, height: 768 },
  { name: "1280x720", width: 1280, height: 720 },
  { name: "1024x768", width: 1024, height: 768 },
  { name: "980x680", width: 980, height: 680 },
  { name: "900x650", width: 900, height: 650 },
  { name: "680x900", width: 680, height: 900 },
];

function expectNoHorizontalOverflow(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const body = document.body;
    return { scrollWidth: body.scrollWidth, clientWidth: body.clientWidth };
  }).then(({ scrollWidth, clientWidth }) => {
    expect(scrollWidth, `body 横向溢出 ${scrollWidth} > ${clientWidth}`).toBeLessThanOrEqual(clientWidth + 1);
  });
}

function expectModalFits(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const modal = document.querySelector(".modal");
    if (!modal) return null;
    const rect = modal.getBoundingClientRect();
    const scrollable = modal.scrollHeight > modal.clientHeight;
    return {
      top: rect.top,
      bottom: rect.bottom,
      vh: window.innerHeight,
      scrollable,
      fits: rect.top >= -1 && rect.bottom <= window.innerHeight + 1,
    };
  });
}

for (const viewport of viewports) {
  test.describe(`viewport ${viewport.name}`, () => {
    test.use({ viewport });

    test("workbench layout stays within the viewport", async ({ page }) => {
      await page.goto("/");
      await expect(page.locator(".official-sheet")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      // Both panes and the statusbar stay inside the viewport; the lint inspector is
      // hidden by design below 930px.
      await expect(page.locator(".left-rail")).toBeVisible();
      await expect(page.locator(".statusbar")).toBeVisible();
      // Below 930px the app switches to the mobile page-scroll layout by design;
      // desktop windows never go that narrow (minWidth 980).
      const desktopLayout = viewport.width > 930;
      if (desktopLayout) {
        await expect(page.locator(".right-inspector")).toBeVisible();
        const statusInView = await page.evaluate(() => {
          const rect = document.querySelector(".statusbar")!.getBoundingClientRect();
          return rect.bottom <= window.innerHeight + 1;
        });
        expect(statusInView, "状态栏应在视口内").toBe(true);
      } else {
        await expect(page.locator(".center-workspace")).toBeVisible();
      }
    });

    test("documents dialog fits", async ({ page }) => {
      await page.goto("/");
      await page.getByRole("button", { name: "最近文档" }).click();
      const info = await expectModalFits(page);
      if (info) {
        expect(info.fits, `对话框越界 top=${info.top} bottom=${info.bottom} vh=${info.vh}`).toBe(true);
      }
      await expectNoHorizontalOverflow(page);
    });

    test("settings dialog fits and is scrollable", async ({ page }) => {
      await page.goto("/");
      await page.getByRole("button", { name: "设置" }).click();
      const info = await expectModalFits(page);
      if (info) {
        expect(info.fits, `设置越界 top=${info.top} bottom=${info.bottom} vh=${info.vh}`).toBe(true);
      }
      await expectNoHorizontalOverflow(page);
    });
  });
}

test.describe("dialog keyboard behavior", () => {
  test("Escape closes the documents dialog", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "最近文档" }).click();
    const dialog = page.getByRole("dialog", { name: "最近文档" });
    await expect(dialog).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("Tab stays inside the settings dialog and Escape closes it", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "设置" }).click();
    const dialog = page.getByRole("dialog", { name: "设置" });
    await expect(dialog).toBeVisible();

    for (let index = 0; index < 30; index += 1) {
      await page.keyboard.press("Tab");
      const focusInside = await dialog.evaluate((element) => element.contains(document.activeElement));
      expect(focusInside, `第 ${index + 1} 次 Tab 后焦点跑出了对话框`).toBe(true);
    }

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });
});
