// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import { createEmptyDocument } from "../lib/document";
import { useWorkbenchStore } from "./workbench";

const initialState = useWorkbenchStore.getInitialState();

beforeEach(() => {
  useWorkbenchStore.setState(initialState, true);
});

function state() {
  return useWorkbenchStore.getState();
}

describe("workbench store", () => {
  it("bumps the revision and marks the document dirty on every edit", () => {
    const before = state().document.revision;
    state().updateTitle("关于测试的请示");
    expect(state().document.body.title.text).toBe("关于测试的请示");
    expect(state().document.revision).toBe(before + 1);
    expect(state().dirty).toBe(true);
  });

  it("stores the header as null when the field is cleared", () => {
    state().updateHeader("documentNumber", "×××〔2026〕1号");
    expect(state().document.header?.documentNumber).toBe("×××〔2026〕1号");
    state().updateHeader("documentNumber", "   ");
    expect(state().document.header?.documentNumber).toBeNull();
  });

  it("treats a whitespace-only recipient as no recipient", () => {
    state().updateRecipient("×××人民政府");
    expect(state().document.body.mainRecipients).toEqual(["×××人民政府"]);
    state().updateRecipient("  ");
    expect(state().document.body.mainRecipients).toEqual([]);
  });

  it("adds a heading block and selects it for editing", () => {
    const before = state().document.body.blocks.length;
    state().addBlock("heading");
    const blocks = state().document.body.blocks;
    expect(blocks).toHaveLength(before + 1);
    expect(blocks[blocks.length - 1]).toMatchObject({ type: "heading", level: "l1", content: "" });
    expect(state().selectedNodeId).toBe(blocks[blocks.length - 1].id);
  });

  it("never leaves the body without a paragraph", () => {
    state().setDocument(createEmptyDocument(), null);
    const onlyBlock = state().document.body.blocks[0];
    state().removeBlock(onlyBlock.id);
    expect(state().document.body.blocks).toHaveLength(1);
    expect(state().document.body.blocks[0].type).toBe("paragraph");
    expect(state().document.body.blocks[0].id).not.toBe(onlyBlock.id);
  });

  it("marks model-generated facts as unverified and normalises units", () => {
    state().setDocument(createEmptyDocument(), null);
    state().addFact({ kind: "data", value: "15", unit: "  ", source: "user_input" });
    state().addFact({ kind: "data", value: "3", unit: "台", source: "model_generated" });

    const [userFact, modelFact] = state().document.facts.facts;
    expect(userFact.unit).toBeNull();
    expect(userFact.verified).toBe(true);
    expect(modelFact.unit).toBe("台");
    expect(modelFact.verified).toBe(false);
  });

  it("preserves fact provenance and clears confirmation after content edits", () => {
    state().setDocument(createEmptyDocument(), null);
    state().addFact({
      kind: "data",
      value: "15",
      unit: "万元",
      source: "model_generated",
      sourceRef: "会议纪要第 2 页",
    });
    let fact = state().document.facts.facts[0];
    expect(fact).toMatchObject({ source: "model_generated", sourceRef: "会议纪要第 2 页", verified: false });

    state().updateFact(fact.id, { verified: true });
    expect(state().document.facts.facts[0].verified).toBe(true);
    state().updateFact(fact.id, { value: "18" });
    fact = state().document.facts.facts[0];
    expect(fact.value).toBe("18");
    expect(fact.verified).toBe(false);
    expect(fact.sourceRef).toBe("会议纪要第 2 页");
    state().updateFact(fact.id, { unit: "  ", sourceRef: "  " });
    expect(state().document.facts.facts[0]).toMatchObject({ unit: null, sourceRef: null, verified: false });
  });

  it("opens a document with a clean dirty flag and no stale highlights", () => {
    state().updateTitle("草稿");
    state().setUnverifiedTokens({ "block-1": ["15万元"] });

    const opened = createEmptyDocument();
    state().setDocument(opened, 1234);

    expect(state().document).toBe(opened);
    expect(state().dirty).toBe(false);
    expect(state().savedAt).toBe(1234);
    expect(state().lastDocumentId).toBe(opened.id);
    expect(state().unverifiedTokens).toEqual({});
  });

  it("keeps the security level in sync with the document", () => {
    state().updateSecurityLevel("secret");
    expect(state().document.meta.securityLevel).toBe("secret");
    expect(state().document.revision).toBe(initialState.document.revision + 1);
  });

  it("starts from the demo document", () => {
    expect(state().document.id).toBe("demo-request-2026");
    expect(state().document.body.title.text).toBe("关于申请购置应急排水设备经费的请示");
  });

  it("edits text blocks with a type-checked action", () => {
    state().setDocument(createEmptyDocument(), null);
    const paragraph = state().document.body.blocks[0];
    state().updateBlockText(paragraph.id, "拟购置排水设备 3 台。");
    expect(state().document.body.blocks[0]).toMatchObject({
      type: "paragraph",
      content: "拟购置排水设备 3 台。",
    });
  });

  it("ignores text edits on non-text blocks instead of corrupting them", () => {
    const list = { type: "list" as const, id: "list-1", style: "ordered" as const, items: [] };
    const document = createEmptyDocument();
    document.body.blocks = [list];
    state().setDocument(document, null);
    const revision = state().document.revision;
    const dirty = state().dirty;

    state().updateBlockText("list-1", "不应写入");
    state().updateHeadingLevel("list-1", "l2");

    expect(state().document.body.blocks[0]).toEqual(list);
    expect(state().document.revision).toBe(revision);
    expect(state().dirty).toBe(dirty);
  });

  it("changes heading levels only on headings", () => {
    const document = createEmptyDocument();
    document.body.blocks = [{ type: "heading", id: "h1", level: "l1", content: "总体要求" }];
    state().setDocument(document, null);

    state().updateHeadingLevel("h1", "l3");
    expect(state().document.body.blocks[0]).toMatchObject({ type: "heading", level: "l3" });
  });
});
