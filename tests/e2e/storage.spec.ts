import { expect, test } from "@playwright/test";
import { enterWorkspace, expectFitsViewport, openMenuItem, openSettings } from "./helpers";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(page.getByRole("heading", { name: "今天想写点什么？" })).toBeVisible();
});

test("edits, autosaves, lists, and persists application settings", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await enterWorkspace(page);
  await page.getByRole("tab", { name: "编辑", exact: true }).click();
  const title = page.getByLabel("公文标题");
  await title.fill("关于自动保存闭环测试的请示");
  await expect(page.locator(".save-state")).toContainText("未保存");
  await expect(page.locator(".save-state")).toContainText("已保存", { timeout: 5_000 });

  await openMenuItem(page, "最近文档");
  const library = page.getByRole("dialog", { name: "最近文档" });
  await expect(library).toBeVisible();
  await expect(library).toContainText("关于自动保存闭环测试的请示");
  await expect(library).toContainText("1 个版本");
  await page.getByRole("button", { name: "关闭最近文档" }).click();

  const settings = await openSettings(page);
  await settings.getByRole("button", { name: "暗色" }).click();
  await settings.getByRole("checkbox", { name: /自动保存/ }).uncheck();
  await settings.getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await enterWorkspace(page);
  await expect(page.getByLabel("公文标题")).toHaveValue("关于自动保存闭环测试的请示");
  await openSettings(page);
  await expect(page.getByRole("dialog", { name: "设置" }).getByRole("checkbox", { name: /自动保存/ })).not.toBeChecked();

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

test("uploads and removes individual drafting references", async ({ page }) => {
  const material = page.getByRole("textbox", { name: "写作要求" });
  await material.fill("拟部署汛期隐患排查，月底前反馈。 ");

  await page.getByLabel("上传参考文件").setInputFiles([
    { name: "会议纪要.md", mimeType: "text/markdown", buffer: Buffer.from("会议决定月底前完成整改。") },
    { name: "数据.csv", mimeType: "text/csv", buffer: Buffer.from("事项,数量\n排水泵,3台") },
  ]);

  const references = page.getByLabel("已加入的参考材料");
  await expect(references).toContainText("会议纪要.md");
  await expect(references).toContainText("数据.csv");
  await references.getByRole("button", { name: "移除参考文件 会议纪要.md" }).click();
  await expect(references).not.toContainText("会议纪要.md");
  await expect(references).toContainText("数据.csv");
  await expect(material).toHaveValue("拟部署汛期隐患排查，月底前反馈。 ");
});

test("model provider settings stay within a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const settings = await openSettings(page);
  await settings.getByRole("button", { name: "模型服务" }).click();
  await expect(settings.locator("#provider-picker")).toBeVisible();

  await expectFitsViewport(settings);
});

test("knowledge-base settings stay within a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const settings = await openSettings(page);
  await settings.getByRole("button", { name: "知识库" }).click();
  await expect(settings.locator("#kb-vault")).toBeVisible();
  await settings.getByRole("button", { name: "本地 Vault" }).click();
  await settings.locator("#kb-vault").fill("政务资料库");
  await settings.locator("#kb-local-path").fill("/tmp/obsidian-vault");
  await settings.getByRole("checkbox", { name: /启用知识库/ }).check();
  await settings.getByRole("button", { name: "保存配置" }).click();
  await expect(settings).toContainText("知识库配置已保存");

  await expectFitsViewport(settings);
  const configStorage = await page.evaluate(() => localStorage.getItem("bingbi-browser-kb-config-v1"));
  expect(configStorage).toContain("政务资料库");
  expect(configStorage).not.toContain("token");
});
