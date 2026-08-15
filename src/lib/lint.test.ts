import { describe, expect, it } from "vitest";
import { createDemoDocument, createEmptyDocument } from "./document";
import { lintDocument, markUnverifiedSegments, unverifiedTokenMap, type Finding } from "./lint";

describe("lintDocument", () => {
  it("reports required fields on an empty request", () => {
    const findings = lintDocument(createEmptyDocument());
    expect(findings.map((finding) => finding.id)).toEqual(expect.arrayContaining(["FMT-001", "FMT-004"]));
  });

  it("accepts a correctly formatted document number", () => {
    const findings = lintDocument(createDemoDocument());
    expect(findings.some((finding) => finding.id === "FMT-013")).toBe(false);
  });

  it("rejects square brackets in a document number", () => {
    const document = createDemoDocument();
    document.header!.documentNumber = "云应急[2026]27号";
    expect(lintDocument(document).some((finding) => finding.id === "FMT-013")).toBe(true);
  });

  it("detects vague actors", () => {
    const document = createDemoDocument();
    document.body.blocks[0] = { type: "paragraph", id: "vague", content: "请相关部门抓好落实。" };
    expect(lintDocument(document).some((finding) => finding.id.startsWith("LANG-006"))).toBe(true);
  });
});

describe("unverifiedTokenMap", () => {
  it("groups fact-check tokens by node id and drops empty tokens", () => {
    const findings: Finding[] = [
      { id: "FACT-001", severity: "error", nodeId: "block-1", message: "m", location: "l", token: "15万元" },
      { id: "FACT-001-2", severity: "error", nodeId: "block-1", message: "m", location: "l", token: "2026年9月30日" },
      { id: "FACT-001-3", severity: "error", nodeId: "block-2", message: "m", location: "l", token: "15万元" },
      { id: "FMT-001", severity: "error", nodeId: "block-3", message: "m", location: "l" },
    ];
    expect(unverifiedTokenMap(findings)).toEqual({
      "block-1": ["15万元", "2026年9月30日"],
      "block-2": ["15万元"],
    });
  });
});

describe("markUnverifiedSegments", () => {
  it("marks every occurrence of each unverified token and preserves the rest", () => {
    const segments = markUnverifiedSegments("购置15万元设备，追加预算15万元，于2026年9月30日完成。", [
      "15万元",
      "2026年9月30日",
    ]);
    expect(segments).toEqual([
      { text: "购置", unverified: false },
      { text: "15万元", unverified: true },
      { text: "设备，追加预算", unverified: false },
      { text: "15万元", unverified: true },
      { text: "，于", unverified: false },
      { text: "2026年9月30日", unverified: true },
      { text: "完成。", unverified: false },
    ]);
  });

  it("returns a single plain segment when there is nothing to mark", () => {
    expect(markUnverifiedSegments("无异常数据。", undefined)).toEqual([{ text: "无异常数据。", unverified: false }]);
    expect(markUnverifiedSegments("无异常数据。", [])).toEqual([{ text: "无异常数据。", unverified: false }]);
    expect(markUnverifiedSegments("无异常数据。", ["15万元"])).toEqual([{ text: "无异常数据。", unverified: false }]);
  });

  it("merges overlapping tokens instead of double-marking", () => {
    const segments = markUnverifiedSegments("支付15万元", ["15", "15万元"]);
    expect(segments).toEqual([
      { text: "支付", unverified: false },
      { text: "15万元", unverified: true },
    ]);
  });
});
