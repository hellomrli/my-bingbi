import { useCallback, useEffect, useRef, useState } from "react";
import type { DraftEvent, DraftOutcome, RerunOutcome, TypeDecision } from "../lib/generated/engine";
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
import { ipcErrorMessage, isDesktopRuntime } from "../lib/ipc";
import type { ProviderProfile } from "../lib/llm";
import type { DocumentType, ODoc } from "../lib/odoc";
import { useWorkbenchStore } from "../store/workbench";

export type DraftPipelineKind = "draft" | "imitate" | "polish";

interface UseDraftPipelineOptions {
  document: ODoc;
  providers: ProviderProfile[];
  providerId: string;
  material: string;
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
  const [typeDecision, setTypeDecision] = useState<TypeDecision | null>(null);
  const [confirmedDocType, setConfirmedDocType] = useState<DocumentType | null>(null);
  // Catch blocks read the stage a failure died on; the render-time value would still be the
  // one from when the run started.
  const stageRef = useRef<DraftStageId | null>(null);
  // Identity + revision of the document the active run snapshotted. A finished run may only be
  // written back while the workbench still shows exactly that state, otherwise the user's edits
  // made during the run would be lost.
  const runBaseRef = useRef<{ id: string; revision: number } | null>(null);
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
    setDrafting(false);
    setCompletedStages([]);
    setSessionId(null);
    resetTransientState();
  }, [documentId, resetTransientState]);

  function providerRejection(): string | null {
    const selected = providers.find((provider) => provider.id === providerId);
    const restricted = isRestricted(document);
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
    if (base && current.id === base.id && current.revision === base.revision) return true;
    if (current.id === outcomeDocumentId) {
      setError("运行期间文档已被修改，结果未自动应用；请重新发起以使用最新内容。");
    }
    return false;
  }

  function applyOutcome(outcome: DraftOutcome) {
    setSessionId(outcome.sessionId);
    resetTransientState();
    onDraftOutcome(outcome);
  }

  function handleEvent(event: DraftEvent) {
    if (event.type === "stage_started") {
      if (isDraftStage(event.stageId)) {
        stageRef.current = event.stageId;
        setStage(event.stageId);
        setFailedStage(null);
        if (event.stageId === "draft" || event.stageId === "deai") setText("");
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
        }
      }
      return;
    }
    if (event.type === "completed") {
      // The stream's final event can arrive after the user opened another document or edited the
      // current one; it must not clobber either state.
      if (canWriteBack(event.outcome.document.id)) {
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
    if (drafting) return;
    if (!isDesktopRuntime()) {
      setError("模型调用仅在桌面应用中可用。");
      return;
    }
    // The wizard composes a brief and starts the run in the same tick, before React has
    // re-rendered the textarea, so it hands the material over explicitly.
    const materialText = options.materialOverride ?? material;
    if (!materialText.trim() || !providerId) return;
    if (options.requiresModelText && !modelText.trim()) return;
    const rejection = providerRejection();
    if (rejection) {
      setError(rejection);
      return;
    }

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

    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await start(
        {
          documentId: snapshot.id,
          material: materialText.trim(),
          providerId,
          confirmedDocType: options.confirmedDocType ?? null,
          templateId: (options.templateIdOverride ?? templateId) || null,
          ...(options.requiresModelText ? { modelText: modelText.trim() } : {}),
        },
        handleEvent,
      );
      // Older runtimes may return the outcome without delivering the final channel event.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyDraftOutcome(current.id, current.revision, outcome)) {
        applyOutcome(outcome);
      }
    } catch (runError) {
      if (errorCode(runError) === "document_type_confirmation_required") {
        setError(null);
      } else {
        setError(ipcErrorMessage(runError));
        if (isDraftStage(stageRef.current ?? "")) setFailedStage(stageRef.current);
      }
    } finally {
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
    if (drafting || !sessionId) return;
    if (!isDesktopRuntime()) {
      setError("模型调用仅在桌面应用中可用。");
      return;
    }
    setDrafting(true);
    setStage(stageId);
    stageRef.current = stageId;
    setFailedStage(null);
    setError(null);
    if (stageId === "draft" || stageId === "deai") setText("");

    try {
      const outcome = await rerunStage(sessionId, stageId, handleEvent);
      // The rerun streams only stage events, so the returned document is applied here; the
      // content guard keeps this idempotent if a future runtime also delivers a final event.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyRerunOutcome(current, outcome)) {
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
      if (errorCode(runError) === "document_type_confirmation_required") {
        setError(null);
      } else {
        setError(ipcErrorMessage(runError));
        if (isDraftStage(stageRef.current ?? "")) setFailedStage(stageRef.current);
      }
    } finally {
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
    typeDecision,
    confirmedDocType,
    setConfirmedDocType,
    startDraft: startDraftRun,
    startImitate: startImitateRun,
    startPolish: startPolishRun,
    rerun,
  };
}
