import { describe, expect, it } from "vitest";
import {
  appendMaterialReference,
  materialByteLength,
  MAX_DRAFT_MATERIAL_BYTES,
  parseMaterialReferences,
  removeMaterialReference,
  safeReferenceLabel,
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
