import { describe, expect, it } from "vitest";
import { PROVIDER_PRESETS, providerPresetInput } from "./providerPresets";

describe("provider presets", () => {
  it("keeps IDs unique and every preset usable as-is", () => {
    const ids = PROVIDER_PRESETS.map((preset) => preset.id);
    expect(new Set(ids).size).toBe(ids.length);

    for (const preset of PROVIDER_PRESETS) {
      expect(preset.displayName.trim()).not.toBe("");
      expect(preset.model.trim()).not.toBe("");
      expect(preset.baseUrl).toMatch(/^https?:\/\//);
      if (preset.group === "cloud") {
        expect(preset.baseUrl.startsWith("https://")).toBe(true);
      }
      if (preset.group === "local") {
        expect(preset.baseUrl).toMatch(/^http:\/\/(127\.0\.0\.1|localhost)/);
      }
    }
  });

  it("covers the common Chinese and international providers", () => {
    const names = PROVIDER_PRESETS.map((preset) => preset.displayName).join(" ");
    for (const provider of ["通义千问", "Kimi", "智谱", "混元", "千帆", "讯飞", "Gemini", "OpenRouter", "Groq"]) {
      expect(names).toContain(provider);
    }
  });

  it("fills a draft and makes the ID unique when it already exists", () => {
    const preset = PROVIDER_PRESETS[0];
    const first = providerPresetInput(preset, []);
    expect(first).toMatchObject({
      id: preset.id,
      kind: "openai_compatible",
      baseUrl: preset.baseUrl,
      model: preset.model,
      enabled: true,
    });

    const second = providerPresetInput(preset, [preset.id]);
    expect(second.id).toBe(`${preset.id}-2`);
    const third = providerPresetInput(preset, [preset.id, `${preset.id}-2`]);
    expect(third.id).toBe(`${preset.id}-3`);
  });
});
