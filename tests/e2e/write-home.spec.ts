import { expect, test } from "@playwright/test";
import { enterWorkspace, openMenuItem } from "./helpers";

test("writing desk accepts a brief and optional document type without exposing a legacy wizard", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "今天想写点什么？" })).toBeVisible();
  await page.getByLabel("写作要求").fill("给各乡镇发一个防汛检查通知，下周检查值班和物资。");
  const types = page.getByRole("group", { name: "文种" });
  await types.getByRole("button", { name: "通知", exact: true }).click();
  await expect(types.getByRole("button", { name: "通知", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(types.getByRole("button", { name: "自动识别" })).toHaveAttribute("aria-pressed", "false");
  await types.getByRole("button", { name: "通知", exact: true }).click();
  await expect(types.getByRole("button", { name: "自动识别" })).toHaveAttribute("aria-pressed", "true");

  // Browser preview must not dispatch a real model request, even with a valid brief.
  await expect(page.getByRole("button", { name: "开始写作" })).toBeDisabled();
  await page.getByLabel("写作要求").press("Control+Enter");
  await expect(page.getByRole("region", { name: "写作输入区" })).toBeVisible();
  await expect(page.getByRole("button", { name: "向导模式", exact: true })).toHaveCount(0);
});

test("new writing keeps the selected security level and only offers local providers", async ({ page }) => {
  await page.goto("/");
  await page.getByLabel("写作密级").selectOption("confidential");
  const providers = page.getByLabel("模型服务", { exact: true });
  await expect(providers.locator("option")).toHaveCount(1);
  await expect(providers).toHaveValue("ollama");
  await expect(page.locator(".app-statusbar")).toContainText("仅本地模型");
  await page.getByLabel("写作要求").fill("机密材料，不得发送云端");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("已保存版本");

  await openMenuItem(page, /新建写作/);
  await expect(page.getByLabel("写作要求")).toHaveValue("");
  await expect(page.getByLabel("写作密级")).toHaveValue("confidential");
  await expect(providers).toHaveValue("ollama");
  await expect(providers.locator("option")).toHaveCount(1);
  await expect(page.getByRole("button", { name: /继续编辑/ })).toHaveCount(0);
});

test("workspace switches between editing and consistently numbered preview", async ({ page }) => {
  await page.goto("/");
  await enterWorkspace(page);
  const preview = page.getByRole("article", { name: "A4 公文版式预览" });
  await expect(preview.locator(".official-heading")).toHaveText([
    "一、现有装备情况",
    "（一）设备配置与损耗。×××局现有移动排水泵车2台，其中1台购置于20××年，今年汛期已3次故障停机。",
    "二、请示事项",
  ]);
  await page.getByRole("tab", { name: "编辑", exact: true }).click();
  await page.locator('[data-node-id="block-h1-current"] textarea').fill("现有防汛装备");
  await page.getByRole("tab", { name: "版式", exact: true }).click();
  await expect(preview.locator(".official-heading").first()).toHaveText("一、现有防汛装备");
  await page.getByRole("button", { name: "回到写作台" }).click();
  await expect(page.getByLabel("写作要求")).toBeVisible();
});
