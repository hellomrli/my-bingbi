import { Bug, LoaderCircle, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { clearCrashEvents, ipcErrorMessage, listCrashEvents } from "../../lib/ipc";
import type { CrashEvent } from "../../lib/store";

interface CrashDiagnosticsSettingsProps {
  enabled: boolean;
  desktop: boolean;
  onEnabledChange: (enabled: boolean) => void;
}

interface CrashContext {
  appVersion?: string;
  os?: string;
  arch?: string;
  thread?: string;
  file?: string | null;
  line?: number | null;
  column?: number | null;
}

function eventContext(contextJson: string): string {
  try {
    const context = JSON.parse(contextJson) as CrashContext;
    const target = [context.os, context.arch].filter(Boolean).join("/");
    const location = context.file
      ? `${context.file}${context.line ? `:${context.line}` : ""}${context.column ? `:${context.column}` : ""}`
      : "";
    return [
      context.appVersion ? `v${context.appVersion}` : "",
      target,
      context.thread ? `线程 ${context.thread}` : "",
      location,
    ].filter(Boolean).join(" · ");
  } catch {
    return "诊断上下文不可解析";
  }
}

function eventTime(timestamp: number): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "时间未知";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function eventIsoTime(timestamp: number): string | undefined {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
}

export function CrashDiagnosticsSettings({
  enabled,
  desktop,
  onEnabledChange,
}: CrashDiagnosticsSettingsProps) {
  const [events, setEvents] = useState<CrashEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!desktop) {
      setEvents([]);
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      setEvents(await listCrashEvents(20));
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }, [desktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function clearEvents() {
    if (!desktop || events.length === 0 || !window.confirm("确定清空本机保存的全部崩溃诊断记录吗？")) return;
    setBusy(true);
    setMessage(null);
    try {
      const deleted = await clearCrashEvents();
      setEvents([]);
      setMessage(`已清空 ${deleted} 条本地诊断记录`);
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="settings-group crash-diagnostics">
      <label className="toggle-row">
        <span>
          <b>本地崩溃诊断</b>
          <small>保存设置后捕获 Rust 内核异常；记录仅留在本机，绝不上传</small>
        </span>
        <input
          type="checkbox"
          checked={enabled}
          disabled={!desktop}
          onChange={(event) => onEnabledChange(event.target.checked)}
        />
      </label>
      <div className="crash-diagnostics-toolbar">
        <span className="settings-hint">
          {desktop
            ? busy ? "正在读取诊断记录…" : events.length > 0 ? `保留了最近 ${events.length} 条记录` : "暂无崩溃记录"
            : "浏览器预览不提供内核诊断"}
        </span>
        <div>
          <button
            className="icon-button tiny"
            type="button"
            onClick={() => void refresh()}
            disabled={!desktop || busy}
            title="刷新诊断记录"
            aria-label="刷新诊断记录"
          >
            {busy ? <LoaderCircle size={14} className="is-spinning" /> : <RefreshCw size={14} />}
          </button>
          <button
            className="icon-button tiny danger-action"
            type="button"
            onClick={() => void clearEvents()}
            disabled={!desktop || busy || events.length === 0}
            title="清空诊断记录"
            aria-label="清空诊断记录"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>
      {message && <div className="source-message" role="status">{message}</div>}
      {events.length > 0 && (
        <details className="crash-event-details">
          <summary><Bug size={13} />查看最近诊断</summary>
          <ol className="crash-event-list">
            {events.map((event) => (
              <li key={event.id}>
                <div>
                  <strong>{event.kind === "rust_panic" ? "Rust 内核异常" : event.kind}</strong>
                  <time dateTime={eventIsoTime(event.createdAt)}>{eventTime(event.createdAt)}</time>
                </div>
                <p>{event.message}</p>
                <small>{eventContext(event.contextJson)}</small>
              </li>
            ))}
          </ol>
        </details>
      )}
    </div>
  );
}
