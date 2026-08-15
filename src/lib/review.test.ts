// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { isReviewStage, reviewStageLabels, reviewStages, startReview } from "./review";

it("does not run the review pipeline in browser preview", async () => {
  await expect(
    startReview({ documentId: "doc-browser", providerId: "ollama" }, () => undefined),
  ).rejects.toMatchObject({ code: "model_runtime_unavailable" });
});

describe("review stage identity", () => {
  it("labels every stage the kernel can report", () => {
    for (const stage of reviewStages) {
      expect(reviewStageLabels[stage]).toBeTruthy();
    }
  });

  it("keeps the canonical kernel order", () => {
    expect([...reviewStages]).toEqual(["doctype", "lint", "factcheck", "review", "score"]);
  });

  it("rejects drafting stages the review pipeline never runs", () => {
    expect(isReviewStage("review")).toBe(true);
    expect(isReviewStage("draft")).toBe(false);
    expect(isReviewStage("deai")).toBe(false);
  });
});
