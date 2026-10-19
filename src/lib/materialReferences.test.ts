import { describe, expect, it } from "vitest";
import {
  appendMaterialReference,
  composeMaterialWithReferences,
  materialByteLength,
  MAX_DRAFT_MATERIAL_BYTES,
  parseMaterialReferences,
  referenceContentBytes,
  removeMaterialReference,
  safeReferenceLabel,
  type DraftReference,
} from "./materialReferences";

describe("material references", () => {
  it("appends one quoted source with readable markers", () => {
    const result = appendMaterialReference("给各乡镇发通知", "上传文件", "检查提纲.txt", "一、值班\n二、物资");

    expect(result.error).toBeNull();
    expect(result.material).toContain("给各乡镇发通知");
    expect(result.material).toContain("【参考来源｜上传文件：检查提纲.txt】");
    expect(result.material).toContain("【参考来源结束】");

    const references = parseMaterialReferences(result.material);
    expect(references).toHaveLength(1);
    expect(references[0]).toMatchObject({ kind: "上传文件", label: "检查提纲.txt" });
  });

  it("removes a source without touching the user's own brief", () => {
    const appended = appendMaterialReference("原始要求", "上传文件", "a.txt", "参考内容").material;
    const reference = parseMaterialReferences(appended)[0];

    expect(removeMaterialReference(appended, reference)).toBe("原始要求");
  });

  it("rejects duplicate and oversized additions instead of breaking the draft", () => {
    const first = appendMaterialReference("", "上传文件", "a.txt", "内容").material;
    const duplicate = appendMaterialReference(first, "上传文件", "a.txt", "其他内容");
    expect(duplicate.error).toContain("已经加入");

    const huge = "字".repeat(MAX_DRAFT_MATERIAL_BYTES);
    const oversized = appendMaterialReference("", "上传文件", "huge.txt", huge);
    expect(oversized.error).toContain("2 MiB");
  });

  it("sanitizes labels and counts UTF-8 bytes", () => {
    expect(safeReferenceLabel("  \u0000文件【一】  ")).toBe("文件［一］");
    expect(materialByteLength("公文")).toBe(6);
  });
});

describe("draft references", () => {
  const references: DraftReference[] = [
    { id: "ref-1", kind: "上传文件", label: "检查提纲.txt", content: "一、值班\n二、物资" },
    { id: "ref-2", kind: "知识库", label: "去年通知｜公文/2025/通知.md", content: "历史通知正文" },
  ];

  it("composes reference content behind the brief without touching the visible text", () => {
    const brief = "给各乡镇发防汛检查通知。";
    const composed = composeMaterialWithReferences(brief, references);

    expect(composed.error).toBeNull();
    expect(composed.material.startsWith(brief)).toBe(true);
    expect(composed.material).toContain("【参考来源｜上传文件：检查提纲.txt】");
    expect(composed.material).toContain("【参考来源｜知识库：去年通知｜公文/2025/通知.md】");
    expect(parseMaterialReferences(composed.material)).toHaveLength(2);
    expect(referenceContentBytes(references)).toBe(materialByteLength(references[0].content) + materialByteLength(references[1].content));
  });

  it("rejects duplicate labels instead of silently duplicating content", () => {
    const composed = composeMaterialWithReferences("写通知", [references[0], references[0]]);
    expect(composed.error).toContain("已经加入");
  });
});
