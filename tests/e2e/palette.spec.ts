import { expect, test } from "@playwright/test";

test("Ctrl+K opens the command palette and runs a searched action", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");

  const dialog = page.getByRole("dialog", { name: "命令面板" });
  await expect(dialog).toBeVisible();
  const search = page.getByLabel("搜索命令");
  await search.fill("设置");
  await search.press("Enter");

  await expect(dialog).toBeHidden();
  await expect(page.getByRole("dialog", { name: "设置", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "设置", exact: true })).toBeHidden();
});

test("Escape closes the command palette without running an action", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: "命令面板" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "命令面板" })).toBeHidden();
});
