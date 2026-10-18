// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createDemoDocument } from "../../lib/document";
import { useWorkbenchStore } from "../../store/workbench";
import { StructureEditor } from "./StructureEditor";

afterEach(() => cleanup());

describe("StructureEditor unverified fact badge", () => {
  beforeEach(() => {
    const document = createDemoDocument();
    useWorkbenchStore.setState({
      document,
      unverifiedTokens: { "block-request": ["15万元"], "block-intro": [] },
    });
  });

  it("shows a red badge listing the unverified tokens of each affected block", () => {
    render(<StructureEditor />);
    const badges = screen.getAllByText(/待核实：/);
    expect(badges).toHaveLength(1);
    expect(badges[0].textContent).toContain("15万元");
  });

  it("lists headings in the outline and jumps to the clicked one", () => {
    render(<StructureEditor />);
    expect(screen.getByText(/文档大纲 · 3 个标题/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "现有装备情况" }));
    expect(useWorkbenchStore.getState().selectedNodeId).toBe("block-h1-current");
  });

  it("disables inline rewriting until a model is ready", () => {
    const { rerender } = render(<StructureEditor />);
    expect((screen.getByRole("button", { name: "AI 改写第 2 段" }) as HTMLButtonElement).disabled).toBe(true);

    rerender(<StructureEditor providerId="ollama" providerName="本地 Ollama" modelReady />);
    expect((screen.getByRole("button", { name: "AI 改写第 2 段" }) as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "AI 改写第 2 段" }));
    expect(screen.getByRole("dialog", { name: /AI 改写本段/ })).toBeTruthy();
  });
});
