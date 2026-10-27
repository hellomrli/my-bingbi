import { useCallback, useEffect, useRef, useState } from "react";
import type { DraftEvent, DraftOutcome, RerunOutcome, StageFailure, TypeDecision } from "../lib/generated/engine";
import {
  draftStageLabels,
  draftStages,
  imitateStages,
  polishStages,
  rerunStage,
  shouldApplyDraftOutcome,
  shouldApplyRerunOutcome,
  startDraft,
  startImitate,
  startPolish,
  type DraftStageId,
  type DraftStartInput,
} from "../lib/engine";
import { errorCode } from "../lib/errors";
import { appendGenerationLog } from "../lib/generationLog";
import { IpcError, ipcErrorMessage, isDesktopRuntime } from "../lib/ipc";
import type { ProviderProfile } from "../lib/llm";
import type { DocumentType, ODoc, SecurityLevel } from "../lib/odoc";
import { useWorkbenchStore } from "../store/workbench";

export type DraftPipelineKind = "draft" | "imitate" | "polish";

interface UseDraftPipelineOptions {
  document: ODoc;
  providers: ProviderProfile[];
  providerId: string;
  material: string;
  materialSecurityLevel?: SecurityLevel;
  modelText: string;
  /** Selected knowledge-pack template; empty means the per-type default. */
  templateId?: string | null;
  /** Persists the current document before the kernel snapshots it for the run. */
  saveBeforeDraft: () => Promise<void>;
  /** Applies a finished draft to the workbench (document + reports). */
  onDraftOutcome: (outcome: DraftOutcome) => void;
  /** Applies a finished single-stage re-run. */
  onRerunOutcome: (outcome: RerunOutcome, stageId: DraftStageId) => void;
}

export interface DraftPipeline {
  drafting: boolean;
  kind: DraftPipelineKind;
  stage: DraftStageId | null;
  completedStages: DraftStageId[];
  failedStage: DraftStageId | null;
  /** Session id of the last completed run; single stages re-run from it. */
  sessionId: string | null;
  /** Streamed token text for the draft/deai stages. */
  text: string;
  error: string | null;
  warnings: StageFailure[];
  typeDecision: TypeDecision | null;
  confirmedDocType: DocumentType | null;
  setConfirmedDocType: (value: DocumentType | null) => void;
  startDraft: (
    confirmedDocType?: DocumentType | null,
    materialOverride?: string,
    templateIdOverride?: string | null,
  ) => Promise<void>;
  startImitate: () => Promise<void>;
  startPolish: () => Promise<void>;
  rerun: (stageId: DraftStageId) => Promise<void>;
}

function isDraftStage(value: string): value is DraftStageId {
  // `in` would also match inherited properties like "toString" from a hostile event source.
  return Object.hasOwn(draftStageLabels, value);
}

function stageOrder(kind: DraftPipelineKind): readonly DraftStageId[] {
  if (kind === "imitate") return imitateStages;
  if (kind === "polish") return polishStages;
  return draftStages;
}

function isRestricted(document: ODoc): boolean {
  return document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
}

/**
 * Owns the desktop draft/imitate/polish pipelines and their re-runs: transient stage state,
 * streaming text, failure attribution, and the guards that keep a finished run from writing
 * into a document the user has since switched away from.
 *
 * The three entry points share one runner on purpose — they only differ in the kernel command
 * and in which inputs they require, which is exactly where the copies used to drift.
 */
export function useDraftPipeline({
  document,
  providers,
  providerId,
  material,
  materialSecurityLevel,
  modelText,
  templateId,
  saveBeforeDraft,
  onDraftOutcome,
  onRerunOutcome,
}: UseDraftPipelineOptions): DraftPipeline {
  const [drafting, setDrafting] = useState(false);
  const [kind, setKind] = useState<DraftPipelineKind>("draft");
  const [stage, setStage] = useState<DraftStageId | null>(null);
  const [completedStages, setCompletedStages] = useState<DraftStageId[]>([]);
  const [failedStage, setFailedStage] = useState<DraftStageId | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<StageFailure[]>([]);
  const [typeDecision, setTypeDecision] = useState<TypeDecision | null>(null);
  const [confirmedDocType, setConfirmedDocType] = useState<DocumentType | null>(null);
  // Catch blocks read the stage a failure died on; the render-time value would still be the
  // one from when the run started.
  const stageRef = useRef<DraftStageId | null>(null);
  // Identity + revision of the document the active run snapshotted. A finished run may only be
  // written back while the workbench still shows exactly that state, otherwise the user's edits
  // made during the run would be lost.
  const runBaseRef = useRef<ODoc | null>(null);
  const runningRef = useRef(false);
  const appliedRef = useRef(false);
  const kindRef = useRef<DraftPipelineKind>("draft");
  const sessionRef = useRef<string | null>(null);
  const warningsRef = useRef<StageFailure[]>([]);
  const lastBodyTextRef = useRef("");
  const failureLoggedRef = useRef(false);
  // Async runs compare against the document they started from.
  const documentId = document.id;

  const resetTransientState = useCallback(() => {
    stageRef.current = null;
    setStage(null);
    setFailedStage(null);
    setText("");
    setError(null);
    setTypeDecision(null);
    setConfirmedDocType(null);
  }, []);

  /** Switching documents invalidates every recorded stage of the previous run. */
  useEffect(() => {
    setCompletedStages([]);
    setSessionId(null);
    sessionRef.current = null;
    setWarnings([]);
    resetTransientState();
  }, [documentId, resetTransientState]);

  useEffect(() => {
    const onPageHide = () => {
      if (!runningRef.current) return;
      appendGenerationLog({
        level: "error", event: "error", code: "view_unloaded",
        message: "写作页面已关闭或重新加载；已生成正文可从起草记录中恢复。",
        stage: stageRef.current, sessionId: sessionRef.current,
        documentId: runBaseRef.current?.id, pipeline: kindRef.current, providerId,
      });
    };
    window.addEventListener("pagehide", onPageHide);
    return () => window.removeEventListener("pagehide", onPageHide);
  }, [providerId]);

  function providerRejection(): string | null {
    const selected = providers.find((provider) => provider.id === providerId);
    // Read the store at call time: the composer may have just swapped in a fresh document.
    const restricted = isRestricted(useWorkbenchStore.getState().document)
      || materialSecurityLevel === "secret" || materialSecurityLevel === "confidential";
    if (!selected?.enabled || (restricted && !selected.isLocal)) {
      return restricted ? "当前密级仅允许本地模型，请先选择本地服务。" : "模型服务不可用，请先在设置中启用服务。";
    }
    return null;
  }

  /**
   * Whether the finished run may be written into the workbench.
   *
   * Silence would be worse than a clobber here: when the document was edited in the meantime the
   * user gets told explicitly, and the local edits stay untouched.
   */
  function canWriteBack(outcomeDocumentId: string): boolean {
    const base = runBaseRef.current;
    const current = useWorkbenchStore.getState().document;
    if (base && current.id === outcomeDocumentId && JSON.stringify(current) === JSON.stringify(base)) return true;
    if (current.id === outcomeDocumentId) {
      const message = "运行期间文档已被修改，结果未自动应用；可从起草记录中另存生成结果。";
      setError(message);
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message,
        providerId,
        pipeline: kindRef.current,
        sessionId: sessionRef.current,
      });
    }
    return false;
  }

  function applyOutcome(outcome: DraftOutcome) {
    appliedRef.current = true;
    setSessionId(outcome.sessionId);
    sessionRef.current = outcome.sessionId;
    resetTransientState();
    setWarnings(outcome.warnings ?? warningsRef.current);
    appendGenerationLog({
      level: "info",
      event: "complete",
      message: (outcome.warnings?.length ?? warningsRef.current.length) > 0
        ? "正文已生成并保存，部分辅助检查未完成，请按提示复核"
        : "起草完成，结果已写入正文",
      pipeline: kindRef.current,
      sessionId: outcome.sessionId,
      providerId,
      documentId: outcome.document.id,
    });
    onDraftOutcome(outcome);
  }

  function handleEvent(event: DraftEvent) {
    if (appliedRef.current) return;
    if (runBaseRef.current && useWorkbenchStore.getState().document.id !== runBaseRef.current.id) return;
    if (event.type === "session_started") {
      sessionRef.current = event.sessionId;
      setSessionId(event.sessionId);
      return;
    }
    if (event.type === "stage_started") {
      if (isDraftStage(event.stageId)) {
        stageRef.current = event.stageId;
        setStage(event.stageId);
        setFailedStage(null);
        failureLoggedRef.current = false;
        if (event.stageId === "draft" || event.stageId === "deai") setText("");
        appendGenerationLog({
          level: "info",
          event: "stage",
          stage: event.stageId,
          message: `开始${draftStageLabels[event.stageId]}阶段`,
          providerId,
          pipeline: kindRef.current,
          sessionId: sessionRef.current,
          documentId: runBaseRef.current?.id,
        });
      }
      return;
    }
    if (event.type === "token") {
      if (event.stageId === "draft" || event.stageId === "deai") setText((current) => current + event.content);
      return;
    }
    if (event.type === "needs_document_type") {
      setTypeDecision(event.decision);
      setConfirmedDocType(event.decision.value);
      return;
    }
    if (event.type === "stage_completed") {
      const stageId = event.output.stageId;
      if (isDraftStage(stageId)) {
        setCompletedStages((current) => (current.includes(stageId) ? current : [...current, stageId]));
        setStage(stageId);
        if ((stageId === "draft" || stageId === "deai") && event.output.content) {
          setText(event.output.content);
          lastBodyTextRef.current = event.output.content;
        }
        appendGenerationLog({
          level: "info",
          event: "stage",
          stage: stageId,
          message: `${draftStageLabels[stageId]}阶段完成`,
          providerId,
          pipeline: kindRef.current,
          sessionId: sessionRef.current,
          documentId: runBaseRef.current?.id,
        });
      }
      return;
    }
    if (event.type === "stage_failed") {
      const stageId = event.output.stageId;
      if (!isDraftStage(stageId)) return;
      let code: string | null = null;
      try {
        const parsed = JSON.parse(event.output.json ?? "null")?.code;
        if (typeof parsed === "string") code = parsed;
      } catch { /* Older runtimes may not attach a structured failure code. */ }
      if (code === "document_type_confirmation_required") return;
      const message = `${draftStageLabels[stageId]}阶段${event.output.status === "cancelled" ? "已取消" : "未完成"}：${event.output.content}`;
      failureLoggedRef.current = true;
      appendGenerationLog({
        level: "error", event: "error", message, code, stage: stageId,
        providerId, pipeline: kindRef.current, sessionId: sessionRef.current, documentId: runBaseRef.current?.id,
      });
      if (event.recoverable) {
        warningsRef.current = [...warningsRef.current, { stageId, code: code ?? "stage_failed", message: event.output.content }];
        setWarnings(warningsRef.current);
        setText(lastBodyTextRef.current);
      } else {
        setFailedStage(stageId);
        setError(message);
      }
      return;
    }
    if (event.type === "completed") {
      // The stream's final event can arrive after the user opened another document or edited the
      // current one; it must not clobber either state.
      if (!appliedRef.current && canWriteBack(event.outcome.document.id)) {
        const current = useWorkbenchStore.getState().document;
        if (shouldApplyDraftOutcome(current.id, current.revision, event.outcome)) {
          applyOutcome(event.outcome);
        }
      }
    }
  }

  async function runPipeline(
    nextKind: DraftPipelineKind,
    start: (input: DraftStartInput, onEvent: (event: DraftEvent) => void) => Promise<DraftOutcome>,
    options: {
      requiresModelText?: boolean;
      confirmedDocType?: DocumentType | null;
      materialOverride?: string;
      templateIdOverride?: string | null;
    },
  ): Promise<void> {
    if (runningRef.current) return;
    if (!isDesktopRuntime()) {
      setError("模型调用仅在桌面应用中可用。");
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message: "当前不在桌面运行时，无法调用模型",
        pipeline: nextKind,
      });
      return;
    }
    // The wizard composes a brief and starts the run in the same tick, before React has
    // re-rendered the textarea, so it hands the material over explicitly.
    const materialText = options.materialOverride ?? material;
    if (!materialText.trim()) {
      const message = "请先填写写作材料，或从向导生成材料。";
      setError(message);
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message,
        providerId,
        pipeline: nextKind,
        documentId: document.id,
      });
      return;
    }
    if (!providerId) {
      const message = "请先选择一个可用的模型服务。";
      setError(message);
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message,
        pipeline: nextKind,
        documentId: document.id,
      });
      return;
    }
    if (options.requiresModelText && !modelText.trim()) {
      const message = "请先粘贴要模仿的范文。";
      setError(message);
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message,
        providerId,
        pipeline: nextKind,
        documentId: document.id,
      });
      return;
    }
    const rejection = providerRejection();
    if (rejection) {
      setError(rejection);
      appendGenerationLog({
        level: "error",
        event: "blocked",
        message: rejection,
        providerId,
        pipeline: nextKind,
        documentId: document.id,
      });
      return;
    }

    appendGenerationLog({
      level: "info",
      event: "start",
      message: `开始${nextKind === "draft" ? "起草" : nextKind === "imitate" ? "仿写" : "润色"}`,
      providerId,
      pipeline: nextKind,
      documentId: document.id,
    });
    runningRef.current = true;
    appliedRef.current = false;
    kindRef.current = nextKind;
    sessionRef.current = null;
    warningsRef.current = [];
    lastBodyTextRef.current = "";
    failureLoggedRef.current = false;
    setWarnings([]);
    setDrafting(true);
    setKind(nextKind);
    setStage(null);
    stageRef.current = null;
    setCompletedStages([]);
    setFailedStage(null);
    setText("");
    setError(null);
    setSessionId(null);
    if (nextKind !== "draft" || options.confirmedDocType == null) {
      setTypeDecision(null);
      setConfirmedDocType(null);
    }

    const targetDocumentId = useWorkbenchStore.getState().document.id;
    let acceptingEvents = true;
    try {
      const expected = JSON.stringify(useWorkbenchStore.getState().document);
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      if (JSON.stringify(snapshot) !== expected) {
        throw new IpcError("document_conflict", "准备运行时文档已变更，请基于最新内容重新发起。");
      }
      // Remember exactly what the pipeline saw: a completed event may only be written back
      // while the workbench still shows this id/revision.
      runBaseRef.current = structuredClone(snapshot);
      const outcome = await start(
        {
          documentId: snapshot.id,
          material: materialText.trim(),
          materialSecurityLevel,
          providerId,
          confirmedDocType: options.confirmedDocType ?? null,
          templateId: (options.templateIdOverride ?? templateId) || null,
          ...(options.requiresModelText ? { modelText: modelText.trim() } : {}),
        },
        (event) => { if (acceptingEvents) handleEvent(event); },
      );
      // Older runtimes may return the outcome without delivering the final channel event.
      const current = useWorkbenchStore.getState().document;
      if (!appliedRef.current && canWriteBack(outcome.document.id) && shouldApplyDraftOutcome(current.id, current.revision, outcome)) {
        applyOutcome(outcome);
      }
    } catch (runError) {
      const stillCurrent = useWorkbenchStore.getState().document.id === targetDocumentId;
      if (errorCode(runError) === "document_type_confirmation_required") {
        if (stillCurrent) setError(null);
        appendGenerationLog({
          level: "info",
          event: "blocked",
          message: "需要确认文种后才能继续",
          providerId,
          pipeline: nextKind,
        });
      } else {
        const message = ipcErrorMessage(runError);
        if (stillCurrent) {
          setError(message);
          if (isDraftStage(stageRef.current ?? "")) setFailedStage(stageRef.current);
        }
        if (!failureLoggedRef.current) appendGenerationLog({
          level: "error",
          event: "error",
          message,
          stage: stageRef.current,
          code: errorCode(runError),
          providerId,
          pipeline: nextKind,
          documentId: document.id,
          sessionId: sessionRef.current,
        });
      }
    } finally {
      acceptingEvents = false;
      runBaseRef.current = null;
      runningRef.current = false;
      setDrafting(false);
    }
  }

  async function startDraftRun(
    nextConfirmedDocType: DocumentType | null = null,
    materialOverride?: string,
    templateIdOverride?: string | null,
  ): Promise<void> {
    await runPipeline("draft", startDraft, {
      confirmedDocType: nextConfirmedDocType,
      materialOverride,
      templateIdOverride,
    });
  }

  async function startImitateRun(): Promise<void> {
    await runPipeline(
      "imitate",
      (input, onEvent) => startImitate({ ...input, modelText: input.modelText ?? "" }, onEvent),
      { requiresModelText: true },
    );
  }

  async function startPolishRun(): Promise<void> {
    await runPipeline("polish", startPolish, {});
  }

  async function rerun(stageId: DraftStageId): Promise<void> {
    if (runningRef.current || !sessionId) return;
    if (!isDesktopRuntime()) {
      setError("模型调用仅在桌面应用中可用。");
      return;
    }
    runningRef.current = true;
    appliedRef.current = false;
    failureLoggedRef.current = false;
    setDrafting(true);
    setStage(stageId);
    stageRef.current = stageId;
    setFailedStage(null);
    setError(null);
    appendGenerationLog({ level: "info", event: "start", message: `重跑${draftStageLabels[stageId]}阶段`, stage: stageId, sessionId, providerId, pipeline: kindRef.current });
    if (stageId === "draft" || stageId === "deai") setText("");

    const targetDocumentId = useWorkbenchStore.getState().document.id;
    let acceptingEvents = true;
    try {
      const expected = JSON.stringify(useWorkbenchStore.getState().document);
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      if (JSON.stringify(snapshot) !== expected) {
        throw new IpcError("document_conflict", "准备运行时文档已变更，请基于最新内容重新发起。");
      }
      runBaseRef.current = structuredClone(snapshot);
      const outcome = await rerunStage(sessionId, stageId, (event) => { if (acceptingEvents) handleEvent(event); });
      // The rerun streams only stage events, so the returned document is applied here; the
      // content guard keeps this idempotent if a future runtime also delivers a final event.
      const current = useWorkbenchStore.getState().document;
      if (canWriteBack(outcome.document.id) && shouldApplyRerunOutcome(current, outcome)) {
        onRerunOutcome(outcome, stageId);
        // Downstream stages ran against the previous output, so their records no longer hold.
        const order = stageOrder(kind);
        const rerunIndex = order.indexOf(stageId);
        setCompletedStages((currentStages) =>
          currentStages.filter((completed) => order.indexOf(completed) <= rerunIndex),
        );
        resetTransientState();
      }
    } catch (runError) {
      const stillCurrent = useWorkbenchStore.getState().document.id === targetDocumentId;
      if (errorCode(runError) === "document_type_confirmation_required") {
        if (stillCurrent) setError(null);
      } else {
        const message = ipcErrorMessage(runError);
        if (stillCurrent) {
          setError(message);
          if (isDraftStage(stageRef.current ?? "")) setFailedStage(stageRef.current);
        }
        if (!failureLoggedRef.current) appendGenerationLog({ level: "error", event: "error", message, code: errorCode(runError), stage: stageRef.current, sessionId, providerId, pipeline: kindRef.current });
      }
    } finally {
      acceptingEvents = false;
      runBaseRef.current = null;
      runningRef.current = false;
      setDrafting(false);
    }
  }

  return {
    drafting,
    kind,
    stage,
    completedStages,
    failedStage,
    sessionId,
    text,
    error,
    warnings,
    typeDecision,
    confirmedDocType,
    setConfirmedDocType,
    startDraft: startDraftRun,
    startImitate: startImitateRun,
    startPolish: startPolishRun,
    rerun,
  };
}
