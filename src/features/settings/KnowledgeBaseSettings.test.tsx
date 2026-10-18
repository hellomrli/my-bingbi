// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { KbConfig } from "../../lib/kb";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";

const mocks = vi.hoisted(() => ({
  loadKbConfig: vi.fn(),
  saveKbConfig: vi.fn(),
  hasKbFnsToken: vi.fn(),
  hasKbFnsWsToken: vi.fn(),
  hasKbMcpToken: vi.fn(),
  kbIndexState: vi.fn(),
  kbCreateLocalVault: vi.fn(),
  kbCreateNote: vi.fn(),
  kbRebuildIndex: vi.fn(),
}));

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return {
    ...actual,
    loadKbConfig: mocks.loadKbConfig,
    saveKbConfig: mocks.saveKbConfig,
    hasKbFnsToken: mocks.hasKbFnsToken,
    hasKbFnsWsToken: mocks.hasKbFnsWsToken,
    hasKbMcpToken: mocks.hasKbMcpToken,
    kbIndexState: mocks.kbIndexState,
    kbCreateLocalVault: mocks.kbCreateLocalVault,
    kbCreateNote: mocks.kbCreateNote,
    kbRebuildIndex: mocks.kbRebuildIndex,
  };
});

const baseConfig: KbConfig = {
  schemaVersion: 1,
  enabled: false,
  providerKind: "fns",
  baseUrl: "http://127.0.0.1:9000",
  vault: "",
  localPath: "",
  indexEnabled: true,
  autoTaggingEnabled: false,
  syncEnabled: true,
  mcpUrl: "",
};

beforeEach(() => {
  mocks.loadKbConfig.mockReset().mockResolvedValue(baseConfig);
  mocks.saveKbConfig.mockReset().mockImplementation(async (config: KbConfig) => config);
  mocks.hasKbFnsToken.mockReset().mockResolvedValue(false);
  mocks.hasKbFnsWsToken.mockReset().mockResolvedValue(false);
  mocks.hasKbMcpToken.mockReset().mockResolvedValue(false);
  mocks.kbIndexState.mockReset().mockResolvedValue(null);
  mocks.kbCreateLocalVault.mockReset().mockResolvedValue("C:\\Data\\knowledge-base");
  mocks.kbCreateNote.mockReset().mockResolvedValue({ path: "收件箱/1-防汛要点.md", title: "防汛要点" });
  mocks.kbRebuildIndex.mockReset().mockResolvedValue({
    vault: "本地资料库",
    providerKind: "local_vault",
    lastCursor: 0,
    noteCount: 1,
    chunkCount: 1,
    status: "ready",
    errorMessage: null,
    updatedAt: 1,
  });
});

afterEach(() => cleanup());

describe("KnowledgeBaseSettings local vault", () => {
  it("creates a managed vault and persists the resulting path", async () => {
    render(<KnowledgeBaseSettings desktop />);
    await screen.findByText("可选组件");

    fireEvent.click(screen.getByRole("button", { name: "本地 Vault" }));
    fireEvent.click(screen.getByRole("button", { name: /创建本地知识库/ }));

    await waitFor(() => expect(mocks.kbCreateLocalVault).toHaveBeenCalledWith(undefined));
    await waitFor(() =>
      expect(mocks.saveKbConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          enabled: true,
          providerKind: "local_vault",
          localPath: "C:\\Data\\knowledge-base",
          vault: "本地资料库",
        }),
      ),
    );
    expect(await screen.findByDisplayValue("C:\\Data\\knowledge-base")).toBeTruthy();
    expect(screen.getByText(/本地知识库已就绪/)).toBeTruthy();
  });

  it("writes a note and refreshes the index", async () => {
    render(<KnowledgeBaseSettings desktop />);
    await screen.findByText("可选组件");

    fireEvent.click(screen.getByRole("button", { name: "本地 Vault" }));
    fireEvent.click(screen.getByRole("button", { name: /创建本地知识库/ }));
    await screen.findByLabelText("资料标题");

    fireEvent.change(screen.getByLabelText("资料标题"), { target: { value: "防汛要点" } });
    fireEvent.change(screen.getByLabelText("资料内容"), { target: { value: "汛前完成隐患排查。" } });
    fireEvent.click(screen.getByRole("button", { name: /保存到知识库/ }));

    await waitFor(() =>
      expect(mocks.kbCreateNote).toHaveBeenCalledWith({ title: "防汛要点", content: "汛前完成隐患排查。" }),
    );
    await waitFor(() => expect(mocks.kbRebuildIndex).toHaveBeenCalledWith(undefined, expect.any(Function)));
    expect(await screen.findByText(/索引收录 1 篇素材/)).toBeTruthy();
  });
});
