import { documentTypeLabels } from "./document";
import type { Fact } from "./odoc";
import type { DocumentType } from "./odoc";

export interface RewritePreset {
  id: string;
  label: string;
  instruction: string;
}

export const REWRITE_PRESETS: readonly RewritePreset[] = [
  {
    id: "specific",
    label: "写具体",
    instruction:
      "把空泛动作改为具体主体、任务、时限和反馈路径；原文没有的信息用“×××”占位，不要编造。",
  },
  {
    id: "concise",
    label: "压缩",
    instruction: "删掉空话、套话和重复表述，保留全部事实与限定条件，压缩到原篇幅的七成左右。",
  },
  {
    id: "formal",
    label: "更规范",
    instruction: "调整为规范公文语体：称谓、动词、关联词和结束语符合公文习惯，不口语化、不文学化。",
  },
  {
    id: "plain",
    label: "去 AI 味",
    instruction:
      "删除空泛排比、假深刻和万能结论，把抽象动作落到具体主体与任务；保留必要的公文惯用语。",
  },
  {
    id: "proofread",
    label: "只校对",
    instruction: "只修正错别字、漏字、标点和明显语病，不改变句式、措辞和事实。",
  },
];

const REWRITE_SYSTEM_PROMPT = [
  "你是中文公文语言编辑。用户会给你一个段落和修改要求，只改写这一段。",
  "纪律：",
  "1. 不得新增或修改数字、日期、文号、金额、单位、人名、机构名和政策依据；原文没有的信息不要编造，可用“×××”占位。",
  "2. 不改变原意和段落功能，不添加标题序号、主送机关、落款、日期。",
  "3. 保持公文语体，删除空泛排比、假深刻和万能结论。",
  "4. 只输出改写后的段落正文，不要解释，不要加引号或代码块。",
].join("\n");

export interface RewritePromptInput {
  docType: DocumentType;
  instruction: string;
  text: string;
  facts: Fact[];
}

/** Builds the chat messages for one-paragraph rewriting. */
export function buildRewriteMessages(input: RewritePromptInput): {
  system: string;
  user: string;
} {
  const facts = input.facts
    .map((fact) => `${fact.value}${fact.unit ?? ""}`)
    .filter((value) => value.trim() !== "");
  return {
    system: REWRITE_SYSTEM_PROMPT,
    user: [
      `文种：${documentTypeLabels[input.docType]}`,
      `修改要求：${input.instruction.trim()}`,
      `已核实事实：${facts.length > 0 ? facts.join("；") : "无"}`,
      "待改写段落：",
      input.text,
    ].join("\n"),
  };
}

/** Removes wrappers models commonly add around a rewritten paragraph. */
export function cleanRewriteOutput(raw: string): string {
  let text = raw.trim();
  const fence = /^```[a-zA-Z]*\n([\s\S]*?)\n```$/.exec(text);
  if (fence) text = fence[1].trim();
  if (text.startsWith("改写后：")) text = text.slice("改写后：".length).trim();
  if (text.length >= 2) {
    const pairs: Array<[string, string]> = [
      ['"', '"'],
      ["“", "”"],
      ["「", "」"],
      ["‘", "’"],
    ];
    for (const [open, close] of pairs) {
      if (text.startsWith(open) && text.endsWith(close)) {
        text = text.slice(open.length, -close.length).trim();
        break;
      }
    }
  }
  return text;
}

const TOKEN_PATTERN =
  /[\u4e00-\u9fffA-Za-z×]{1,20}〔\d{4}〕\d{1,6}号|\d{4}年\d{1,2}月\d{1,2}日|\d+(?:\.\d+)?(?:万元|亿元|元|%|台|个|人|次|天|年|月|日|公里|千米|平方米|立方米)?/g;

function tokenCounts(text: string): Map<string, number> {
  const counts = new Map<string, number>();
  const normalized = text.replace(/\s+/g, "");
  for (const match of normalized.matchAll(TOKEN_PATTERN)) {
    const token = match[0];
    counts.set(token, (counts.get(token) ?? 0) + 1);
  }
  return counts;
}

export interface FactTokenDiff {
  added: string[];
  removed: string[];
}

/**
 * Compares the numeric/date/document-number tokens of the original and the rewrite.
 *
 * Rewriting must not invent a number or silently drop one; the dialog asks the user to confirm
 * when this returns anything, so the check is advisory rather than a hard block.
 */
export function diffFactTokens(original: string, rewritten: string): FactTokenDiff {
  const before = tokenCounts(original);
  const after = tokenCounts(rewritten);
  const added: string[] = [];
  const removed: string[] = [];
  for (const [token, count] of after) {
    const previous = before.get(token) ?? 0;
    for (let index = previous; index < count; index += 1) added.push(token);
  }
  for (const [token, count] of before) {
    const next = after.get(token) ?? 0;
    for (let index = next; index < count; index += 1) removed.push(token);
  }
  return { added, removed };
}
