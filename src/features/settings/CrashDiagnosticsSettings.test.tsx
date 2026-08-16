// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CrashDiagnosticsSettings } from "./CrashDiagnosticsSettings";

const ipc = vi.hoisted(() => ({
  listCrashEvents: vi.fn(),
  clearCrashEvents: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  listCrashEvents: ipc.listCrashEvents,
  clearCrashEvents: ipc.clearCrashEvents,
  ipcErrorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}));

const event = {
  id: 7,
  kind: "rust_panic",
  message: "index out of bounds",
  contextJson: JSON.stringify({
    appVersion: "0.1.1",
    os: "linux",
    arch: "x86_64",
    thread: "main",
    file: "src/lib.rs",
    line: 42,
    column: 9,
  }),
  createdAt: Date.UTC(2026, 7, 16, 3, 20, 0),
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

beforeEach(() => {
  ipc.listCrashEvents.mockReset().mockResolvedValue([event]);
  ipc.clearCrashEvents.mockReset().mockResolvedValue(1);
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

describe("CrashDiagnosticsSettings", () => {
  it("loads and renders local panic diagnostics", async () => {
    render(<CrashDiagnosticsSettings enabled desktop onEnabledChange={vi.fn()} />);

    expect(await screen.findByText("保留了最近 1 条记录")).toBeTruthy();
    expect(screen.getByText("index out of bounds")).toBeTruthy();
    expect(screen.getByText(/v0\.1\.1 · linux\/x86_64 · 线程 main · src\/lib\.rs:42:9/)).toBeTruthy();
    expect(ipc.listCrashEvents).toHaveBeenCalledWith(20);
  });

  it("reports toggle changes to the settings draft", async () => {
    const onEnabledChange = vi.fn();
    render(<CrashDiagnosticsSettings enabled={false} desktop onEnabledChange={onEnabledChange} />);

    fireEvent.click(screen.getByRole("checkbox", { name: /本地崩溃诊断/ }));
    expect(onEnabledChange).toHaveBeenCalledWith(true);
  });

  it("clears retained diagnostics after confirmation", async () => {
    render(<CrashDiagnosticsSettings enabled desktop onEnabledChange={vi.fn()} />);
    await screen.findByText("index out of bounds");

    fireEvent.click(screen.getByRole("button", { name: "清空诊断记录" }));
    await waitFor(() => expect(ipc.clearCrashEvents).toHaveBeenCalledOnce());
    expect(await screen.findByText("已清空 1 条本地诊断记录")).toBeTruthy();
    expect(screen.queryByText("index out of bounds")).toBeNull();
  });

  it("does not access desktop diagnostics in browser preview", () => {
    render(<CrashDiagnosticsSettings enabled={false} desktop={false} onEnabledChange={vi.fn()} />);

    expect(screen.getByText("浏览器预览不提供内核诊断")).toBeTruthy();
    expect((screen.getByRole("checkbox", { name: /本地崩溃诊断/ }) as HTMLInputElement).disabled).toBe(true);
    expect(ipc.listCrashEvents).not.toHaveBeenCalled();
  });
});
