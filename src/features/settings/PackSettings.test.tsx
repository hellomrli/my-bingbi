// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { PackSettings } from "./PackSettings";

vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke: vi.fn() };
});

const mockedInvoke = vi.mocked(invoke);

const builtinPacks = [
  {
    schemaVersion: "1",
    id: "gb9704-format",
    name: "GB/T 9704 格式规范包",
    version: "1.0.0",
    description: "国标排版与文种模板",
    license: "MIT",
    source: "https://example.com/gb9704",
    docTypes: ["request", "notice", "report"],
    capabilities: ["format_rules", "templates", "style"] as const,
  },
  {
    schemaVersion: "1",
    id: "renmin-style",
    name: "人民日报语言包",
    version: "1.0.0",
    description: "反 AI 味与评分规则",
    license: "MIT",
    source: "https://example.com/renmin",
    docTypes: [],
    capabilities: ["language_rules", "checklists"] as const,
  },
  {
    schemaVersion: "1",
    id: "local-corpus",
    name: "本单位语料包",
    version: "0.1.0",
    description: "从 Obsidian 蒸馏的禁用语与笔法",
    license: "private",
    source: "",
    docTypes: [],
    capabilities: ["language_rules"] as const,
  },
];

afterEach(() => cleanup());

beforeEach(() => {
  mockedInvoke.mockReset();
  (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__ = {};
  mockedInvoke.mockImplementation(async (command: string) => {
    if (command === "list_pack_summaries") return builtinPacks;
    if (command === "user_pack_root") return "/home/test/.config/my-wordediter/packs";
    if (command === "pack_load_issues") return [];
    throw new Error(`unexpected command ${command}`);
  });
});

describe("PackSettings", () => {
  it("lists packs with core packs locked on", async () => {
    render(
      <PackSettings activePackIds={[]} onSave={vi.fn()} />,
    );
    expect(await screen.findByText("GB/T 9704 格式规范包")).toBeTruthy();
    expect(screen.getByText("人民日报语言包")).toBeTruthy();
    expect(screen.getByText("本单位语料包")).toBeTruthy();
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes).toHaveLength(3);
    // Core packs are locked and checked.
    const boxes = checkboxes as unknown as HTMLInputElement[];
    expect(boxes[0].checked).toBe(true);
    expect(boxes[0].disabled).toBe(true);
    expect(boxes[1].checked).toBe(true);
    expect(boxes[1].disabled).toBe(true);
    // The custom pack is off but switchable.
    expect(boxes[2].checked).toBe(false);
    expect(boxes[2].disabled).toBe(false);
  });

  it("saves a toggled custom pack with the new active ids", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<PackSettings activePackIds={["gb9704-format", "renmin-style"]} onSave={onSave} />);
    fireEvent.click(await screen.findByRole("checkbox", { name: /本单位语料包/ }));
    fireEvent.click(screen.getByRole("button", { name: "保存知识包设置" }));
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(["gb9704-format", "renmin-style", "local-corpus"]),
    );
    expect(await screen.findByText("知识包设置已保存")).toBeTruthy();
  });

  it("allows disabling the last optional pack while keeping core packs enabled", async () => {
    const onSave = vi.fn();
    render(<PackSettings activePackIds={["gb9704-format", "renmin-style", "local-corpus"]} onSave={onSave} />);
    const custom = await screen.findByRole("checkbox", { name: /本单位语料包/ });
    fireEvent.click(custom);
    expect((custom as HTMLInputElement).checked).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "保存知识包设置" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(["gb9704-format", "renmin-style"]));
  });

  it("shows the user pack directory and reports load issues", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_pack_summaries") return builtinPacks;
      if (command === "user_pack_root") return "/home/test/.config/my-wordediter/packs";
      if (command === "pack_load_issues") return ["pack rule 'x' is invalid", "duplicate pack id 'core-pack'"];
      throw new Error(`unexpected command ${command}`);
    });
    render(<PackSettings activePackIds={["gb9704-format", "renmin-style"]} onSave={vi.fn()} />);
    expect(await screen.findByText("/home/test/.config/my-wordediter/packs")).toBeTruthy();
    expect(screen.getByText(/用户包加载失败：pack rule 'x' is invalid/)).toBeTruthy();
    expect(screen.getByText(/用户包加载失败：duplicate pack id 'core-pack'/)).toBeTruthy();
  });
});
