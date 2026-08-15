// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { gbFonts, probeGbFonts } from "./fonts";

afterEach(() => {
  vi.restoreAllMocks();
  delete (document as unknown as { fonts?: unknown }).fonts;
});

describe("probeGbFonts", () => {
  it("reports unsupported runtimes quietly", () => {
    expect(probeGbFonts()).toEqual({ supported: false, missing: [] });
  });

  it("lists every missing GB/T 9704 font", () => {
    const check = vi.fn((spec: string) => !spec.includes("仿宋_GB2312"));
    Object.defineProperty(document, "fonts", { value: { check }, configurable: true });
    const probe = probeGbFonts();
    expect(probe.supported).toBe(true);
    expect(probe.missing).toEqual(["仿宋_GB2312"]);
    expect(check).toHaveBeenCalledTimes(gbFonts.length);
  });

  it("reports no missing fonts when all are installed", () => {
    Object.defineProperty(document, "fonts", { value: { check: vi.fn(() => true) }, configurable: true });
    expect(probeGbFonts()).toEqual({ supported: true, missing: [] });
  });
});
