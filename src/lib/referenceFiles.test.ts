import { describe, expect, it } from "vitest";
import {
  decodeUploadText,
  fileExtension,
  stripMarkdownFrontmatter,
  textByteLength,
  titleFromFileName,
  uploadAccept,
} from "./referenceFiles";

describe("knowledge-base upload helpers", () => {
  it("derives a safe title and extension from a filename", () => {
    expect(fileExtension("2026年防汛工作要点.DOCX")).toBe("docx");
    expect(fileExtension("no-extension")).toBe("");
    expect(titleFromFileName("2026年防汛工作要点.docx")).toBe("2026年防汛工作要点");
    expect(titleFromFileName(".md")).toBe("未命名资料");
    expect(titleFromFileName("x".repeat(200))).toHaveLength(120);
  });

  it("decodes UTF-8 and UTF-16 text uploads", () => {
    const utf8 = new TextEncoder().encode("汛前检查");
    expect(decodeUploadText(utf8.buffer)).toBe("汛前检查");

    const utf16 = new Uint8Array([0xff, 0xfe, 0x5b, 0x6c, 0x4d, 0x52]);
    expect(decodeUploadText(utf16.buffer)).toBe("汛前");
  });

  it("removes an existing frontmatter block from uploaded Markdown", () => {
    expect(stripMarkdownFrontmatter("---\ntitle: 旧标题\n---\n\n正文")).toBe("正文");
    expect(stripMarkdownFrontmatter("正文\n---\n不是开头")).toBe("正文\n---\n不是开头");
  });

  it("builds an accept list with both text and binary formats", () => {
    const accept = uploadAccept();
    expect(accept).toContain(".md");
    expect(accept).toContain(".docx");
    expect(accept).toContain(".pdf");
    expect(textByteLength("防汛")).toBe(6);
  });
});
