// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import {
  appendGenerationLog,
  clearGenerationLogs,
  formatGenerationLogs,
  readGenerationLogs,
  subscribeGenerationLogs,
} from "./generationLog";

beforeEach(() => {
  localStorage.clear();
});

describe("generation log", () => {
  it("stores entries newest first and formats them for copying", () => {
    appendGenerationLog({ level: "info", event: "start", message: "开始起草", providerId: "ollama" });
    appendGenerationLog({ level: "error", event: "error", message: "模型连接失败", stage: "draft", code: "provider_error" });

    const entries = readGenerationLogs();
    expect(entries).toHaveLength(2);
    expect(entries[0].message).toBe("模型连接失败");
    expect(entries[0].code).toBe("provider_error");
    expect(entries[1].message).toBe("开始起草");
    const text = formatGenerationLogs(entries);
    expect(text).toContain("ERROR error draft 模型连接失败 [provider_error]");
  });

  it("caps the log at 100 entries", () => {
    for (let index = 0; index < 130; index += 1) {
      appendGenerationLog({ level: "info", event: "stage", message: `stage-${index}` });
    }
    const entries = readGenerationLogs();
    expect(entries).toHaveLength(100);
    expect(entries[0].message).toBe("stage-129");
  });

  it("notifies subscribers and can be cleared", () => {
    let notifications = 0;
    const unsubscribe = subscribeGenerationLogs(() => {
      notifications += 1;
    });
    appendGenerationLog({ level: "info", event: "start", message: "开始" });
    clearGenerationLogs();
    unsubscribe();
    expect(notifications).toBe(2);
    expect(readGenerationLogs()).toEqual([]);
  });
});
