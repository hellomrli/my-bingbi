import { expect, test } from "@playwright/test";
import { enterWorkspace, expectFitsViewport, expectNoHorizontalOverflow, openMenuItem, openSettings } from "./helpers";

const viewports = [
  { name: "1366x768", width: 1366, height: 768 },
  { name: "1280x720", width: 1280, height: 720 },
  { name: "1024x768", width: 1024, height: 768 },
  { name: "980x680", width: 980, height: 680 },
  { name: "900x650", width: 900, height: 650 },
  { name: "680x900", width: 680, height: 900 },
  { name: "390x844", width: 390, height: 844 },
];

for (const viewport of viewports) {
  test.describe(`viewport ${viewport.name}`, () => {
    test.use({ viewport });

    test("writing desk, workspace and quality drawer stay within the viewport", async ({ page }) => {
      await page.goto("/");
      await expect(page.getByRole("heading", { name: "今天想写点什么？" })).toBeVisible();
      await expect(page.getByLabel("写作要求")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await expectFitsViewport(page.locator(".app-statusbar"));

      await enterWorkspace(page);
      await expect(page.getByRole("article", { name: "A4 公文版式预览" })).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await expectFitsViewport(page.locator(".app-statusbar"));

      await page.getByRole("tab", { name: "编辑", exact: true }).click();
      await expect(page.locator(".structure-editor")).toBeVisible();
      await expectNoHorizontalOverflow(page);

      await page.getByRole("button", { name: /^质检/ }).click();
      const drawer = page.getByRole("dialog", { name: "质检结果" });
      await expectFitsViewport(drawer);
      await expect(drawer.getByRole("heading", { name: /事实台账/ })).toBeVisible();
      await drawer.getByRole("button", { name: "返回正文" }).click();
      await expect(drawer).toBeHidden();
    });

    test("documents dialog fits", async ({ page }) => {
      await page.goto("/");
      await openMenuItem(page, "最近文档");
      await expectFitsViewport(page.getByRole("dialog", { name: "最近文档" }));
      await expectNoHorizontalOverflow(page);
    });

    test("settings dialog fits and is scrollable", async ({ page }) => {
      await page.goto("/");
      const dialog = await openSettings(page);
      await expectFitsViewport(dialog);
      await expectNoHorizontalOverflow(page);
    });
  });
}

test.describe("dialog keyboard behavior", () => {
  test("Escape closes the documents dialog", async ({ page }) => {
    await page.goto("/");
    await openMenuItem(page, "最近文档");
    const dialog = page.getByRole("dialog", { name: "最近文档" });
    await expect(dialog).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("Tab stays inside the settings dialog and Escape closes it", async ({ page }) => {
    await page.goto("/");
    const dialog = await openSettings(page);

    for (let index = 0; index < 30; index += 1) {
      await page.keyboard.press("Tab");
      const focusInside = await dialog.evaluate((element) => element.contains(document.activeElement));
      expect(focusInside, `第 ${index + 1} 次 Tab 后焦点跑出了对话框`).toBe(true);
    }

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });
});
