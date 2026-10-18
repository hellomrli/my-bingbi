import { describe, expect, it } from "vitest";
import { composeWizardMaterial, createWizardDraft, isWizardReady, recipientRequired, validateStep } from "./wizardModel";

describe("draft wizard model", () => {
  it("requires a subject and, for request-like types, a recipient", () => {
    const draft = createWizardDraft("request");
    expect(validateStep("subject", draft)).toContain("发文事由");

    draft.subject = "申请防汛设备经费";
    expect(validateStep("subject", draft)).toContain("主送机关");

    draft.recipient = "×××人民政府";
    expect(validateStep("subject", draft)).toBeNull();
    expect(isWizardReady(draft)).toBe(true);
  });

  it("treats the recipient as optional for types that do not address one", () => {
    const draft = createWizardDraft("minutes");
    draft.subject = "汛前检查部署";
    expect(recipientRequired("minutes")).toBe(false);
    expect(validateStep("subject", draft)).toBeNull();
  });

  it("composes a brief with an explicit no-fabrication instruction", () => {
    const draft = {
      ...createWizardDraft("notice"),
      subject: "开展汛前安全检查",
      recipient: "各乡镇人民政府",
      facts: "5 月 20 日前完成自查\n重点检查低洼地段",
      deadline: "2026 年 5 月 20 日",
      requirements: "报送电子版即可",
      references: "《防汛条例》第十条",
    };

    const material = composeWizardMaterial(draft);

    expect(material.startsWith("请根据以下要素起草公文")).toBe(true);
    expect(material).toContain("不要编造");
    expect(material).toContain("【文种】通知");
    expect(material).toContain("【事由】开展汛前安全检查");
    expect(material).toContain("【主送机关】各乡镇人民政府");
    expect(material).toContain("- 5 月 20 日前完成自查");
    expect(material).toContain("- 重点检查低洼地段");
    expect(material).toContain("【完成时限】2026 年 5 月 20 日");
    expect(material).toContain("【其他要求】报送电子版即可");
    expect(material).toContain("【参考材料】");
    expect(material).toContain("《防汛条例》第十条");
  });

  it("omits empty sections instead of inventing placeholders", () => {
    const draft = { ...createWizardDraft("report"), subject: "上半年工作情况" };
    const material = composeWizardMaterial(draft);

    expect(material).toContain("【文种】报告");
    expect(material).not.toContain("【主送机关】");
    expect(material).not.toContain("【关键事实】");
    expect(material).not.toContain("【完成时限】");
    expect(material).not.toContain("【参考材料】");
  });

  it("ignores blank lines inside the facts and references blocks", () => {
    const draft = {
      ...createWizardDraft("notice"),
      subject: "测试",
      recipient: "×××单位",
      facts: "第一项\n\n   \n第二项",
    };
    const material = composeWizardMaterial(draft);
    expect(material).toContain("- 第一项\n- 第二项");
  });
});
