import { describe, expect, it } from "vitest";
import {
  blockText,
  countChineseCharacters,
  createDemoDocument,
  createEmptyDocument,
  documentPlainText,
} from "./document";
import type { ODoc } from "./odoc";

describe("createEmptyDocument", () => {
  it("starts a blank, editable document with one paragraph", () => {
    const document = createEmptyDocument();
    expect(document.schemaVersion).toBe("0.1.0");
    expect(document.revision).toBe(0);
    expect(document.meta.docType).toBe("request");
    expect(document.body.title.text).toBe("");
    expect(document.body.blocks).toHaveLength(1);
    expect(document.body.blocks[0].type).toBe("paragraph");
    expect(document.facts.facts).toEqual([]);
  });
});

describe("createDemoDocument", () => {
  it("keeps the demo free of real organisations and names", () => {
    const document = createDemoDocument();
    const text = documentPlainText(document);
    expect(text).toContain("×××");
    expect(text).not.toContain("人民政府办公室");
    expect(document.facts.facts.length).toBeGreaterThan(0);
    expect(document.facts.facts.every((fact) => fact.verified)).toBe(true);
  });
});

describe("blockText", () => {
  it("joins list items and table cells so the fact check can see every value", () => {
    expect(
      blockText({ type: "list", id: "l1", style: "ordered", items: [
        { id: "i1", content: "第一项" },
        { id: "i2", content: "第二项" },
      ] }),
    ).toBe("第一项\n第二项");

    expect(
      blockText({
        type: "table",
        id: "t1",
        rows: [
          { id: "r1", cells: [{ id: "c1", content: "项目" }, { id: "c2", content: "金额" }] },
          { id: "r2", cells: [{ id: "c3", content: "设备" }, { id: "c4", content: "15万元" }] },
        ],
      }),
    ).toBe("项目\n金额\n设备\n15万元");
  });

  it("returns the raw content for headings, paragraphs and quotes", () => {
    expect(blockText({ type: "heading", id: "h1", level: "l2", content: "一、总体要求" })).toBe("一、总体要求");
    expect(blockText({ type: "paragraph", id: "p1", content: "正文。" })).toBe("正文。");
    expect(blockText({ type: "quote", id: "q1", content: "引文。" })).toBe("引文。");
  });
});

describe("documentPlainText", () => {
  it("walks header, title, recipients, body, signer and date in reading order", () => {
    const document = createEmptyDocument();
    document.header = {
      id: "header-1",
      issuingAuthorityMark: "×××局文件",
      documentNumber: "×××〔2026〕1号",
      signer: null,
    };
    document.body.title.text = "关于测试的请示";
    document.body.mainRecipients = ["×××政府"];
    document.body.blocks = [{ type: "paragraph", id: "p1", content: "妥否，请批示。" }];
    document.body.signer = { id: "s1", authority: "×××局", representative: null };
    document.body.date = { id: "d1", value: "2026年8月12日" };

    expect(documentPlainText(document).split("\n")).toEqual([
      "×××局文件",
      "×××〔2026〕1号",
      "关于测试的请示",
      "×××政府",
      "妥否，请批示。",
      "×××局",
      "2026年8月12日",
    ]);
  });

  it("skips blank fields instead of emitting empty lines", () => {
    const document = createEmptyDocument();
    document.body.title.text = "标题";
    const lines = documentPlainText(document).split("\n");
    expect(lines).toEqual(["标题"]);
  });
});

describe("countChineseCharacters", () => {
  function documentWith(title: string, content: string): ODoc {
    const document = createEmptyDocument();
    document.body.title.text = title;
    document.body.blocks = [{ type: "paragraph", id: "p1", content }];
    return document;
  }

  it("counts CJK characters one by one and digit runs as one word", () => {
    // "2026" is one run; 年/工/作 are three characters.
    expect(countChineseCharacters(documentWith("2026年工作", ""))).toBe(4);
  });

  it("counts digits inside prose without inflating the total", () => {
    // 共 + "3" + 项 on top of the title above.
    expect(countChineseCharacters(documentWith("2026年工作", "共3项"))).toBe(7);
  });

  it("counts punctuation as zero", () => {
    expect(countChineseCharacters(documentWith("", "。，；："))).toBe(0);
  });
});
