// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { createDemoDocument } from "../../lib/document";
import { useWorkbenchStore } from "../../store/workbench";
import { DocumentPreview } from "./DocumentPreview";

describe("DocumentPreview unverified fact marks", () => {
  beforeEach(() => {
    const document = createDemoDocument();
    for (const block of document.body.blocks) {
      if (block.type === "paragraph") block.content = "购置15万元设备，追加预算15万元。";
    }
    useWorkbenchStore.setState({
      document,
      unverifiedTokens: { "block-request": ["15万元"] },
    });
  });

  it("wraps every occurrence of the unverified token in a red mark", () => {
    const { container } = render(<DocumentPreview />);
    const marks = container.querySelectorAll("mark.fact-unverified");
    expect(marks).toHaveLength(2);
    expect(marks[0].textContent).toBe("15万元");
    expect(marks[1].textContent).toBe("15万元");
  });

  it("renders no red marks when the fact-check report is empty", () => {
    useWorkbenchStore.setState({ unverifiedTokens: {} });
    const { container } = render(<DocumentPreview />);
    expect(container.querySelectorAll("mark.fact-unverified")).toHaveLength(0);
  });
});
