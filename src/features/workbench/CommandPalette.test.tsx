// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CommandPalette, type CommandAction } from "./CommandPalette";

const actions: CommandAction[] = [
  { id: "save", label: "保存到本机", hint: "Ctrl+S", keywords: ["save"], run: vi.fn() },
  { id: "wizard", label: "向导模式", hint: "逐步收集要素", keywords: ["wizard"], run: vi.fn() },
  { id: "export", label: "导出 DOCX", run: vi.fn() },
];

beforeEach(() => {
  for (const action of actions) vi.mocked(action.run).mockClear();
});

afterEach(() => cleanup());

describe("CommandPalette", () => {
  it("filters actions and runs the active one with Enter", () => {
    render(<CommandPalette open actions={actions} onClose={() => {}} />);
    const input = screen.getByLabelText("搜索命令");

    fireEvent.change(input, { target: { value: "向导" } });
    expect(screen.queryByText("保存到本机")).toBeNull();
    expect(screen.getByText("向导模式")).toBeTruthy();

    fireEvent.keyDown(input, { key: "Enter" });
    expect(actions[1].run).toHaveBeenCalledTimes(1);
  });

  it("moves the selection with the arrow keys", () => {
    render(<CommandPalette open actions={actions} onClose={() => {}} />);
    const input = screen.getByLabelText("搜索命令");

    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(actions[1].run).toHaveBeenCalledTimes(1);
  });

  it("does not run disabled actions", () => {
    const disabled = [{ ...actions[0], disabled: true }];
    render(<CommandPalette open actions={disabled} onClose={() => {}} />);

    fireEvent.click(screen.getByText("保存到本机"));
    expect(actions[0].run).not.toHaveBeenCalled();
  });

  it("shows an empty state for an unmatched query", () => {
    render(<CommandPalette open actions={actions} onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText("搜索命令"), { target: { value: "不存在" } });
    expect(screen.getByText("没有匹配的命令")).toBeTruthy();
  });
});
