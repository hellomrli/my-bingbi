import { Channel, invoke } from "@tauri-apps/api/core";
import type { ReviewDimension, ReviewEvent, ReviewOutcome } from "./generated/engine";
import { isDesktopRuntime } from "./ipc";

/** The persisted review pipeline order used by both the engine and workbench UI. */
export const reviewStages = ["doctype", "lint", "factcheck", "review", "score"] as const;

export type ReviewStageId = (typeof reviewStages)[number];

export const reviewStageLabels: Record<ReviewStageId, string> = {
  doctype: "文种复核",
  lint: "规则校验",
  factcheck: "事实回查",
  review: "语义评审",
  score: "质量评分",
};

export const reviewDimensionLabels: Record<ReviewDimension, string> = {
  format: "格式",
  content: "内容",
  language: "语言",
  logic: "逻辑",
  detail: "细节",
};

export interface ReviewStartInput {
  documentId: string;
  providerId: string;
  focus?: string | null;
}

export function isReviewStage(value: string): value is ReviewStageId {
  return value in reviewStageLabels;
}

/**
 * Starts a kernel review of the stored document and forwards ordered stage events.
 *
 * Reviewing reads the revision the kernel holds and never writes back, so the workbench keeps
 * whatever the user has on screen.
 */
export async function startReview(
  input: ReviewStartInput,
  onEvent: (event: ReviewEvent) => void,
): Promise<ReviewOutcome> {
  if (!isDesktopRuntime()) {
    throw {
      code: "model_runtime_unavailable",
      message: "公文审核仅在桌面应用中可用。",
    };
  }
  const channel = new Channel<ReviewEvent>();
  channel.onmessage = onEvent;
  return invoke<ReviewOutcome>("review_start", {
    request: {
      documentId: input.documentId,
      providerId: input.providerId,
      focus: input.focus?.trim() || null,
    },
    onEvent: channel,
  });
}
