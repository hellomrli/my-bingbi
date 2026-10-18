import { describe, expect, it } from "vitest";
import { formatQuotaReset, quotaLabel } from "./clinepass";

describe("Cline Pass helpers", () => {
  it("labels the gateway quota windows", () => {
    expect(quotaLabel("five_hour")).toBe("5 小时窗口");
    expect(quotaLabel("weekly")).toBe("本周");
    expect(quotaLabel("monthly")).toBe("本月");
    expect(quotaLabel("custom")).toBe("custom");
  });

  it("formats reset timestamps and ignores invalid values", () => {
    expect(formatQuotaReset(null)).toBe("");
    expect(formatQuotaReset("not-a-date")).toBe("");
    expect(formatQuotaReset("2026-10-18T12:00:00Z")).not.toBe("");
  });
});
