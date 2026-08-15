// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import {
  draftStageLabels,
  draftStages,
  imitateStages,
  polishStages,
  rerunStage,
  startDraft,
  startImitate,
  startPolish,
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
    startPolish(
      { documentId: "doc-browser", material: "为进一步贯彻落实上级精神", providerId: "ollama" },
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

  it("keeps the polish pipeline to decide, humanize and hard-check", () => {
    expect([...polishStages]).toEqual(["doctype", "deai", "lint", "factcheck"]);
    for (const stage of polishStages) {
      expect(draftStageLabels[stage]).toBeTruthy();
    }
  });
});
