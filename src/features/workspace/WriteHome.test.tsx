// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DraftReference } from "../../lib/materialReferences";
import { WriteHome } from "./WriteHome";

const references: DraftReference[] = [
  { id: "ref-1", kind: "上传文件", label: "检查提纲.txt", content: "这是一段不应该出现在主输入框里的文件内容。" },
];

function renderHome() {
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
      documentId="doc-1"
      kbEnabled
      currentTitle=""
      onContinueDocument={vi.fn()}
      references={references}
      onAddReferences={vi.fn()}
      onRemoveReference={vi.fn()}
    />,
  );
}

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
});
