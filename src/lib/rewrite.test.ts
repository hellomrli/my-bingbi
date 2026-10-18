import { describe, expect, it } from "vitest";
import { createDemoDocument } from "./document";
import { buildRewriteMessages, cleanRewriteOutput, diffFactTokens } from "./rewrite";

describe("rewrite helpers", () => {
  it("builds a prompt that carries the instruction and the verified facts", () => {
    const document = createDemoDocument();
    const { system, user } = buildRewriteMessages({
      docType: document.meta.docType,
      instruction: "  写具体  ",
      text: "拟购置排水设备。",
      facts: document.facts.facts,
    });

    expect(system).toContain("不得新增或修改数字");
    expect(system).toContain("只输出改写后的段落正文");
    expect(user).toContain("文种：请示");
    expect(user).toContain("修改要求：写具体");
    expect(user).toContain("15万元");
    expect(user).toContain("待改写段落：\n拟购置排水设备。");
  });

  it("strips code fences, labels and surrounding quotes", () => {
    expect(cleanRewriteOutput("```text\n改写后：拟购置设备 3 台。\n```")).toBe("拟购置设备 3 台。");
    expect(cleanRewriteOutput("“拟购置设备。”")).toBe("拟购置设备。");
    expect(cleanRewriteOutput("  「正文」  ")).toBe("正文");
  });

  it("flags added and removed numbers, dates and document numbers", () => {
    const diff = diffFactTokens(
      "拟购置设备 3 台，经费 15 万元，于 2026 年 9 月 30 日前完成，×××〔2026〕27号。",
      "拟购置设备 4 台，经费 15 万元，于 2026 年 9 月 30 日前完成。",
    );
    expect(diff.added).toContain("4台");
    expect(diff.removed).toContain("3台");
    expect(diff.removed).toContain("×××〔2026〕27号");
  });

  it("ignores whitespace differences and reports nothing for a clean rewrite", () => {
    expect(diffFactTokens("经费 15 万元", "经费15万元")).toEqual({ added: [], removed: [] });
  });
});
