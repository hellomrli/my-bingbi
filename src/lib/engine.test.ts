// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import {
  draftStageLabels,
  draftStages,
  imitateStages,
  rerunStage,
  startDraft,
  startImitate,
} from "./engine";

it("does not run or rerun the draft pipeline in browser preview", async () => {
  await expect(
    startDraft(
      { documentId: "doc-browser", material: "申请事项", providerId: "ollama" },
      () => undefined,
    ),
  ).rejects.toMatchObject({ code: "model_runtime_unavailable" });
  await expect(
    startImitate(
      { documentId: "doc-browser", material: "申请事项", providerId: "ollama", modelText: "范文" },
      () => undefined,
    ),
  ).rejects.toMatchObject({ code: "model_runtime_unavailable" });
  await expect(
    rerunStage("session-browser", "deai", () => undefined),
  ).rejects.toMatchObject({ code: "model_runtime_unavailable" });
});

describe("draft stage identity", () => {
  it("labels every stage the kernel can report", () => {
    for (const stage of draftStages) {
      expect(draftStageLabels[stage]).toBeTruthy();
    }
  });

  it("keeps the canonical kernel order", () => {
    expect([...draftStages]).toEqual([
      "doctype",
      "facts",
      "retrieve",
      "outline",
      "draft",
      "deai",
      "lint",
      "factcheck",
      "score",
    ]);
  });

  it("inserts the style stage between retrieval and outline in imitate mode", () => {
    expect([...imitateStages]).toEqual([
      "doctype",
      "facts",
      "retrieve",
      "style",
      "outline",
      "draft",
      "deai",
      "lint",
      "factcheck",
      "score",
    ]);
    expect(draftStageLabels.style).toBeTruthy();
  });
});
