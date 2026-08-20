// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WelcomeGuide } from "./WelcomeGuide";

vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/core")>();
  return { ...actual, invoke: vi.fn() };
});

const mockedInvoke = vi.mocked(invoke);

const ollamaProfile = {
  id: "ollama",
  kind: "ollama",
  displayName: "本地 Ollama",
  baseUrl: "http://127.0.0.1:11434",
  model: "qwen2.5:7b",
  enabled: true,
  isLocal: true,
};

afterEach(() => cleanup());

beforeEach(() => {
  mockedInvoke.mockReset();
  (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__ = {};
});

function renderGuide(onComplete = vi.fn(), onOpenSettings = vi.fn(), onSelectProvider = vi.fn()) {
  return {
    onComplete,
    onOpenSettings,
    onSelectProvider,
    ...render(<WelcomeGuide open onComplete={onComplete} onOpenSettings={onOpenSettings} onSelectProvider={onSelectProvider} />),
  };
}

describe("WelcomeGuide", () => {
  it("walks through welcome, provider selection and completion", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") return [ollamaProfile];
      throw new Error(`unexpected command ${command}`);
    });
    const { onComplete } = renderGuide();
    expect(screen.getByText("公文写作工作台")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /下一步/ }));
    expect(await screen.findByText("本地 Ollama")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /下一步/ }));
    expect(screen.getByText("准备就绪")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /开始使用/ }));
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("offers the settings route when no provider is enabled", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return [{ ...ollamaProfile, enabled: false }];
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { onOpenSettings } = renderGuide();
    fireEvent.click(screen.getByRole("button", { name: /下一步/ }));
    expect(await screen.findByText("去设置中添加")).toBeTruthy();
    // The primary action stays disabled without an enabled provider.
    expect((screen.getByRole("button", { name: /下一步/ }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "去设置中添加" }));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it("lets the user pick a provider: the radio is controlled and wires the choice up", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_provider_profiles") {
        return [
          ollamaProfile,
          { ...ollamaProfile, id: "deepseek", displayName: "DeepSeek", isLocal: false },
        ];
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { onSelectProvider } = renderGuide();
    fireEvent.click(screen.getByRole("button", { name: /下一步/ }));
    await screen.findByText("本地 Ollama");

    const radios = screen.getAllByRole("radio") as HTMLInputElement[];
    // The first enabled provider is pre-selected.
    expect(radios[0].checked).toBe(true);
    expect(radios[1].checked).toBe(false);

    fireEvent.click(screen.getByText("DeepSeek"));
    expect(radios[0].checked).toBe(false);
    expect(radios[1].checked).toBe(true);
    expect(onSelectProvider).toHaveBeenCalledWith("deepseek");
  });
});
