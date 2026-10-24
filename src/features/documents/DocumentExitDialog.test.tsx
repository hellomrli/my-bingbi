// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DocumentExitDialog } from "./DocumentExitDialog";

afterEach(() => cleanup());

describe("DocumentExitDialog", () => {
  it("lets the writer save, discard or cancel", () => {
    const onChoice = vi.fn();
    render(<DocumentExitDialog open prompt failure={null} onChoice={onChoice} />);

    fireEvent.click(screen.getByRole("button", { name: "保存并继续" }));
    fireEvent.click(screen.getByRole("button", { name: "不保存并继续" }));
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(onChoice.mock.calls).toEqual([["save"], ["discard"], ["cancel"]]);
  });

  it("keeps a failed save visible and still allows leaving deliberately", () => {
    const onChoice = vi.fn();
    render(<DocumentExitDialog open prompt failure="磁盘空间不足" onChoice={onChoice} />);

    expect(screen.getByRole("alert").textContent).toContain("磁盘空间不足");
    fireEvent.click(screen.getByRole("button", { name: "保存并继续" }));
    expect(onChoice).toHaveBeenCalledWith("save");
  });

  it("shows progress without dismissal buttons while the exit is running", () => {
    render(<DocumentExitDialog open prompt={false} failure={null} onChoice={() => {}} />);

    expect(screen.getByText("正在完成退出操作")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });
});
