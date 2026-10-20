import { expect, test } from "@playwright/test";
import { enterWorkspace } from "./helpers";

test("blocked export opens quality checks and facts can be confirmed and corrected", async ({ page }) => {
  await page.goto("/");
  await enterWorkspace(page);
  await page.getByLabel("公文标题").fill("");
  await page.getByRole("button", { name: "导出", exact: true }).click();
  const drawer = page.getByRole("dialog", { name: "质检结果" });
  await expect(drawer).toBeVisible();
  await expect(drawer).toContainText("公文标题不能为空");

  const add = drawer.getByRole("form", { name: "补充事实" });
  await add.getByLabel("事实值", { exact: true }).fill("18");
  await add.getByLabel("单位", { exact: true }).fill("万元");
  await add.getByLabel("核实依据 / 来源").fill("预算批复附件");
  await add.getByRole("checkbox", { name: "已对照材料人工核实" }).check();
  await add.getByRole("button", { name: "加入事实台账" }).click();

  const fact = drawer.getByRole("group", { name: "18万元 · 已核实", exact: true });
  await expect(fact.getByRole("checkbox")).toBeChecked();
  await expect(fact.getByLabel("核实依据 / 来源")).toHaveValue("预算批复附件");
  await fact.getByLabel("事实值", { exact: true }).fill("19");
  const corrected = drawer.getByRole("group", { name: "19万元 · 待核实", exact: true });
  await expect(corrected.getByRole("checkbox")).not.toBeChecked();
  await corrected.getByRole("checkbox").check();
  await drawer.getByRole("button", { name: "移除事实 19万元" }).click();
  await expect(drawer.getByRole("group", { name: /19万元/ })).toHaveCount(0);
  await drawer.getByRole("button", { name: "返回正文" }).click();
  await expect(drawer).toBeHidden();
});
