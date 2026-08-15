// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { createDemoDocument } from "../../lib/document";
import { useWorkbenchStore } from "../../store/workbench";
import { StructureEditor } from "./StructureEditor";

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
});
