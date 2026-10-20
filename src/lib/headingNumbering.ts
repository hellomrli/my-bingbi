import type { HeadingLevel } from "./odoc";

const chineseDigits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
export type HeadingCounters = Record<HeadingLevel, number>;

export function createHeadingCounters(): HeadingCounters {
  return { l1: 0, l2: 0, l3: 0, l4: 0 };
}

function chineseNumber(value: number): string {
  if (value > 99) return String(value);
  if (value <= 10) return chineseDigits[value];
  if (value < 20) return `十${chineseDigits[value - 10]}`;
  const tens = Math.floor(value / 10);
  const ones = value % 10;
  return `${chineseDigits[tens]}十${ones ? chineseDigits[ones] : ""}`;
}

function hasNumberPrefix(level: HeadingLevel, content: string): boolean {
  if (level === "l1") return /^[零〇一二三四五六七八九十百千\d]+、/.test(content);
  if (level === "l2") return /^（[零〇一二三四五六七八九十百千\d]+）/.test(content);
  if (level === "l3") return /^\d+\.\s*/.test(content);
  return /^（\d+）/.test(content);
}

/** Mirrors the numbering used by the preview and both native export formats. */
export function formatHeading(content: string, level: HeadingLevel, counters: HeadingCounters): string {
  counters[level] += 1;
  if (level === "l1") counters.l2 = counters.l3 = counters.l4 = 0;
  if (level === "l2") counters.l3 = counters.l4 = 0;
  if (level === "l3") counters.l4 = 0;
  if (hasNumberPrefix(level, content)) return content;

  const number = counters[level];
  const prefix = level === "l1"
    ? `${chineseNumber(number)}、`
    : level === "l2"
      ? `（${chineseNumber(number)}）`
      : level === "l3"
        ? `${number}. `
        : `（${number}）`;
  return `${prefix}${content}`;
}
