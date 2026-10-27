// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createEmptyDocument } from "../../lib/document";
import { chatWithProvider } from "../../lib/ipc";
import { useWorkbenchStore } from "../../store/workbench";
import { RecipientClarification } from "./RecipientClarification";

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return { ...actual, chatWithProvider: vi.fn() };
});

const mockedChat = vi.mocked(chatWithProvider);

function response(recipients: string[], evidence: string | null, question: string | null = null) {
  return { content: JSON.stringify({ recipients, evidence, question }), finishReason: "stop", usage: null };
}

function renderQuestion() {
  const onDismiss = vi.fn();
  const saveDocument = vi.fn(async () => {});
  render(<RecipientClarification
    question="这份通知准备发给谁？"
    document={useWorkbenchStore.getState().document}
    providerId="local-model"
    saveDocument={saveDocument}
    onDismiss={onDismiss}
  />);
  return { onDismiss, saveDocument };
}

beforeEach(() => {
  mockedChat.mockReset();
  const document = createEmptyDocument();
  document.meta.docType = "notice";
  document.body.blocks = [{ type: "paragraph", id: "body-text", content: "已经生成的正文。" }];
  useWorkbenchStore.getState().setDocument(document, null);
});

afterEach(() => cleanup());

describe("RecipientClarification", () => {
  it("can be skipped without any provider request or document change", () => {
    const snapshot = structuredClone(useWorkbenchStore.getState().document);
    const { onDismiss, saveDocument } = renderQuestion();
    fireEvent.click(screen.getByRole("button", { name: "先留空，继续编辑" }));
    expect(onDismiss).toHaveBeenCalledOnce();
    expect(mockedChat).not.toHaveBeenCalled();
    expect(saveDocument).not.toHaveBeenCalled();
    expect(useWorkbenchStore.getState().document).toEqual(snapshot);
  });

  it("understands a reply and updates recipients without rewriting the body", async () => {
    mockedChat.mockResolvedValue(response(["各乡镇政府"], "发给各乡镇政府"));
    const snapshot = structuredClone(useWorkbenchStore.getState().document);
    const { onDismiss, saveDocument } = renderQuestion();
    fireEvent.change(screen.getByLabelText("补充说明"), { target: { value: "发给各乡镇政府" } });
    fireEvent.click(screen.getByRole("button", { name: "回复并更新" }));
    await waitFor(() => expect(onDismiss).toHaveBeenCalledOnce());
    expect(saveDocument).toHaveBeenCalledOnce();
    const updated = useWorkbenchStore.getState().document;
    expect(updated.body.mainRecipients).toEqual(["各乡镇政府"]);
    expect(updated.body.blocks).toEqual(snapshot.body.blocks);
    expect(updated.revision).toBe(snapshot.revision + 1);
  });

  it("asks another optional question when the answer is ambiguous", async () => {
    mockedChat.mockResolvedValue(response([], null, "是给市级部门，还是给各乡镇？"));
    const { onDismiss } = renderQuestion();
    fireEvent.change(screen.getByLabelText("补充说明"), { target: { value: "给下面单位" } });
    fireEvent.click(screen.getByRole("button", { name: "回复并更新" }));
    expect(await screen.findByText("是给市级部门，还是给各乡镇？")).toBeTruthy();
    expect(onDismiss).not.toHaveBeenCalled();
    expect(useWorkbenchStore.getState().document.body.mainRecipients).toEqual([]);
    expect((screen.getByRole("button", { name: "先留空，继续编辑" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("ignores a late reply after the user skips the question", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof chatWithProvider>>) => void;
    mockedChat.mockImplementation(() => new Promise((done) => { resolve = done; }));
    const { onDismiss } = renderQuestion();
    fireEvent.change(screen.getByLabelText("补充说明"), { target: { value: "发给市政府" } });
    fireEvent.click(screen.getByRole("button", { name: "回复并更新" }));
    await waitFor(() => expect(mockedChat).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "先留空，继续编辑" }));
    await act(async () => resolve(response(["市政府"], "发给市政府")));
    expect(onDismiss).toHaveBeenCalledOnce();
    expect(useWorkbenchStore.getState().document.body.mainRecipients).toEqual([]);
  });

  it("does not apply a late reply to another document", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof chatWithProvider>>) => void;
    mockedChat.mockImplementation(() => new Promise((done) => { resolve = done; }));
    renderQuestion();
    fireEvent.change(screen.getByLabelText("补充说明"), { target: { value: "发给市政府" } });
    fireEvent.click(screen.getByRole("button", { name: "回复并更新" }));
    await waitFor(() => expect(mockedChat).toHaveBeenCalledOnce());
    const other = createEmptyDocument();
    useWorkbenchStore.getState().setDocument(other, null);
    await act(async () => resolve(response(["市政府"], "发给市政府")));
    expect(useWorkbenchStore.getState().document).toEqual(other);
    expect(screen.getByRole("alert").textContent).toContain("正文不会被覆盖");
  });
});
