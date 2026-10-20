// @vitest-environment jsdom

import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DraftReference } from "../../lib/materialReferences";
import { WriteHome } from "./WriteHome";

const mocks = vi.hoisted(() => ({
  dragHandler: null as null | ((event: { payload: { type: string; paths?: string[] } }) => void),
  readDroppedReference: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: typeof mocks.dragHandler) => {
      mocks.dragHandler = handler;
      return mocks.unlisten;
    },
  }),
}));

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return {
    ...actual,
    readDroppedReference: mocks.readDroppedReference,
  };
});

const references: DraftReference[] = [
  { id: "ref-1", kind: "上传文件", label: "检查提纲.txt", content: "这是一段不应该出现在主输入框里的文件内容。" },
];

function renderHome(onAddReferences = vi.fn()) {
  return render(
    <WriteHome
      material="给各乡镇发一个防汛检查通知。"
      onMaterialChange={vi.fn()}
      providers={[{
        id: "cline-pass",
        kind: "openai_compatible",
        displayName: "Cline Pass",
        baseUrl: "https://api.cline.bot/api/v1",
        model: "cline-pass/deepseek-v4.1-flash",
        enabled: true,
        isLocal: false,
      }]}
      providerId="cline-pass"
      onProviderChange={vi.fn()}
      desktop
      starting={false}
      drafting={false}
      stage={null}
      completedStages={[]}
      draftText=""
      error={null}
      typeDecision={null}
      confirmedDocType={null}
      onConfirmedDocTypeChange={vi.fn()}
      onConfirmDocType={vi.fn()}
      onStart={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenDocuments={vi.fn()}
      onOpenRecent={vi.fn()}
      recentDocuments={[]}
      securityLevel="internal"
      onSecurityLevelChange={vi.fn()}
      prepareKnowledgeDocument={async () => "doc-1"}
      kbEnabled
      currentTitle=""
      onContinueDocument={vi.fn()}
      references={references}
      onAddReferences={onAddReferences}
      onRemoveReference={vi.fn()}
    />,
  );
}

beforeEach(() => {
  mocks.dragHandler = null;
  mocks.unlisten.mockReset();
  mocks.readDroppedReference.mockReset().mockResolvedValue({
    fileName: "拖拽文件.txt",
    text: "拖拽文件里的参考内容",
  });
});

afterEach(() => cleanup());

describe("WriteHome references", () => {
  it("shows only the reference name and keeps the file content out of the brief", () => {
    renderHome();

    const material = screen.getByLabelText("写作要求") as HTMLTextAreaElement;
    expect(material.value).toBe("给各乡镇发一个防汛检查通知。");
    expect(material.value).not.toContain("不应该出现在主输入框");
    expect(screen.getByText("检查提纲.txt")).toBeTruthy();
    expect(screen.getByRole("button", { name: "添加参考文件" })).toBeTruthy();
  });

  it("adds native Tauri drop paths without pasting their content into the brief", async () => {
    const onAddReferences = vi.fn();
    renderHome(onAddReferences);
    await waitFor(() => expect(mocks.dragHandler).not.toBeNull());

    act(() => {
      mocks.dragHandler?.({
        payload: { type: "drop", paths: ["/tmp/拖拽文件.txt"] },
      });
    });

    await waitFor(() => expect(onAddReferences).toHaveBeenCalledTimes(1));
    expect(mocks.readDroppedReference).toHaveBeenCalledWith("/tmp/拖拽文件.txt");
    expect(onAddReferences.mock.calls[0][0]).toEqual([
      expect.objectContaining({
        kind: "上传文件",
        label: "拖拽文件.txt",
        content: "拖拽文件里的参考内容",
      }),
    ]);
    const material = screen.getByLabelText("写作要求") as HTMLTextAreaElement;
    expect(material.value).toBe("给各乡镇发一个防汛检查通知。");
    expect(material.value).not.toContain("拖拽文件里的参考内容");
  });
});
