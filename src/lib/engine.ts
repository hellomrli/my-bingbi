import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  DraftEvent,
  DraftOutcome,
  RerunOutcome,
  Score,
} from "./generated/engine";

export type { RerunOutcome, Score };
import type { DocumentType, ODoc, SecurityLevel } from "./odoc";
import { IpcError, isDesktopRuntime } from "./ipc";

/** The persisted draft pipeline order used by both the engine and workbench UI. */
export const draftStages = [
  "doctype",
  "facts",
  "retrieve",
  "outline",
  "draft",
  "deai",
  "lint",
  "correct",
  "factcheck",
  "score",
] as const;

/** Imitate mode inserts the style stage between retrieval and outline. */
export const imitateStages = [
  "doctype",
  "facts",
  "retrieve",
  "style",
  "outline",
  "draft",
  "deai",
  "lint",
  "correct",
  "factcheck",
  "score",
] as const;

/** Polish mode rewrites the supplied material in place: decide, humanize, hard-check. */
export const polishStages = ["doctype", "deai", "lint", "correct", "factcheck"] as const;

export type DraftStageId =
  | (typeof draftStages)[number]
  | (typeof imitateStages)[number]
  | (typeof polishStages)[number];

/**
 * Whether a finished draft outcome should be written back into the workbench.
 *
 * The runtime delivers the outcome both through the stream's final event and as the command's
 * return value; the workbench applies the return value only when the document is the same one
 * that was drafted AND its revision is still behind the outcome's. Requiring the same id keeps
 * a draft that finished after the user switched documents from clobbering the now-active
 * document's unsaved edits; requiring the earlier revision keeps edits made while the pipeline
 * ran from being clobbered by a stale outcome (once applied, the revision has caught up and
 * the return value is a no-op).
 */
export function shouldApplyDraftOutcome(
  currentId: string,
  currentRevision: number,
  outcome: DraftOutcome,
): boolean {
  return currentId === outcome.document.id && currentRevision < outcome.document.revision;
}

/** Same write-back guard for single-stage re-runs, which must never touch another document. */
export function shouldApplyRerunOutcome(current: {
  id: string;
  revision: number;
}, outcome: RerunOutcome): boolean {
  return (
    current.id === outcome.document.id &&
    current.revision < outcome.document.revision &&
    JSON.stringify(current) !== JSON.stringify(outcome.document)
  );
}

/** Completed sessions retain their result even when applying it would overwrite a user edit. */
export function sessionOutcomeDocument(stateJson: string): ODoc | null {
  try {
    const candidate = JSON.parse(stateJson)?.document as Partial<ODoc> | undefined;
    if (candidate?.schemaVersion !== "0.1.0" || typeof candidate.id !== "string"
      || !Number.isInteger(candidate.revision) || !candidate.meta
      || !["public", "internal", "secret", "confidential"].includes(candidate.meta.securityLevel)
      || typeof candidate.body?.title?.text !== "string" || !Array.isArray(candidate.body.blocks)
      || !Array.isArray(candidate.facts?.facts)) return null;
    return candidate as ODoc;
  } catch {
    return null;
  }
}

export const draftStageLabels: Record<DraftStageId, string> = {
  doctype: "文种",
  facts: "事实",
  retrieve: "检索",
  style: "笔法",
  outline: "骨架",
  draft: "起草",
  deai: "降 AI 味",
  lint: "校验",
  correct: "校对",
  factcheck: "事实回查",
  score: "评分",
};

export * from "./generated/engine";

export interface DraftStartInput {
  documentId: string;
  material: string;
  materialSecurityLevel?: SecurityLevel;
  providerId: string;
  confirmedDocType?: DocumentType | null;
  /** Reference text for imitate mode; its presence selects the imitate pipeline. */
  modelText?: string | null;
  /** Knowledge-pack scenario template (`notice-meeting`); empty means the per-type default. */
  templateId?: string | null;
}

/** Starts the persisted desktop drafting pipeline and forwards ordered stage events. */
export async function startDraft(
  input: DraftStartInput,
  onEvent: (event: DraftEvent) => void,
): Promise<DraftOutcome> {
  if (!isDesktopRuntime()) {
    throw new IpcError("model_runtime_unavailable", "模型调用仅在桌面应用中可用。");
  }
  const channel = new Channel<DraftEvent>();
  channel.onmessage = onEvent;
  return invoke<DraftOutcome>("draft_start", {
    request: {
      documentId: input.documentId,
      material: input.material,
      materialSecurityLevel: input.materialSecurityLevel ?? null,
      providerId: input.providerId,
      confirmedDocType: input.confirmedDocType ?? null,
      modelText: input.modelText ?? null,
      templateId: input.templateId ?? null,
    },
    onEvent: channel,
  });
}

/**
 * Imitate mode: drafts a new document copying the writing style of the supplied reference
 * text. The kernel distills the style into a profile first, then drafts from the material.
 */
export async function startImitate(
  input: DraftStartInput & { modelText: string },
  onEvent: (event: DraftEvent) => void,
): Promise<DraftOutcome> {
  if (!isDesktopRuntime()) {
    throw new IpcError("model_runtime_unavailable", "模型调用仅在桌面应用中可用。");
  }
  return startDraft(input, onEvent);
}

/**
 * Polish mode: rewrites the supplied material in place with the humanizer pass, then runs the
 * hard lint and fact checks. The result replaces the current document body.
 */
export async function startPolish(
  input: DraftStartInput,
  onEvent: (event: DraftEvent) => void,
): Promise<DraftOutcome> {
  if (!isDesktopRuntime()) {
    throw new IpcError("model_runtime_unavailable", "模型调用仅在桌面应用中可用。");
  }
  const channel = new Channel<DraftEvent>();
  channel.onmessage = onEvent;
  return invoke<DraftOutcome>("draft_start", {
    request: {
      documentId: input.documentId,
      material: input.material,
      materialSecurityLevel: input.materialSecurityLevel ?? null,
      providerId: input.providerId,
      confirmedDocType: input.confirmedDocType ?? null,
      modelText: null,
      polish: true,
    },
    onEvent: channel,
  });
}

/**
 * Re-executes one draft stage from the input snapshot its session recorded.
 *
 * The kernel replays the same provider, knowledge pack and knowledge-base configuration the
 * stage originally ran with; the caller only names the session and the stage.
 */
export async function rerunStage(
  sessionId: string,
  stageId: DraftStageId,
  onEvent: (event: DraftEvent) => void,
): Promise<RerunOutcome> {
  if (!isDesktopRuntime()) {
    throw new IpcError("model_runtime_unavailable", "模型调用仅在桌面应用中可用。");
  }
  const channel = new Channel<DraftEvent>();
  channel.onmessage = onEvent;
  return invoke<RerunOutcome>("rerun_stage", {
    request: { sessionId, stageId },
    onEvent: channel,
  });
}
