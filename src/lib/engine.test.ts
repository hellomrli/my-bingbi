// @vitest-environment jsdom

import { expect, it } from "vitest";
import { startDraft } from "./engine";

it("does not run the persisted drafting pipeline in browser preview", async () => {
  await expect(
    startDraft(
      {
        documentId: "doc-browser",
        material: "申请事项",
        providerId: "ollama",
      },
      () => undefined,
    ),
  ).rejects.toMatchObject({ code: "model_runtime_unavailable" });
});
