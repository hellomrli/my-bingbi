// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createEmptyDocument } from "../../lib/document";
import { useWorkbenchStore } from "../../store/workbench";
import { FactLedger } from "./FactLedger";

afterEach(() => cleanup());

beforeEach(() => {
  useWorkbenchStore.setState(useWorkbenchStore.getInitialState(), true);
  const document = createEmptyDocument();
  document.facts.facts.push({
    id: "model-fact",
    kind: "data",
    value: "15",
    unit: "万元",
    role: null,
    source: "model_generated",
    sourceRef: "",
    verified: false,
  });
  useWorkbenchStore.getState().setDocument(document, null);
});

describe("FactLedger", () => {
  it("shows model provenance and requires a human check after editing a value", () => {
    render(<FactLedger />);
    expect(screen.getByText("来源：模型提取（未经人工核实）")).toBeTruthy();
    const value = screen.getAllByLabelText("事实值")[0] as HTMLInputElement;
    fireEvent.change(value, { target: { value: "16" } });
    expect(useWorkbenchStore.getState().document.facts.facts[0]).toMatchObject({ value: "16", verified: false });
    fireEvent.click(screen.getAllByRole("checkbox", { name: "已对照材料人工核实" })[0]);
    expect(useWorkbenchStore.getState().document.facts.facts[0].verified).toBe(true);
  });

  it("adds manually confirmed facts with a source and removes them", () => {
    render(<FactLedger />);
    const valueFields = screen.getAllByLabelText("事实值") as HTMLInputElement[];
    fireEvent.change(valueFields[1], { target: { value: "2026年9月30日" } });
    fireEvent.change(screen.getAllByLabelText("核实依据 / 来源")[1], { target: { value: "批復文件" } });
    fireEvent.click(screen.getAllByRole("checkbox", { name: "已对照材料人工核实" })[1]);
    fireEvent.click(screen.getByRole("button", { name: "加入事实台账" }));

    const added = useWorkbenchStore.getState().document.facts.facts.at(-1);
    expect(added).toMatchObject({ value: "2026年9月30日", sourceRef: "批復文件", verified: true });
    fireEvent.click(screen.getByRole("button", { name: `移除事实 ${added!.value}` }));
    expect(useWorkbenchStore.getState().document.facts.facts).toHaveLength(1);
  });
});
