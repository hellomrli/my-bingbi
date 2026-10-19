// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DraftEvent, DraftOutcome, RerunOutcome } from "../lib/generated/engine";
import type { ODoc } from "../lib/odoc";
import { createEmptyDocument } from "../lib/document";
import { useWorkbenchStore } from "../store/workbench";
import { useDraftPipeline } from "./useDraftPipeline";

vi.mock("../lib/engine", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/engine")>();
  return {
    ...actual,
    startDraft: vi.fn(),
    startImitate: vi.fn(),
    startPolish: vi.fn(),
    rerunStage: vi.fn(),
  };
});

vi.mock("../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/ipc")>();
  return { ...actual, isDesktopRuntime: () => true };
});

import { rerunStage, startDraft, startImitate, startPolish } from "../lib/engine";

const mockedStartDraft = vi.mocked(startDraft);
const mockedStartImitate = vi.mocked(startImitate);
const mockedStartPolish = vi.mocked(startPolish);
const mockedRerunStage = vi.mocked(rerunStage);

const localProvider = {
  id: "ollama",
  kind: "ollama" as const,
  displayName: "本地 Ollama",
  baseUrl: "http://127.0.0.1:11434",
  model: "qwen3:8b",
  enabled: true,
  isLocal: true,
};

function outcomeFor(document: ODoc, revision: number, sessionId = "session-1"): DraftOutcome {
  return {
    sessionId,
    document: { ...structuredClone(document), revision },
    typeDecision: { value: document.meta.docType, confidence: 0.9, alternatives: [], missingInformation: [] },
    facts: { facts: [], missingInformation: [] },
    stages: [],
    lintReport: null,
    factCheckReport: null,
    score: null,
    revisionIterations: 0,
  };
}

function rerunFor(document: ODoc, stageId: RerunOutcome["stageId"], sessionId = "session-1"): RerunOutcome {
  return {
    sessionId,
    stageId,
    document: { ...structuredClone(document), revision: document.revision + 1 },
    output: { stageId, status: "completed", content: "重跑结果", json: null, usage: null, durationMs: 1 },
    lintReport: null,
    factCheckReport: null,
  };
}

function renderPipeline(overrides: Partial<Parameters<typeof useDraftPipeline>[0]> = {}) {
  const callbacks = {
    saveBeforeDraft: vi.fn(async () => {}),
    onDraftOutcome: vi.fn((outcome: DraftOutcome) => {
      useWorkbenchStore.getState().setDocument(outcome.document, 1);
    }),
    onRerunOutcome: vi.fn((outcome: RerunOutcome) => {
      useWorkbenchStore.getState().setDocument(outcome.document, 1);
    }),
  };
  const props: Parameters<typeof useDraftPipeline>[0] = {
    document: useWorkbenchStore.getState().document,
    providers: [localProvider],
    providerId: "ollama",
    material: "  写一份请示  ",
    modelText: "范文字段",
    ...callbacks,
    ...overrides,
  };
  return {
    ...renderHook((current: Parameters<typeof useDraftPipeline>[0]) => useDraftPipeline(current), {
      initialProps: props,
    }),
    callbacks,
    props,
  };
}

beforeEach(() => {
  localStorage.clear();
  const document = createEmptyDocument();
  document.body.title.text = "测试请示";
  useWorkbenchStore.getState().setDocument(document, null);
  mockedStartDraft.mockReset();
  mockedStartImitate.mockReset();
  mockedStartPolish.mockReset();
  mockedRerunStage.mockReset();
});

describe("useDraftPipeline", () => {
  it("saves, runs and applies a finished draft", async () => {
    const document = useWorkbenchStore.getState().document;
    const outcome = outcomeFor(document, 5);
    mockedStartDraft.mockResolvedValue(outcome);
    const { result, callbacks } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(callbacks.saveBeforeDraft).toHaveBeenCalledTimes(1);
    expect(mockedStartDraft).toHaveBeenCalledWith(
      expect.objectContaining({ documentId: document.id, material: "写一份请示", providerId: "ollama" }),
      expect.any(Function),
    );
    expect(callbacks.onDraftOutcome).toHaveBeenCalledWith(outcome);
    expect(result.current.sessionId).toBe("session-1");
    expect(result.current.drafting).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it("applies the streamed completion only once when the command returns the same outcome", async () => {
    const document = useWorkbenchStore.getState().document;
    const outcome = outcomeFor(document, 5);
    mockedStartDraft.mockImplementation(async (_input, onEvent) => {
      onEvent({ type: "completed", outcome });
      return outcome;
    });
    const { result, callbacks } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(callbacks.onDraftOutcome).toHaveBeenCalledTimes(1);
    expect(result.current.error).toBeNull();
  });

  it("does not clobber edits made while the draft is running", async () => {
    const document = useWorkbenchStore.getState().document;
    const outcome = outcomeFor(document, 5);
    mockedStartDraft.mockImplementation(async () => {
      useWorkbenchStore.getState().updateTitle("运行期间用户改过的标题");
      return outcome;
    });
    const { result, callbacks } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(callbacks.onDraftOutcome).not.toHaveBeenCalled();
    expect(result.current.error).toContain("运行期间文档已被修改");
    expect(useWorkbenchStore.getState().document.body.title.text).toBe("运行期间用户改过的标题");
  });

  it("runs with the material the wizard composed, before the textarea re-renders", async () => {
    const document = useWorkbenchStore.getState().document;
    mockedStartDraft.mockResolvedValue(outcomeFor(document, 5));
    const { result } = renderPipeline();
    const wizardMaterial = "【文种】通知\n【事由】开展汛前检查";

    await act(async () => {
      await result.current.startDraft("notice", wizardMaterial, "notice-meeting");
    });

    expect(mockedStartDraft).toHaveBeenCalledWith(
      expect.objectContaining({
        material: wizardMaterial,
        confirmedDocType: "notice",
        templateId: "notice-meeting",
      }),
      expect.any(Function),
    );
  });

  it("attributes a failure to the stage that was running", async () => {
    mockedStartDraft.mockImplementation(async (_input, onEvent) => {
      onEvent({ type: "stage_started", stageId: "draft" });
      throw { code: "provider_error", message: "模型连接失败" };
    });
    const { result } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(result.current.error).toBe("模型连接失败");
    expect(result.current.failedStage).toBe("draft");
    expect(result.current.drafting).toBe(false);
  });

  it("treats a pending document-type confirmation as a pause, not a failure", async () => {
    mockedStartDraft.mockRejectedValue({
      code: "document_type_confirmation_required",
      message: "需要确认文种",
    });
    const { result } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(result.current.error).toBeNull();
    expect(result.current.failedStage).toBeNull();
  });

  it("refuses to run for a remote provider when the document is classified", async () => {
    const document = useWorkbenchStore.getState().document;
    document.meta.securityLevel = "secret";
    useWorkbenchStore.getState().setDocument(document, null);
    const { result } = renderPipeline({
      document,
      providers: [{ ...localProvider, id: "cloud", isLocal: false }],
      providerId: "cloud",
    });

    await act(async () => {
      await result.current.startDraft(null);
    });

    expect(mockedStartDraft).not.toHaveBeenCalled();
    expect(result.current.error).toContain("仅允许本地模型");
  });

  it("clears session and stages when another document is opened", async () => {
    const document = useWorkbenchStore.getState().document;
    const outcome = outcomeFor(document, 5);
    mockedStartDraft.mockImplementation(async (_input, onEvent: (event: DraftEvent) => void) => {
      onEvent({ type: "stage_started", stageId: "doctype" });
      onEvent({
        type: "stage_completed",
        output: { stageId: "doctype", status: "completed", content: "", json: null, usage: null, durationMs: 1 },
      });
      return outcome;
    });
    const { result, rerender, props } = renderPipeline();

    await act(async () => {
      await result.current.startDraft(null);
    });
    expect(result.current.sessionId).toBe("session-1");
    expect(result.current.completedStages).toEqual(["doctype"]);

    const other = createEmptyDocument();
    await act(async () => {
      rerender({ ...props, document: other });
    });

    expect(result.current.sessionId).toBeNull();
    expect(result.current.completedStages).toEqual([]);
  });

  it("keeps only the stages up to a re-run stage", async () => {
    const document = useWorkbenchStore.getState().document;
    const outcome = outcomeFor(document, 5);
    mockedStartDraft.mockImplementation(async (_input, onEvent) => {
      for (const stageId of ["doctype", "facts", "retrieve", "outline"] as const) {
        onEvent({
          type: "stage_completed",
          output: { stageId, status: "completed", content: "", json: null, usage: null, durationMs: 1 },
        });
      }
      return outcome;
    });
    const { result } = renderPipeline();
    await act(async () => {
      await result.current.startDraft(null);
    });
    expect(result.current.completedStages).toEqual(["doctype", "facts", "retrieve", "outline"]);

    mockedRerunStage.mockResolvedValueOnce(rerunFor(useWorkbenchStore.getState().document, "retrieve"));
    await act(async () => {
      await result.current.rerun("retrieve");
    });

    expect(result.current.completedStages).toEqual(["doctype", "facts", "retrieve"]);
    expect(mockedRerunStage).toHaveBeenCalledWith("session-1", "retrieve", expect.any(Function));
  });
});
