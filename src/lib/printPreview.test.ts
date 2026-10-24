// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { printPreviewHtml } from "./printPreview";

function mountedFrame(): HTMLIFrameElement {
  const frame = document.querySelector<HTMLIFrameElement>('iframe[title="公文打印预览"]');
  if (!frame) throw new Error("print preview frame was not mounted");
  return frame;
}

afterEach(() => {
  vi.useRealTimers();
  document.querySelectorAll("iframe").forEach((frame) => frame.remove());
});

describe("printPreviewHtml", () => {
  it("prints the rendered document and resolves after it loads", async () => {
    const result = printPreviewHtml("<p>正文</p>");
    const frame = mountedFrame();
    const print = vi.fn();
    Object.defineProperty(frame.contentWindow!, "print", { value: print, configurable: true });
    Object.defineProperty(frame.contentWindow!, "focus", { value: vi.fn(), configurable: true });

    frame.dispatchEvent(new Event("load"));

    await expect(result).resolves.toBeUndefined();
    expect(print).toHaveBeenCalledTimes(1);
    expect(frame.getAttribute("sandbox")).toBe("allow-same-origin allow-modals");
  });

  it("reports a load failure and removes the hidden frame", async () => {
    const result = printPreviewHtml("<p>正文</p>");
    const frame = mountedFrame();

    frame.dispatchEvent(new Event("error"));

    await expect(result).rejects.toThrow("无法加载打印预览。");
    expect(document.querySelector('iframe[title="公文打印预览"]')).toBeNull();
  });

});
