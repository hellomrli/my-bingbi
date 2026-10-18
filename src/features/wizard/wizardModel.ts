import { documentTypeLabels } from "../../lib/document";
import type { DocumentType } from "../../lib/odoc";

/** Everything the wizard collects before handing the brief to the drafting pipeline. */
export interface WizardDraft {
  docType: DocumentType;
  /** Scenario template ID from the knowledge pack; empty means the per-type default. */
  templateId: string;
  /** Display name of the selected template, used in the composed brief. */
  templateName: string;
  subject: string;
  recipient: string;
  facts: string;
  deadline: string;
  requirements: string;
  references: string;
}

export type WizardStepId = "type" | "subject" | "facts" | "constraints" | "materials" | "review";

export interface WizardStep {
  id: WizardStepId;
  title: string;
  hint: string;
}

export const WIZARD_STEPS: readonly WizardStep[] = [
  { id: "type", title: "选择文种", hint: "先定文种，后面的结构、称谓和结尾都跟着它走。" },
  { id: "subject", title: "事由与主送", hint: "一句话说清为什么发文、发给谁。" },
  { id: "facts", title: "关键事实", hint: "数字、日期、金额、文号、责任单位，一行一条；没写的事实模型不会替你编。" },
  { id: "constraints", title: "时限与要求", hint: "完成节点、报送方式、篇幅或口径要求，都可以写在这里。" },
  { id: "materials", title: "补充材料", hint: "可粘贴政策依据、领导批示或参考段落；知识库引用仍可在左侧单独添加。" },
  { id: "review", title: "确认并起草", hint: "核对后交给模型起草；不满意可以随时返回修改。" },
];

/** Types whose recipient is mandatory under GB/T 9704 practice and the built-in lint rules. */
const RECIPIENT_REQUIRED: readonly DocumentType[] = ["request", "notice", "report", "letter"];

export function createWizardDraft(docType: DocumentType): WizardDraft {
  return {
    docType,
    templateId: "",
    templateName: "",
    subject: "",
    recipient: "",
    facts: "",
    deadline: "",
    requirements: "",
    references: "",
  };
}

export function recipientRequired(docType: DocumentType): boolean {
  return RECIPIENT_REQUIRED.includes(docType);
}

/** Returns the first blocking problem for a step, or null when the step may advance. */
export function validateStep(step: WizardStepId, draft: WizardDraft): string | null {
  if (step === "subject") {
    if (!draft.subject.trim()) return "请填写发文事由，一句话说明为什么发文。";
    if (recipientRequired(draft.docType) && !draft.recipient.trim()) {
      return `${documentTypeLabels[draft.docType]}必须填写主送机关。`;
    }
  }
  return null;
}

/** Whether the wizard collected enough to start a draft. */
export function isWizardReady(draft: WizardDraft): boolean {
  return validateStep("subject", draft) === null;
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

/**
 * Turns the collected answers into the material the drafting pipeline expects.
 *
 * The first line is an explicit anti-fabrication instruction: the product promise is that only
 * supplied facts survive into the document, and the wizard is the flow most likely to be used by
 * someone who has not written a brief before.
 */
export function composeWizardMaterial(draft: WizardDraft): string {
  const sections: string[] = [
    "请根据以下要素起草公文。未提供的数字、日期、文号和政策依据不要编造；缺失的信息请留待人工补充。",
    "",
    `【文种】${documentTypeLabels[draft.docType]}`,
    `【事由】${draft.subject.trim()}`,
  ];

  if (draft.recipient.trim()) sections.push(`【主送机关】${draft.recipient.trim()}`);
  if (draft.templateName.trim()) sections.push(`【场景模板】${draft.templateName.trim()}`);

  const facts = lines(draft.facts);
  if (facts.length > 0) {
    sections.push("【关键事实】", ...facts.map((fact) => `- ${fact}`));
  }

  if (draft.deadline.trim()) sections.push(`【完成时限】${draft.deadline.trim()}`);
  if (draft.requirements.trim()) sections.push(`【其他要求】${draft.requirements.trim()}`);

  if (draft.references.trim()) {
    sections.push("【参考材料】", draft.references.trim());
  }

  return sections.join("\n");
}
