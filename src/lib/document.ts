import type { Block, DocumentType, Fact, ODoc } from "./odoc";
import { createId } from "./id";

export const documentTypeLabels: Record<DocumentType, string> = {
  request: "请示",
  notice: "通知",
  report: "报告",
  letter: "函",
  minutes: "纪要",
  summary: "总结",
  decision: "决定",
  announcement: "公告",
  regulation: "公报",
  proclamation: "通告",
  reply: "批复",
  instruction: "命令",
  proposal: "议案",
  work_plan: "工作方案",
  work_report: "工作汇报",
  briefing: "简报",
  speech: "讲话稿",
  presentation: "发言稿",
  memo: "备忘录",
  invitation: "邀请函",
  certificate: "证明",
  other: "其他",
};

export const securityLabels = {
  public: "公开",
  internal: "内部",
  secret: "秘密",
  confidential: "机密",
} as const;

export function createEmptyDocument(): ODoc {
  return {
    schemaVersion: "0.1.0",
    id: createId("doc"),
    revision: 0,
    meta: {
      docType: "request",
      customDocType: null,
      securityLevel: "internal",
      urgency: "normal",
      copyNumber: null,
      status: "draft",
    },
    header: {
      id: createId("header"),
      issuingAuthorityMark: "",
      documentNumber: null,
      signer: null,
    },
    body: {
      id: createId("body"),
      title: { id: createId("title"), text: "" },
      mainRecipients: [],
      blocks: [{ type: "paragraph", id: createId("block"), content: "" }],
      attachments: [],
      signer: null,
      date: null,
      note: null,
    },
    footer: null,
    facts: { facts: [] },
    provenance: [],
  };
}

/**
 * 演示范文模板。
 *
 * 为保持中立、避免被误认为真实单位或个人，正文中的机关、人名一律以
 * 占位符“×××”表示，不出现任何具体单位名与人名。用户可直接在此基础上
 * 替换占位符形成自己的公文。
 */
export function createDemoDocument(): ODoc {
  const document = createEmptyDocument();
  document.id = "demo-request-2026";
  document.header = {
    id: "header-demo",
    issuingAuthorityMark: "×××县×××局文件",
    documentNumber: "×××〔2026〕27号",
    signer: null,
  };
  document.body.title.text = "关于申请购置应急排水设备经费的请示";
  document.body.mainRecipients = ["×××人民政府"];
  document.body.blocks = [
    {
      type: "paragraph",
      id: "block-intro",
      content: "今年入汛以来，×××县强降雨增多，城区低洼地段出现内涝，现有排水装备难以满足抢险需要。",
    },
    { type: "heading", id: "block-h1-current", level: "l1", content: "现有装备情况" },
    {
      type: "heading",
      id: "block-h2-loss",
      level: "l2",
      content: "设备配置与损耗。×××局现有移动排水泵车2台，其中1台购置于20××年，今年汛期已3次故障停机。",
    },
    { type: "heading", id: "block-h1-request", level: "l1", content: "请示事项" },
    {
      type: "paragraph",
      id: "block-request",
      content: "拟购置大流量排水抢险车3台，所需经费15万元，从×××专项经费列支，于2026年9月30日前完成采购。",
    },
    { type: "paragraph", id: "block-closing", content: "妥否，请批示。" },
  ];
  document.body.signer = {
    id: "signer-demo",
    authority: "×××县×××局",
    representative: null,
  };
  document.body.date = { id: "date-demo", value: "2026年8月12日" };
  document.footer = {
    id: "footer-demo",
    copyRecipients: ["×××局", "×××局"],
    printingAuthority: "×××县×××局办公室",
    printingDate: "2026年8月12日",
  };
  document.facts.facts = [
    demoFact("data", "15", "万元"),
    demoFact("data", "3", "台"),
    demoFact("time", "2026年9月30日", null),
    { ...demoFact("data", "2", "台"), source: "kb_retrieved", sourceRef: "公文/2024/防汛装备请示.md" },
  ];
  return document;
}

function demoFact(kind: Fact["kind"], value: string, unit: string | null): Fact {
  return {
    id: createId("fact"),
    kind,
    value,
    unit,
    role: null,
    source: "user_input",
    sourceRef: null,
    verified: true,
  };
}

export function blockText(block: Block): string {
  if (block.type === "list") return block.items.map((item) => item.content).join("\n");
  if (block.type === "table") {
    return block.rows.flatMap((row) => row.cells.map((cell) => cell.content)).join("\n");
  }
  return block.content;
}

export function documentPlainText(document: ODoc): string {
  return [
    document.header?.issuingAuthorityMark,
    document.header?.documentNumber,
    document.body.title.text,
    ...document.body.mainRecipients,
    ...document.body.blocks.map(blockText),
    document.body.signer?.authority,
    document.body.date?.value,
  ]
    .filter(Boolean)
    .join("\n");
}

export function countChineseCharacters(document: ODoc): number {
  return (documentPlainText(document).match(/[\u3400-\u9fff\w]/g) ?? []).length;
}
