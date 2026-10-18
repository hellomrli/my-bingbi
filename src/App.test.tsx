// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useWorkbenchStore } from "./store/workbench";

vi.mock("./lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/ipc")>();
  return {
    ...actual,
    isDesktopRuntime: () => false,
    backendVersion: vi.fn(async () => "browser-preview"),
    createEmptyDocument: vi.fn(async () => (await import("./lib/document")).createEmptyDocument()),
    loadAppConfig: vi.fn(async () => ({ ...actual.defaultAppConfig, firstRunCompleted: true })),
    loadKbConfig: vi.fn(async () => actual.defaultKbConfig),
    listProviderProfiles: vi.fn(async () => [
      {
        id: "ollama",
        kind: "ollama",
        displayName: "本地 Ollama",
        baseUrl: "http://127.0.0.1:11434",
        model: "qwen3:8b",
        enabled: true,
        isLocal: true,
      },
    ]),
    listDocuments: vi.fn(async () => []),
    listTemplates: vi.fn(async () => []),
    saveAppConfig: vi.fn(async () => {}),
    saveDocument: vi.fn(async (_document: unknown, _stage: string) => ({
      documentId: "demo-request-2026",
      versionSeq: 1,
      created: false,
      versionCreated: true,
      savedAt: 1_700_000_000_000,
    })),
    exportDocx: vi.fn(),
    exportPdf: vi.fn(),
    importDocx: vi.fn(),
    loadDocument: vi.fn(),
    deleteDocument: vi.fn(),
  };
});

import { saveDocument } from "./lib/ipc";

const mockedSave = vi.mocked(saveDocument);

beforeEach(() => {
  localStorage.clear();
  useWorkbenchStore.setState(useWorkbenchStore.getInitialState(), true);
  mockedSave.mockClear();
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("App", () => {
  it("renders the workbench with the demo document and security status", async () => {
    render(<App />);

    expect((await screen.findAllByText("关于申请购置应急排水设备经费的请示")).length).toBeGreaterThan(0);
    expect(screen.getByText("允许配置的云端模型")).toBeTruthy();
    expect(screen.getByText("版式预览")).toBeTruthy();
  });

  it("saves the current document with Ctrl+S", async () => {
    render(<App />);

    fireEvent.keyDown(window, { key: "s", ctrlKey: true });

    await waitFor(() => expect(mockedSave).toHaveBeenCalledTimes(1));
    expect(mockedSave).toHaveBeenCalledWith(
      expect.objectContaining({ id: "demo-request-2026" }),
      "user_save",
    );
    expect(await screen.findByText("已保存版本 1")).toBeTruthy();
  });

  it("creates a blank document and switches to the structure editor", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "新建公文" }));

    expect(await screen.findByText("已创建空白 ODoc")).toBeTruthy();
    expect(useWorkbenchStore.getState().document.body.title.text).toBe("");
    expect(useWorkbenchStore.getState().view).toBe("structure");
  });

  it("opens the drafting wizard from the toolbar", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "向导模式" }));

    expect(screen.getByRole("dialog", { name: "向导模式" })).toBeTruthy();
    expect(screen.getByText(/第 1 \/ 6 步/)).toBeTruthy();
  });

  it("opens the command palette with Ctrl+K and runs a searched action", async () => {
    render(<App />);

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const search = await screen.findByLabelText("搜索命令");
    fireEvent.change(search, { target: { value: "向导" } });
    fireEvent.keyDown(search, { key: "Enter" });

    expect(screen.getByRole("dialog", { name: "向导模式" })).toBeTruthy();
  });

  it("fills the writing material from the wizard when no model is available", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "向导模式" }));
    const dialog = screen.getByRole("dialog", { name: "向导模式" });

    fireEvent.click(within(dialog).getByRole("button", { name: /下一步/ }));
    fireEvent.change(within(dialog).getByLabelText(/发文事由/), { target: { value: "开展汛前检查" } });
    fireEvent.change(within(dialog).getByLabelText(/主送机关/), { target: { value: "×××单位" } });
    // subject → facts → constraints → materials → review
    for (let step = 0; step < 4; step += 1) {
      fireEvent.click(within(dialog).getByRole("button", { name: /下一步/ }));
    }
    fireEvent.click(within(dialog).getByRole("button", { name: /填入写作材料/ }));

    const material = screen.getByPlaceholderText(/写下想解决什么问题/) as HTMLTextAreaElement;
    expect(material.value).toContain("【文种】请示");
    expect(material.value).toContain("【事由】开展汛前检查");
    expect(await screen.findByText(/向导内容已填入写作区/)).toBeTruthy();
    expect(useWorkbenchStore.getState().document.meta.docType).toBe("request");
  });
});
