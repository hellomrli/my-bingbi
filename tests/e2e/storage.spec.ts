import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(page.locator(".official-sheet")).toBeVisible();
});

test("edits, autosaves, lists, and persists application settings", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.getByRole("button", { name: "结构编辑" }).click();
  const title = page.locator(".title-editor textarea");
  await title.fill("关于自动保存闭环测试的请示");
  await expect(page.locator(".statusbar")).toContainText("有未保存修改");
  await expect(page.locator(".statusbar")).toContainText("已保存", { timeout: 5_000 });

  await page.getByRole("button", { name: "最近文档" }).click();
  const library = page.getByRole("dialog", { name: "最近文档" });
  await expect(library).toBeVisible();
  await expect(library).toContainText("关于自动保存闭环测试的请示");
  await expect(library).toContainText("1 个版本");
  await page.getByRole("button", { name: "关闭最近文档" }).click();

  await page.getByRole("button", { name: "设置" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await expect(settings).toBeVisible();
  await settings.getByRole("button", { name: "暗色" }).click();
  await settings.locator(".toggle-row input[type=checkbox]").uncheck();
  await settings.getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("dialog", { name: "设置" }).locator(".toggle-row input[type=checkbox]")).not.toBeChecked();

  const reopenedSettings = page.getByRole("dialog", { name: "设置" });
  await reopenedSettings.getByRole("button", { name: "模型服务" }).click();
  await expect(reopenedSettings.locator("#provider-picker")).toContainText("OpenAI 兼容");
  await reopenedSettings.getByRole("button", { name: "新增模型服务" }).click();
  await reopenedSettings.getByLabel("名称").fill("本地回归模型");
  await reopenedSettings.getByLabel("服务 ID").fill("e2e-local");
  await reopenedSettings.getByRole("button", { name: "Ollama" }).click();
  await reopenedSettings.locator("#provider-url").fill("http://localhost:11434");
  await reopenedSettings.locator("#provider-model").fill("qwen3:8b");
  await reopenedSettings.getByRole("button", { name: "保存服务" }).click();
  await expect(reopenedSettings).toContainText("服务配置已保存");
  await expect(reopenedSettings.locator("#provider-picker")).toContainText("本地回归模型");
  const providerStorage = await page.evaluate(() => localStorage.getItem("bingbi-browser-providers-v1"));
  expect(providerStorage).toContain("e2e-local");
  expect(providerStorage).not.toContain("apiKey");
  expect(consoleErrors).toEqual([]);
});

test("model provider settings stay within a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "设置" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await settings.getByRole("button", { name: "模型服务" }).click();
  await expect(settings.locator("#provider-picker")).toBeVisible();

  const bounds = await settings.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return {
      left: rect.left,
      right: rect.right,
      viewportWidth: window.innerWidth,
      horizontalOverflow: element.scrollWidth - element.clientWidth,
    };
  });
  expect(bounds.left).toBeGreaterThanOrEqual(0);
  expect(bounds.right).toBeLessThanOrEqual(bounds.viewportWidth);
  expect(bounds.horizontalOverflow).toBeLessThanOrEqual(1);
});

test("knowledge-base settings stay within a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "设置" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await settings.getByRole("button", { name: "知识库" }).click();
  await expect(settings.locator("#kb-vault")).toBeVisible();
  await settings.getByRole("button", { name: "本地 Vault" }).click();
  await settings.locator("#kb-vault").fill("政务资料库");
  await settings.locator("#kb-local-path").fill("/tmp/obsidian-vault");
  await settings.getByRole("checkbox", { name: /启用知识库/ }).check();
  await settings.getByRole("button", { name: "保存配置" }).click();
  await expect(settings).toContainText("知识库配置已保存");

  const bounds = await settings.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return {
      left: rect.left,
      right: rect.right,
      viewportWidth: window.innerWidth,
      horizontalOverflow: element.scrollWidth - element.clientWidth,
    };
  });
  expect(bounds.left).toBeGreaterThanOrEqual(0);
  expect(bounds.right).toBeLessThanOrEqual(bounds.viewportWidth);
  expect(bounds.horizontalOverflow).toBeLessThanOrEqual(1);
  const configStorage = await page.evaluate(() => localStorage.getItem("bingbi-browser-kb-config-v1"));
  expect(configStorage).toContain("政务资料库");
  expect(configStorage).not.toContain("token");
});
