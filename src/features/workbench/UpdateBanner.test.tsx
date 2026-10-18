// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { UpdateCheckResult } from "../../lib/ipc";
import { UpdateBanner } from "./UpdateBanner";

const update: UpdateCheckResult = {
  available: true,
  version: "0.1.7",
  currentVersion: "0.1.6",
  installed: false,
};

afterEach(() => cleanup());

describe("UpdateBanner", () => {
  it("shows the available and current versions", () => {
    render(<UpdateBanner update={update} installing={false} onInstall={vi.fn()} onDismiss={vi.fn()} />);
    expect(screen.getByText("发现新版本 v0.1.7")).toBeTruthy();
    expect(screen.getByText(/当前 v0\.1\.6/)).toBeTruthy();
  });

  it("installs only after the button is clicked", () => {
    const onInstall = vi.fn();
    render(<UpdateBanner update={update} installing={false} onInstall={onInstall} onDismiss={vi.fn()} />);

    expect(onInstall).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /立即更新/ }));
    expect(onInstall).toHaveBeenCalledTimes(1);
  });

  it("can be dismissed and disables actions while installing", () => {
    const onDismiss = vi.fn();
    const { rerender } = render(
      <UpdateBanner update={update} installing={false} onInstall={vi.fn()} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "稍后提醒" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);

    rerender(<UpdateBanner update={update} installing onInstall={vi.fn()} onDismiss={vi.fn()} />);
    expect((screen.getByRole("button", { name: /正在更新/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "稍后提醒" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
