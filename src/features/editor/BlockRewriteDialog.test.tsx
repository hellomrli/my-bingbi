// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createEmptyDocument } from "../../lib/document";
import { BlockRewriteDialog } from "./BlockRewriteDialog";

const mocks = vi.hoisted(() => ({ chatWithProvider: vi.fn() }));

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return { ...actual, chatWithProvider: mocks.chatWithProvider };
});

function setup(original = "拟购置排水设备 3 台。") {
  const document = createEmptyDocument();
  document.body.title.text = "关于购置设备的请示";
  document.body.blocks = [{ type: "paragraph", id: "b1", content: original }];
  const onApply = vi.fn();
  const onClose = vi.fn();
  render(
    <BlockRewriteDialog
      open
      blockId="b1"
      original={original}
      document={document}
      providerId="ollama"
      providerName="本地 Ollama"
      onApply={onApply}
      onClose={onClose}
    />,
  );
  return { onApply, onClose };
}

beforeEach(() => {
  mocks.chatWithProvider.mockReset();
});

afterEach(() => cleanup());

describe("BlockRewriteDialog", () => {
  it("generates a rewrite and applies it after the user reviews it", async () => {
    mocks.chatWithProvider.mockResolvedValue({
      content: "拟购置排水设备 3 台，尽快完成采购。",
      finishReason: "stop",
      usage: null,
    });
    const { onApply } = setup();

    fireEvent.click(screen.getByRole("button", { name: /生成改写/ }));
    await waitFor(() =>
      expect((screen.getByLabelText("改写结果") as HTMLTextAreaElement).value).toContain("尽快完成采购"),
    );

    expect(mocks.chatWithProvider).toHaveBeenCalledWith(
      "ollama",
      expect.any(String),
      expect.objectContaining({ jsonMode: false }),
    );
    fireEvent.click(screen.getByRole("button", { name: /应用到本段/ }));
    expect(onApply).toHaveBeenCalledWith("b1", expect.stringContaining("尽快完成采购"));
  });

  it("blocks applying when the rewrite changes numbers until the user confirms", async () => {
    mocks.chatWithProvider.mockResolvedValue({
      content: "拟购置排水设备 4 台。",
      finishReason: "stop",
      usage: null,
    });
    const { onApply } = setup();

    fireEvent.click(screen.getByRole("button", { name: /生成改写/ }));
    await screen.findByText(/数字、日期或文号发生了变化/);
    expect(screen.getByText(/新增：4台/)).toBeTruthy();

    const apply = screen.getByRole("button", { name: /应用到本段/ }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    fireEvent.click(apply);
    expect(onApply).not.toHaveBeenCalled();

    fireEvent.click(screen.getByLabelText(/我已核对上述变化/));
    expect(apply.disabled).toBe(false);
    fireEvent.click(apply);
    expect(onApply).toHaveBeenCalledWith("b1", "拟购置排水设备 4 台。");
  });

  it("reports model failures without touching the document", async () => {
    mocks.chatWithProvider.mockRejectedValue({ code: "provider_error", message: "模型连接失败" });
    const { onApply } = setup();

    fireEvent.click(screen.getByRole("button", { name: /生成改写/ }));
    expect(await screen.findByText("模型连接失败")).toBeTruthy();
    expect(onApply).not.toHaveBeenCalled();
  });
});
