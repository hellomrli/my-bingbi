// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ClinePassSettings } from "./ClinePassSettings";

const mocks = vi.hoisted(() => ({
  overview: {
    configured: true,
    limits: [
      { kind: "five_hour", percentUsed: 42.4, resetsAt: "2026-10-18T12:00:00Z" },
      { kind: "weekly", percentUsed: 86, resetsAt: null },
    ],
    models: [
      {
        id: "cline-pass/glm-5.3",
        name: "GLM-5.3",
        contextWindow: 1_000_000,
        recommended: true,
        gateway: true,
      },
      {
        id: "cline-pass/kimi-k3",
        name: "Kimi K3",
        contextWindow: 1_048_576,
        recommended: true,
        gateway: false,
      },
    ],
    error: null,
  },
  clinePassOverview: vi.fn(),
  openClinePassSubscription: vi.fn(),
  hasProviderSecret: vi.fn(),
  setProviderSecret: vi.fn(),
  deleteProviderSecret: vi.fn(),
  saveProviderProfile: vi.fn(),
  listProviderProfiles: vi.fn(),
}));

vi.mock("../../lib/clinepass", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/clinepass")>();
  return {
    ...actual,
    clinePassOverview: mocks.clinePassOverview,
    openClinePassSubscription: mocks.openClinePassSubscription,
  };
});

vi.mock("../../lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/ipc")>();
  return {
    ...actual,
    hasProviderSecret: mocks.hasProviderSecret,
    setProviderSecret: mocks.setProviderSecret,
    deleteProviderSecret: mocks.deleteProviderSecret,
    saveProviderProfile: mocks.saveProviderProfile,
    listProviderProfiles: mocks.listProviderProfiles,
  };
});

beforeEach(() => {
  mocks.clinePassOverview.mockReset().mockResolvedValue(mocks.overview);
  mocks.openClinePassSubscription.mockReset().mockResolvedValue(undefined);
  mocks.hasProviderSecret.mockReset().mockResolvedValue(true);
  mocks.setProviderSecret.mockReset().mockResolvedValue(undefined);
  mocks.deleteProviderSecret.mockReset().mockResolvedValue(true);
  mocks.saveProviderProfile.mockReset().mockResolvedValue({});
  mocks.listProviderProfiles.mockReset().mockResolvedValue([
    {
      id: "cline-pass",
      kind: "openai_compatible",
      displayName: "Cline Pass",
      baseUrl: "https://api.cline.bot/api/v1",
      model: "cline-pass/glm-5.3",
      enabled: true,
      isLocal: false,
    },
  ]);
});

afterEach(() => cleanup());

describe("ClinePassSettings", () => {
  it("renders quota windows and the model catalog", async () => {
    render(<ClinePassSettings desktop />);

    expect(await screen.findByText("5 小时窗口")).toBeTruthy();
    expect(screen.getByText("42%")).toBeTruthy();
    expect(screen.getByText("本周")).toBeTruthy();
    expect(screen.getByText("86%")).toBeTruthy();
    expect(await screen.findAllByText("GLM-5.3")).not.toHaveLength(0);
    expect(screen.getByText("Kimi K3")).toBeTruthy();
    expect(screen.getByText("网关可用")).toBeTruthy();
    expect(screen.getByText("使用中")).toBeTruthy();
  });

  it("saves the key through the keyring boundary", async () => {
    render(<ClinePassSettings desktop />);
    await screen.findByText("5 小时窗口");

    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk_cline" } });
    fireEvent.click(screen.getByRole("button", { name: /保存 Key/ }));

    await waitFor(() => expect(mocks.setProviderSecret).toHaveBeenCalledWith("cline-pass", "sk_cline"));
    expect(await screen.findByText(/已保存到系统凭据库/)).toBeTruthy();
  });

  it("switches the provider to the selected model", async () => {
    render(<ClinePassSettings desktop />);
    await screen.findByText("Kimi K3");

    fireEvent.click(screen.getByRole("button", { name: "使用 Kimi K3" }));

    await waitFor(() =>
      expect(mocks.saveProviderProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          id: "cline-pass",
          model: "cline-pass/kimi-k3",
          enabled: true,
          baseUrl: "https://api.cline.bot/api/v1",
        }),
      ),
    );
    expect(await screen.findByText(/已切换到 Kimi K3/)).toBeTruthy();
  });

  it("opens the subscription page", async () => {
    render(<ClinePassSettings desktop />);
    await screen.findByText("5 小时窗口");

    fireEvent.click(screen.getByRole("button", { name: /打开订阅页/ }));
    await waitFor(() => expect(mocks.openClinePassSubscription).toHaveBeenCalledTimes(1));
  });

  it("explains that the panel is desktop-only in the browser", () => {
    render(<ClinePassSettings desktop={false} />);
    expect(screen.getByText(/桌面内核读取/)).toBeTruthy();
    expect(mocks.clinePassOverview).not.toHaveBeenCalled();
  });
});
