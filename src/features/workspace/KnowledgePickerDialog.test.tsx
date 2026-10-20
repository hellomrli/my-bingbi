// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { kbGet, kbSearch } from "../../lib/ipc";
import { KnowledgePickerDialog } from "./KnowledgePickerDialog";

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return { ...actual, kbGet: vi.fn(), kbSearch: vi.fn() };
});

const mockedKbGet = vi.mocked(kbGet);
const mockedKbSearch = vi.mocked(kbSearch);

afterEach(() => cleanup());

beforeEach(() => {
  mockedKbGet.mockReset();
  mockedKbSearch.mockReset().mockResolvedValue([]);
});

describe("KnowledgePickerDialog document preparation", () => {
  it("waits for the current document save before searching the knowledge base", async () => {
    let finishSave!: (documentId: string) => void;
    const prepareDocument = vi.fn(() => new Promise<string>((resolve) => { finishSave = resolve; }));
    render(<KnowledgePickerDialog open prepareDocument={prepareDocument} onClose={vi.fn()} onInsert={vi.fn()} />);

    fireEvent.change(screen.getByLabelText("搜索知识库"), { target: { value: "防汛" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));

    expect(prepareDocument).toHaveBeenCalledTimes(1);
    expect(mockedKbSearch).not.toHaveBeenCalled();
    finishSave("saved-doc");

    await waitFor(() => expect(mockedKbSearch).toHaveBeenCalledTimes(1));
    expect(mockedKbSearch.mock.calls[0][1]).toBe("saved-doc");
  });

  it("does not issue a search or fetch a note when saving the document fails", async () => {
    const prepareDocument = vi.fn().mockRejectedValue(new Error("文档保存失败"));
    render(<KnowledgePickerDialog open prepareDocument={prepareDocument} onClose={vi.fn()} onInsert={vi.fn()} />);

    fireEvent.change(screen.getByLabelText("搜索知识库"), { target: { value: "防汛" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(mockedKbSearch).not.toHaveBeenCalled();
    expect(mockedKbGet).not.toHaveBeenCalled();
  });
});
