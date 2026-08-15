import type { Block, ODoc } from "./odoc";
import { blockText, documentPlainText, documentTypeLabels } from "./document";

export type Severity = "error" | "warn" | "info";

export interface Finding {
  id: string;
  severity: Severity;
  nodeId: string;
  message: string;
  location: string;
  suggestion?: string;
}

const slogans = ["走深走实", "提质增效", "凝心聚力", "开拓创新", "真抓实干", "持续发力", "全面推动", "不断完善"];
const vagueActorPattern = /(相关部门|有关方面|各有关单位)/;

function blockLocation(block: Block, index: number): string {
  if (block.type === "heading") return `正文 · 第 ${index + 1} 节`;
  return `正文 · 第 ${index + 1} 段`;
}

export function lintDocument(document: ODoc): Finding[] {
  const findings: Finding[] = [];
  const title = document.body.title.text.trim();

  if (!title) {
    findings.push({
      id: "FMT-001",
      severity: "error",
      nodeId: document.body.title.id,
      message: "公文标题不能为空。",
      location: "主体 · 标题",
      suggestion: `补充“发文机关 + 事由 + ${documentTypeLabels[document.meta.docType]}”`,
    });
  }

  const needsRecipient = ["request", "notice", "report", "letter"].includes(document.meta.docType);
  if (needsRecipient && document.body.mainRecipients.every((recipient) => !recipient.trim())) {
    findings.push({
      id: "FMT-004",
      severity: "error",
      nodeId: document.body.id,
      message: `${documentTypeLabels[document.meta.docType]}缺少主送机关。`,
      location: "主体 · 主送机关",
    });
  }

  const documentNumber = document.header?.documentNumber?.trim();
  if (documentNumber && !/^.+〔\d{4}〕\d+号$/.test(documentNumber)) {
    findings.push({
      id: "FMT-013",
      severity: "error",
      nodeId: document.header?.id ?? document.body.id,
      message: "发文字号应使用六角括号〔〕并包含完整年份和序号。",
      location: "版头 · 发文字号",
      suggestion: "例如：云应急〔2026〕27号",
    });
  }

  const date = document.body.date?.value.trim();
  if (date && !/^\d{4}年(?:1[0-2]|[1-9])月(?:3[01]|[12]\d|[1-9])日$/.test(date)) {
    findings.push({
      id: "DATE-001",
      severity: "error",
      nodeId: document.body.date?.id ?? document.body.id,
      message: "成文日期应使用阿拉伯数字全称。",
      location: "落款 · 成文日期",
      suggestion: "例如：2026年8月12日",
    });
  }

  for (const [index, block] of document.body.blocks.entries()) {
    const text = blockText(block);
    if (vagueActorPattern.test(text)) {
      findings.push({
        id: `LANG-006-${index + 1}`,
        severity: "warn",
        nodeId: block.id,
        message: "存在模糊责任主体，请明确到具体单位。",
        location: blockLocation(block, index),
      });
    }
  }

  const plainText = documentPlainText(document);
  const sloganCount = slogans.reduce((count, slogan) => count + plainText.split(slogan).length - 1, 0);
  const limit = Math.max(1, Math.floor(plainText.length / 100));
  if (sloganCount > limit) {
    findings.push({
      id: "LANG-012",
      severity: "warn",
      nodeId: document.body.id,
      message: `抽象口号出现 ${sloganCount} 次，超过当前篇幅建议值 ${limit} 次。`,
      location: "正文 · 全文",
      suggestion: "替换为明确的主体、任务、机制或时限",
    });
  }

  if (document.body.blocks.every((block) => !blockText(block).trim())) {
    findings.push({
      id: "CNT-001",
      severity: "info",
      nodeId: document.body.id,
      message: "正文尚未填写。",
      location: "主体 · 正文",
    });
  }

  return findings;
}
