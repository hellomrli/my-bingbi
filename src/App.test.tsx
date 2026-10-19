// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

describe("App writing desk", () => {
  it("opens on a single writing input instead of a form", async () => {
    render(<App />);

    expect(await screen.findByRole("heading", { name: "今天想写点什么？" })).toBeTruthy();
    expect(screen.getByLabelText("写作要求")).toBeTruthy();
    expect(screen.getByRole("button", { name: /开始写作/ })).toBeTruthy();
    // Optional metadata fields stay out of the main flow.
    expect(screen.queryByText("发文机关标志")).toBeNull();
    expect(screen.queryByText("密级")).toBeNull();
  });

  it("accepts a rough brief and an optional document type", async () => {
    render(<App />);
    const material = screen.getByLabelText("写作要求") as HTMLTextAreaElement;

    fireEvent.change(material, {
      target: { value: "给各乡镇发一个防汛检查通知，下周开始检查，重点看值班和物资。" },
    });
    fireEvent.click(screen.getByRole("button", { name: "通知" }));

    expect(material.value).toContain("防汛检查");
    expect(screen.getByRole("button", { name: "通知" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "自动识别" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("continues the current document into the workspace", async () => {
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: /继续编辑/ }));

    expect(screen.getByRole("tab", { name: "编辑" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "版式" })).toBeTruthy();
    const title = screen.getByLabelText("公文标题") as HTMLInputElement;
    expect(title.value).toBe("关于申请购置应急排水设备经费的请示");
    expect(screen.getByRole("button", { name: /^质检/ })).toBeTruthy();
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

  it("creates a blank document and returns to the writing desk", async () => {
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "更多" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /新建写作/ }));

    expect(await screen.findByText("已新建空白文档，从写作台开始吧")).toBeTruthy();
    expect(useWorkbenchStore.getState().document.body.title.text).toBe("");
    expect(screen.getByLabelText("写作要求")).toBeTruthy();
  });

  it("opens the command palette and runs the settings action", async () => {
    render(<App />);

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const search = await screen.findByLabelText("搜索命令");
    fireEvent.change(search, { target: { value: "设置" } });
    fireEvent.keyDown(search, { key: "Enter" });

    expect(await screen.findByRole("dialog", { name: "设置" })).toBeTruthy();
  });

  it("keeps optional document fields in a dialog", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /继续编辑/ }));
    fireEvent.click(screen.getByRole("button", { name: "更多" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /文档信息/ }));

    expect(await screen.findByRole("dialog", { name: "文档信息" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("发文字号"), { target: { value: "×政〔2026〕9号" } });
    expect(useWorkbenchStore.getState().document.header?.documentNumber).toBe("×政〔2026〕9号");
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    expect(screen.queryByRole("dialog", { name: "文档信息" })).toBeNull();
  });

  it("shows quality findings in a drawer", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /继续编辑/ }));
    fireEvent.click(screen.getByRole("button", { name: /^质检/ }));

    const drawer = await screen.findByRole("dialog", { name: "质检结果" });
    expect(drawer).toBeTruthy();
    expect(screen.getByText(/事实台账/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "返回正文" }));
    expect(screen.queryByRole("dialog", { name: "质检结果" })).toBeNull();
  });
});
