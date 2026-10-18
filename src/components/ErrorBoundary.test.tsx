// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

let shouldThrow = true;

function Fragile() {
  if (shouldThrow) throw new Error("面板渲染失败");
  return <p>工作台已恢复</p>;
}

beforeEach(() => {
  shouldThrow = true;
  // React logs caught render errors; keep the test output readable.
  vi.spyOn(console, "error").mockImplementation(() => {});
  URL.createObjectURL = vi.fn(() => "blob:draft");
  URL.revokeObjectURL = vi.fn();
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ErrorBoundary", () => {
  it("shows a recovery screen instead of a blank window", () => {
    render(
      <ErrorBoundary>
        <Fragile />
      </ErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toBeTruthy();
    expect(screen.getByText("工作台遇到了未处理的错误")).toBeTruthy();
    expect(screen.getByText("面板渲染失败")).toBeTruthy();
    expect(screen.getByRole("button", { name: /导出草稿 JSON/ })).toBeTruthy();
  });

  it("exports the in-memory draft before the user reloads", () => {
    render(
      <ErrorBoundary>
        <Fragile />
      </ErrorBoundary>,
    );

    fireEvent.click(screen.getByRole("button", { name: /导出草稿 JSON/ }));
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledTimes(1);
  });

  it("retries rendering the workbench on demand", () => {
    render(
      <ErrorBoundary>
        <Fragile />
      </ErrorBoundary>,
    );
    shouldThrow = false;

    fireEvent.click(screen.getByRole("button", { name: /忽略错误并返回工作台/ }));
    expect(screen.getByText("工作台已恢复")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
