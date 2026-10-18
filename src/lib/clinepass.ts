import { invoke } from "@tauri-apps/api/core";
import { IpcError, isDesktopRuntime } from "./ipc";

export const CLINE_PASS_PROVIDER_ID = "cline-pass";
export const CLINE_PASS_BASE_URL = "https://api.cline.bot/api/v1";
export const CLINE_PASS_SUBSCRIPTION_URL = "https://cline.bot/cline-pass";

export interface ClinePassQuota {
  kind: string;
  percentUsed: number;
  resetsAt: string | null;
}

export interface ClinePassModel {
  id: string;
  name: string;
  contextWindow: number | null;
  recommended: boolean;
  gateway: boolean;
}

export interface ClinePassOverview {
  configured: boolean;
  limits: ClinePassQuota[];
  models: ClinePassModel[];
  error: string | null;
}

/** Reads quota windows and the model catalog; the key never leaves the Rust kernel. */
export async function clinePassOverview(): Promise<ClinePassOverview> {
  if (!isDesktopRuntime()) {
    throw new IpcError("clinepass_unavailable", "Cline Pass 面板仅在桌面应用中可用。");
  }
  return invoke<ClinePassOverview>("clinepass_overview");
}

/** Opens cline.bot/cline-pass in the system browser. */
export async function openClinePassSubscription(): Promise<void> {
  if (!isDesktopRuntime()) {
    throw new IpcError("clinepass_unavailable", "Cline Pass 面板仅在桌面应用中可用。");
  }
  await invoke("clinepass_open_subscription");
}

export function quotaLabel(kind: string): string {
  switch (kind) {
    case "five_hour":
      return "5 小时窗口";
    case "weekly":
      return "本周";
    case "monthly":
      return "本月";
    default:
      return kind;
  }
}

export function formatQuotaReset(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
