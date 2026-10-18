import { expect, test } from "@playwright/test";

test("wizard collects a brief step by step and previews the composed material", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "向导模式", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "向导模式" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/第 1 \/ 6 步/)).toBeVisible();

  const next = dialog.getByRole("button", { name: "下一步" });
  await next.click();
  await dialog.getByLabel(/发文事由/).fill("申请购置应急排水设备经费");
  await dialog.getByLabel(/主送机关/).fill("×××人民政府");
  await next.click();

  await dialog.getByLabel(/关键事实/).fill("拟购置 3 台\n经费 15 万元");
  await next.click();
  await dialog.getByLabel(/完成时限/).fill("2026 年 9 月 30 日");
  await next.click();
  await dialog.getByLabel(/补充材料/).fill("《防汛条例》第十条");
  await next.click();

  const preview = dialog.getByLabel("起草材料预览");
  await expect(preview).toBeVisible();
  await expect(preview).toContainText("【文种】请示");
  await expect(preview).toContainText("【事由】申请购置应急排水设备经费");
  await expect(preview).toContainText("【主送机关】×××人民政府");
  await expect(preview).toContainText("《防汛条例》第十条");

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("wizard blocks an empty subject instead of composing an unusable brief", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "向导模式", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "向导模式" });
  await dialog.getByRole("button", { name: "下一步" }).click();
  await dialog.getByRole("button", { name: "下一步" }).click();

  await expect(dialog.getByRole("alert")).toContainText("发文事由");
});
