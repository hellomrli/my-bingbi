// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { SessionHistoryDialog } from "./SessionHistoryDialog";

vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke: vi.fn() };
});

const mockedInvoke = vi.mocked(invoke);

afterEach(() => cleanup());

beforeEach(() => {
  mockedInvoke.mockReset();
  (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__ = {};
});

function renderDialog() {
  return render(
    <SessionHistoryDialog
      open
      documentId="doc-1"
      title="设备经费请示"
      onClose={() => undefined}
    />,
  );
}

describe("SessionHistoryDialog cost panel", () => {
  it("shows an empty state when the document has no drafting sessions", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_sessions") return [];
      throw new Error(`unexpected command ${command}`);
    });
    renderDialog();
    expect(await screen.findByText("「设备经费请示」还没有起草记录")).toBeTruthy();
  });

  it("lists sessions with pipeline label and token totals", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_sessions") {
        return [
          {
            id: "session-1",
            documentId: "doc-1",
            pipeline: "imitate",
            stateJson: "{}",
            costTokens: 1234,
            createdAt: 1_000_000,
            updatedAt: 1_753_900_000_000,
          },
          {
            id: "session-2",
            documentId: "doc-1",
            pipeline: "draft",
            stateJson: "{}",
            costTokens: 66,
            createdAt: 1_000_000,
            updatedAt: 1_753_800_000_000,
          },
        ];
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderDialog();
    expect(await screen.findByText("仿写")).toBeTruthy();
    expect(screen.getByText("起草")).toBeTruthy();
    expect(screen.getByText("1,234 tokens")).toBeTruthy();
    expect(screen.getByText("2 次 · 共 1,300 tokens")).toBeTruthy();
  });

  it("loads and shows the stage breakdown of an expanded session", async () => {
    mockedInvoke.mockImplementation(async (command: string, args?: unknown) => {
      const params = args as Record<string, unknown> | undefined;
      if (command === "list_sessions") {
        return [
          {
            id: "session-1",
            documentId: "doc-1",
            pipeline: "draft",
            stateJson: "{}",
            costTokens: 25,
            createdAt: 1_000_000,
            updatedAt: 1_753_900_000_000,
          },
        ];
      }
      if (command === "list_stage_runs" && params?.sessionId === "session-1") {
        return [
          {
            id: 1,
            sessionId: "session-1",
            stageId: "doctype",
            inputJson: "{}",
            outputJson: "{}",
            provider: "deepseek",
            tokens: 12,
            durationMs: 340,
            createdAt: 1_753_900_000_000,
          },
          {
            id: 2,
            sessionId: "session-1",
            stageId: "draft",
            inputJson: "{}",
            outputJson: "{}",
            provider: "deepseek",
            tokens: 13,
            durationMs: 2100,
            createdAt: 1_753_900_000_000,
          },
        ];
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderDialog();
    fireEvent.click(await screen.findByRole("button", { name: /展开 起草 记录的阶段明细/ }));
    await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("list_stage_runs", { sessionId: "session-1" }));
    expect(await screen.findByText("文种")).toBeTruthy();
    expect(screen.getAllByText("deepseek", { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getAllByText("12").length).toBeGreaterThan(0);
  });

  it("falls back to the raw stage id when a stage label is unknown", async () => {
    mockedInvoke.mockImplementation(async (command: string, args?: unknown) => {
      const params = args as Record<string, unknown> | undefined;
      if (command === "list_sessions") {
        return [
          {
            id: "session-1",
            documentId: "doc-1",
            pipeline: "draft",
            stateJson: "{}",
            costTokens: 0,
            createdAt: 1_000_000,
            updatedAt: 1_753_900_000_000,
          },
        ];
      }
      if (command === "list_stage_runs") {
        return [
          {
            id: 1,
            sessionId: "session-1",
            stageId: "custom-stage",
            inputJson: "{}",
            outputJson: "{}",
            provider: null,
            tokens: 3,
            durationMs: 10,
            createdAt: 1_753_900_000_000,
          },
        ];
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderDialog();
    fireEvent.click(await screen.findByRole("button", { name: /展开 起草 记录的阶段明细/ }));
    expect(await screen.findByText("custom-stage")).toBeTruthy();
  });
});
