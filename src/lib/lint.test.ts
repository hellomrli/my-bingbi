import { describe, expect, it } from "vitest";
import { createDemoDocument, createEmptyDocument } from "./document";
import { lintDocument } from "./lint";

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
