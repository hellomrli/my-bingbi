import { describe, expect, it } from "vitest";
import { createHeadingCounters, formatHeading } from "./headingNumbering";

describe("heading numbering", () => {
  it("matches the official numbering scheme and resets subordinate levels", () => {
    const counters = createHeadingCounters();
    expect(formatHeading("总则", "l1", counters)).toBe("一、总则");
    expect(formatHeading("范围", "l2", counters)).toBe("（一）范围");
    expect(formatHeading("目标", "l3", counters)).toBe("1. 目标");
    expect(formatHeading("措施", "l4", counters)).toBe("（1）措施");
    expect(formatHeading("职责", "l2", counters)).toBe("（二）职责");
    expect(formatHeading("落实", "l3", counters)).toBe("1. 落实");
    expect(formatHeading("保障", "l1", counters)).toBe("二、保障");
  });

  it("does not duplicate numbering already present on imported headings", () => {
    const counters = createHeadingCounters();
    expect(formatHeading("一、总则", "l1", counters)).toBe("一、总则");
    expect(formatHeading("（一）范围", "l2", counters)).toBe("（一）范围");
    expect(formatHeading("100、附则", "l1", counters)).toBe("100、附则");
  });
});
