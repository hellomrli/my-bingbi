import { describe, expect, it } from "vitest";
import type { FactCheckReport, LintReport } from "./generated/lint";
import type { Finding } from "./lint";
import { selectFindings } from "./report";

const documentId = "doc-1";
const otherDocumentId = "doc-2";

const localFindings: Finding[] = [
  { id: "FMT-001", severity: "error", nodeId: "title", message: "公文标题不能为空。", location: "主体 · 标题" },
];

const engineFinding = {
  id: "PACK-001",
  severity: "warn" as const,
  nodeId: "block-1",
  message: "自定义规则命中。",
  location: "正文 · 第 1 段",
  suggestion: "改写",
  token: null,
};

function lintReport(findings: LintReport["findings"]): LintReport {
  return { findings, errorCount: 0, warnCount: 0, infoCount: 0 };
}

function factReport(findings: FactCheckReport["findings"]): FactCheckReport {
  return { findings, checkedTokens: 1, unresolvedTokens: 1 };
}

describe("selectFindings", () => {
  it("falls back to the local rules when no kernel report matches the revision", () => {
    expect(selectFindings(documentId, 7, localFindings, null, null)).toBe(localFindings);
    expect(
      selectFindings(documentId, 7, localFindings, {
        documentId,
        revision: 6,
        report: lintReport([engineFinding]),
      }, null),
    ).toBe(localFindings);
  });

  it("never reuses a report from another document that happens to share its revision", () => {
    const result = selectFindings(documentId, 0, localFindings, {
      documentId: otherDocumentId,
      revision: 0,
      report: lintReport([engineFinding]),
    }, null);
    expect(result).toBe(localFindings);
  });

  it("prefers the kernel report for the current revision, including custom pack rules", () => {
    const result = selectFindings(documentId, 7, localFindings, {
      documentId,
      revision: 7,
      report: lintReport([engineFinding]),
    }, null);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ id: "PACK-001", suggestion: "改写" });
    expect(result[0]).not.toHaveProperty("token");
  });

  it("treats an empty kernel report as authoritative", () => {
    expect(
      selectFindings(documentId, 7, localFindings, { documentId, revision: 7, report: lintReport([]) }, null),
    ).toEqual([]);
  });

  it("does not mix a current lint report with a stale fact check", () => {
    const staleFact = {
      documentId,
      revision: 6,
      report: factReport([{ ...engineFinding, id: "FCT-001", token: "15万元" }]),
    };
    const result = selectFindings(
      documentId,
      7,
      localFindings,
      { documentId, revision: 7, report: lintReport([engineFinding]) },
      staleFact,
    );
    expect(result.map((finding) => finding.id)).toEqual(["PACK-001"]);
  });

  it("keeps fact-check tokens for red marking when they match the revision", () => {
    const fact = {
      documentId,
      revision: 7,
      report: factReport([{ ...engineFinding, id: "FCT-001", severity: "error" as const, token: "15万元" }]),
    };
    const result = selectFindings(documentId, 7, localFindings, null, fact);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ id: "FCT-001", token: "15万元" });
  });
});
