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
  /** The exact inline text span flagged by the fact check, when present. */
  token?: string;
}

export interface MarkedSegment {
  text: string;
  unverified: boolean;
}

/** Groups the fact-check `token` values by the node they were found in. */
export function unverifiedTokenMap(
  findings: Array<{ nodeId: string; token?: string | null }>,
): Record<string, string[]> {
  const map: Record<string, string[]> = {};
  for (const finding of findings) {
    if (!finding.token || finding.token.length === 0) continue;
    const nodeId = finding.nodeId;
    const tokens = map[nodeId] ?? (map[nodeId] = []);
    if (!tokens.includes(finding.token)) tokens.push(finding.token);
  }
  return map;
}

/**
 * Splits `text` into marked/unmarked segments by wrapping every occurrence of the
 * unverified tokens. Token matches are literal substring matches, exactly as the
 * kernel extracted them, so ordering and case are preserved.
 */
export function markUnverifiedSegments(text: string, tokens: string[] | undefined): MarkedSegment[] {
  if (!tokens || tokens.length === 0) return [{ text, unverified: false }];
  const ranges: Array<[number, number]> = [];
  for (const token of tokens) {
    if (token.length === 0) continue;
    let from = 0;
    while (true) {
      const index = text.indexOf(token, from);
      if (index < 0) break;
      ranges.push([index, index + token.length]);
      from = index + token.length;
    }
  }
  if (ranges.length === 0) return [{ text, unverified: false }];
  ranges.sort((left, right) => left[0] - right[0]);
  const merged: Array<[number, number]> = [];
  for (const range of ranges) {
    const previous = merged[merged.length - 1];
    if (previous && range[0] <= previous[1]) {
      previous[1] = Math.max(previous[1], range[1]);
    } else {
      merged.push([...range]);
    }
  }
  const segments: MarkedSegment[] = [];
  let cursor = 0;
  for (const [start, end] of merged) {
    if (start > cursor) segments.push({ text: text.slice(cursor, start), unverified: false });
    segments.push({ text: text.slice(start, end), unverified: true });
    cursor = end;
  }
  if (cursor < text.length) segments.push({ text: text.slice(cursor), unverified: false });
  return segments;
}

const CHINESE_DATE_PATTERN = /^(\d{4})年(1[0-2]|[1-9])月(3[01]|[12]\d|[1-9])日$/;

/**
 * Whether the GB/T 9704 date string names a real calendar day: format per the standard plus
 * month-length and leap-year checks, so "2026年2月29日" or "4月31日" is rejected.
 */
export function isValidChineseDate(value: string): boolean {
  const match = CHINESE_DATE_PATTERN.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  // Day 0 of the next month index is the last day of `month` itself; Date handles leap years.
  return day <= new Date(year, month, 0).getDate();
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
  if (date && !isValidChineseDate(date)) {
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
