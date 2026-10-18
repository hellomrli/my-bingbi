import { Clipboard, LoaderCircle, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  clearGenerationLogs,
  formatGenerationLogs,
  readGenerationLogs,
  subscribeGenerationLogs,
  type GenerationLogEntry,
} from "../../lib/generationLog";

function formatTime(value: number): string {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

/** Local-only draft diagnostics: what was attempted, which stage stopped, and why. */
export function GenerationLogSettings() {
  const [entries, setEntries] = useState<GenerationLogEntry[]>(() => readGenerationLogs());
  const [message, setMessage] = useState<string | null>(null);
  const [copying, setCopying] = useState(false);

  useEffect(() => subscribeGenerationLogs(() => setEntries(readGenerationLogs())), []);

  async function copyLogs() {
    if (entries.length === 0 || copying) return;
    setCopying(true);
    setMessage(null);
    try {
      await navigator.clipboard.writeText(formatGenerationLogs(entries));
      setMessage("日志已复制到剪贴板");
    } catch {
      setMessage("复制失败，请手动选择文本");
    } finally {
      setCopying(false);
    }
  }

  function clearLogs() {
    if (!window.confirm("清空本机的生成日志吗？这不影响文档或模型配置。")) return;
    clearGenerationLogs();
    setEntries([]);
    setMessage("生成日志已清空");
  }

  return (
    <section className="generation-log" aria-label="生成日志">
      <header>
        <span>生成日志</span>
        <small>仅本机保存最近 100 条；不记录材料正文</small>
        <div className="generation-log-actions">
          <button
            className="icon-button tiny"
            type="button"
            onClick={() => void copyLogs()}
            disabled={entries.length === 0 || copying}
            title="复制日志"
            aria-label="复制生成日志"
          >
            {copying ? <LoaderCircle size={14} className="is-spinning" /> : <Clipboard size={14} />}
          </button>
          <button
            className="icon-button tiny danger-action"
            type="button"
            onClick={clearLogs}
            disabled={entries.length === 0}
            title="清空日志"
            aria-label="清空生成日志"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </header>
      {entries.length === 0 ? (
        <div className="settings-hint">还没有生成记录。失败时这里会显示阶段、错误码和模型服务。</div>
      ) : (
        <ol className="generation-log-list">
          {entries.map((entry) => (
            <li className={`generation-log-row is-${entry.level}`} key={entry.id}>
              <span className="generation-log-time">{formatTime(entry.at)}</span>
              <span className="generation-log-stage">
                {entry.stage ? `${entry.stage}` : entry.event}
              </span>
              <span className="generation-log-provider">{entry.providerId ?? ""}</span>
              <span className="generation-log-message">{entry.message}</span>
              {entry.code && <code>{entry.code}</code>}
            </li>
          ))}
        </ol>
      )}
      {message && <small className="field-message">{message}</small>}
    </section>
  );
}
