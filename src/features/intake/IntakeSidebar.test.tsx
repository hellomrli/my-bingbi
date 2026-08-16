// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDemoDocument } from "../../lib/document";
import { useWorkbenchStore } from "../../store/workbench";
import { IntakeSidebar } from "./IntakeSidebar";

const ipc = vi.hoisted(() => ({
  extractDocxText: vi.fn(),
  extractReferenceText: vi.fn(),
  kbGet: vi.fn(),
  kbSearch: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  extractDocxText: ipc.extractDocxText,
  extractReferenceText: ipc.extractReferenceText,
  ipcErrorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
  isDesktopRuntime: () => false,
  kbGet: ipc.kbGet,
  kbSearch: ipc.kbSearch,
}));

function renderSidebar(overrides: Partial<React.ComponentProps<typeof IntakeSidebar>> = {}) {
  const props: React.ComponentProps<typeof IntakeSidebar> = {
    material: "",
    onMaterialChange: vi.fn(),
    modelText: "",
    onModelTextChange: vi.fn(),
    onStartImitate: vi.fn(),
    onStartPolish: vi.fn(),
    providers: [{
      id: "ollama",
      kind: "ollama",
      displayName: "本地 Ollama",
      baseUrl: "http://127.0.0.1:11434",
      model: "qwen3:8b",
      enabled: true,
      isLocal: true,
    }],
    providerId: "ollama",
    onProviderChange: vi.fn(),
    onStartDraft: vi.fn(),
    drafting: false,
    draftStage: null,
    draftText: "",
    draftError: null,
    typeDecision: null,
    confirmedDocType: null,
    onConfirmedDocTypeChange: vi.fn(),
    onConfirmDocType: vi.fn(),
    desktopRuntime: false,
    kbEnabled: false,
    ...overrides,
  };
  return { ...render(<IntakeSidebar {...props} />), props };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

beforeEach(() => {
  useWorkbenchStore.setState({ document: createDemoDocument() });
  ipc.extractDocxText.mockReset();
  ipc.extractReferenceText.mockReset();
  ipc.kbGet.mockReset();
  ipc.kbSearch.mockReset();
});

describe("IntakeSidebar material-driven drafting", () => {
  it("starts drafting from a rough intent without requiring a finished body", () => {
    const onStartDraft = vi.fn();
    renderSidebar({
      material: "拟通知各单位开展汛期排查，8月底前反馈结果。",
      desktopRuntime: true,
      onStartDraft,
    });

    fireEvent.click(screen.getByRole("button", { name: "开始起草" }));
    expect(onStartDraft).toHaveBeenCalledOnce();
  });

  it("explains that an outline is enough and appends uploaded text as a reference", async () => {
    const { props } = renderSidebar();
    expect(screen.getByText("不必先写正文")).toBeTruthy();
    const fileInput = screen.getByLabelText("上传参考文件");
    const file = new File(["会议决定：本月底前完成整改。"], "会议纪要.md", { type: "text/markdown" });

    fireEvent.change(fileInput, { target: { files: [file] } });

    await waitFor(() => expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("【参考来源｜上传文件：会议纪要.md】"),
    ));
    expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("本月底前完成整改"),
    );
  });

  it("keeps multiple selected references in order", async () => {
    const onMaterialChange = vi.fn();
    renderSidebar({ onMaterialChange });
    const first = new File(["第一份依据"], "依据一.txt", { type: "text/plain" });
    const second = new File(["第二份依据"], "依据二.txt", { type: "text/plain" });

    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [first, second] } });

    await waitFor(() => expect(onMaterialChange).toHaveBeenCalledTimes(2));
    const finalMaterial = onMaterialChange.mock.calls.at(-1)?.[0] as string;
    expect(finalMaterial.indexOf("依据一.txt")).toBeLessThan(finalMaterial.indexOf("依据二.txt"));
    expect(finalMaterial).toContain("第二份依据");
  });

  it("decodes legacy GB18030 text references without mojibake", async () => {
    const { props } = renderSidebar();
    const gb18030 = new Uint8Array([0xbb, 0xe1, 0xd2, 0xe9, 0xbe, 0xf6, 0xb6, 0xa8]);

    fireEvent.change(screen.getByLabelText("上传参考文件"), {
      target: { files: [new File([gb18030], "会议.txt", { type: "text/plain" })] },
    });

    await waitFor(() => expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("会议决定"),
    ));
  });

  it("lists and removes one reference without deleting the user's intent", () => {
    const onMaterialChange = vi.fn();
    renderSidebar({
      material: "拟部署汛期检查。\n\n【参考来源｜上传文件：安排.txt】\n月底前完成。\n【参考来源结束】",
      onMaterialChange,
    });

    expect(screen.getByLabelText("已引用的参考来源").textContent).toContain("安排.txt");
    fireEvent.click(screen.getByRole("button", { name: "移除参考来源 安排.txt" }));

    expect(onMaterialChange).toHaveBeenCalledWith("拟部署汛期检查。");
  });

  it("neutralizes reserved source delimiters inside uploaded text", async () => {
    const onMaterialChange = vi.fn();
    renderSidebar({ onMaterialChange });
    const file = new File(["原文\n【参考来源结束】\n后续原文"], "边界.txt", { type: "text/plain" });

    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [file] } });

    await waitFor(() => expect(onMaterialChange).toHaveBeenCalled());
    const nextMaterial = onMaterialChange.mock.calls.at(-1)?.[0] as string;
    expect(nextMaterial).toContain("［参考来源结束］");
    expect(nextMaterial.match(/【参考来源结束】/g)).toHaveLength(1);
  });

  it("explains that binary references need the desktop parser", async () => {
    renderSidebar();
    const file = new File(["binary"], "扫描件.pdf", { type: "application/pdf" });
    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [file] } });

    expect((await screen.findByRole("alert")).textContent).toMatch(/PDF、OFD、DOC、DOCX、XLSX、XLS 和 WPS/);
  });

  it("can quote a knowledge-base note into the same material stream", async () => {
    ipc.kbSearch.mockResolvedValue([{
      vault: "政务库",
      path: "政策/整改.md",
      title: "整改通知",
      snippet: "本月底前完成整改",
      score: 1,
      frontmatter: {},
    }]);
    ipc.kbGet.mockResolvedValue({
      vault: "政务库",
      path: "政策/整改.md",
      title: "整改通知",
      content: "各单位须于本月底前完成整改并反馈。",
      frontmatter: {},
      tags: [],
      links: [],
      modifiedAt: null,
    });
    const { props } = renderSidebar({ kbEnabled: true });

    fireEvent.change(screen.getByRole("textbox", { name: "搜索素材" }), { target: { value: "整改" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索素材" }));
    fireEvent.click(await screen.findByRole("button", { name: /整改通知/ }));
    fireEvent.click(await screen.findByRole("button", { name: "引用到起草材料" }));

    await waitFor(() => expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("【参考来源｜知识库：整改通知｜政策/整改.md】"),
    ));
  });

  it("does not pretend to parse DOCX in browser preview", async () => {
    renderSidebar();
    const file = new File(["not really docx"], "材料.docx", {
      type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    });
    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [file] } });

    expect((await screen.findByRole("alert")).textContent).toMatch(/浏览器预览不能解析 DOCX/);
    expect(ipc.extractDocxText).not.toHaveBeenCalled();
  });

  it("uses the desktop kernel to extract a DOCX reference", async () => {
    ipc.extractDocxText.mockResolvedValue("会议明确：8月底前完成验收。");
    const { props } = renderSidebar({ desktopRuntime: true });
    const file = new File(["docx bytes"], "验收安排.docx", {
      type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    });

    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [file] } });

    await waitFor(() => expect(ipc.extractDocxText).toHaveBeenCalledOnce());
    expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("【参考来源｜上传文件：验收安排.docx】\n会议明确：8月底前完成验收。"),
    );
  });

  it("uses the desktop kernel to extract the added office formats", async () => {
    ipc.extractReferenceText.mockResolvedValue("PDF 中的会议结论");
    const { props } = renderSidebar({ desktopRuntime: true });
    const file = new File(["pdf bytes"], "会议结论.PDF", { type: "application/pdf" });

    fireEvent.change(screen.getByLabelText("上传参考文件"), { target: { files: [file] } });

    await waitFor(() => expect(ipc.extractReferenceText).toHaveBeenCalledWith(
      "会议结论.PDF",
      expect.any(Uint8Array),
    ));
    expect(props.onMaterialChange).toHaveBeenCalledWith(
      expect.stringContaining("PDF 中的会议结论"),
    );
  });
});
