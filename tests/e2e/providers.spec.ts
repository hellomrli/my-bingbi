import { expect, test } from "@playwright/test";
import { openSettings } from "./helpers";

test("adds a common provider from the preset picker", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByRole("button", { name: "模型服务" }).click();

  await page.getByLabel("常用服务").selectOption("moonshot");

  await expect(page.getByLabel("名称")).toHaveValue("Kimi（Moonshot）");
  await expect(page.getByLabel("服务地址")).toHaveValue("https://api.moonshot.cn/v1");
  await expect(page.getByLabel("模型", { exact: true })).toHaveValue("moonshot-v1-32k");
  await expect(page.getByText(/Moonshot 控制台创建 API Key/)).toBeVisible();

  await page.getByRole("button", { name: "保存服务" }).click();
  await expect(page.getByText("服务配置已保存")).toBeVisible();

  await page.getByRole("button", { name: "关闭设置" }).click();
  const providerSelect = page.getByLabel("模型服务");
  await expect(providerSelect.locator("option", { hasText: "Kimi（Moonshot）" })).toHaveCount(1);
});

test("keeps the custom-provider path next to the presets", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByRole("button", { name: "模型服务" }).click();

  await page.getByRole("button", { name: "新增模型服务" }).click();
  await expect(page.getByLabel("名称")).toHaveValue("");
  await expect(page.getByLabel("服务地址")).toHaveValue("https://api.example.com");
  await page.getByLabel("常用服务").selectOption("siliconflow");
  await expect(page.getByLabel("名称")).toHaveValue("硅基流动 SiliconFlow");
});

test("exposes the Cline Pass console as its own settings tab", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByRole("button", { name: "Cline Pass" }).click();

  await expect(page.getByText(/一个订阅覆盖 DeepSeek/)).toBeVisible();
  await expect(page.getByText(/桌面内核读取/)).toBeVisible();
});

