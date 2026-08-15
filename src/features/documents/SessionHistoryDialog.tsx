import { ChevronDown, History, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { draftStageLabels, type DraftStageId } from "../../lib/engine";
import { ipcErrorMessage, listSessions, listStageRuns } from "../../lib/ipc";
import type { SessionSummary, StageRunRecord } from "../../lib/store";

interface SessionHistoryDialogProps {
  open: boolean;
  documentId: string | null;
  title: string;
  onClose: () => void;
}

const pipelineLabels: Record<string, string> = {
  draft: "起草",
  imitate: "仿写",
  review: "审核",
};

function formatTimestamp(value: number): string {
  return new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function stageLabel(stageId: string): string {
  return draftStageLabels[stageId as DraftStageId] ?? stageId;
}

function formatTokens(value: number): string {
  return value.toLocaleString("zh-CN");
}

export function SessionHistoryDialog({
  open,
  documentId,
  title,
  onClose,
}: SessionHistoryDialogProps) {
  const [sessions, setSessions] = useState<SessionSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [runs, setRuns] = useState<Record<string, StageRunRecord[]>>({});
  const [runBusy, setRunBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !documentId) return;
    setSessions(null);
    setError(null);
    setExpanded(null);
    setRuns({});
    listSessions(documentId, 20)
      .then(setSessions)
      .catch((loadError) => setError(ipcErrorMessage(loadError)));
  }, [open, documentId]);

  async function toggleRuns(session: SessionSummary) {
    if (expanded === session.id) {
      setExpanded(null);
      return;
    }
    setExpanded(session.id);
    if (runs[session.id]) return;
    setRunBusy(session.id);
    try {
      const records = await listStageRuns(session.id);
      setRuns((current) => ({ ...current, [session.id]: records }));
    } catch (loadError) {
      setError(ipcErrorMessage(loadError));
    } finally {
      setRunBusy(null);
    }
  }

  const totalTokens = (sessions ?? []).reduce((sum, session) => sum + session.costTokens, 0);

  if (!open || !documentId) return null;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="modal session-history" role="dialog" aria-modal="true" aria-labelledby="session-history-title">
        <header className="modal-header">
          <div>
            <h2 id="session-history-title">起草记录</h2>
            <span>{sessions ? `${sessions.length} 次 · 共 ${formatTokens(totalTokens)} tokens` : "加载中"}</span>
          </div>
          <div className="modal-header-actions">
            <button className="icon-button" type="button" onClick={onClose} title="关闭" aria-label="关闭起草记录">
              <X size={17} />
            </button>
          </div>
        </header>
        <div className="session-list">
          {sessions === null && !error && (
            <div className="empty-state modal-empty">
              <LoaderCircle size={25} className="is-spinning" />
              <span>正在加载起草记录</span>
            </div>
          )}
          {error && <div className="source-message is-error" role="alert">{error}</div>}
          {sessions !== null && sessions.length === 0 && !error && (
            <div className="empty-state modal-empty">
              <History size={25} />
              <span>「{title}」还没有起草记录</span>
            </div>
          )}
          {sessions?.map((session) => {
            const open = expanded === session.id;
            const stageRuns = runs[session.id];
            return (
              <div className={open ? "session-row is-expanded" : "session-row"} key={session.id}>
                <button
                  className="session-row-head"
                  type="button"
                  onClick={() => void toggleRuns(session)}
                  aria-expanded={open}
                  aria-label={`展开 ${pipelineLabels[session.pipeline] ?? session.pipeline} 记录的阶段明细`}
                >
                  <span className={`chip chip-${pipelineLabels[session.pipeline] ? "accent" : "neutral"}`}>
                    {pipelineLabels[session.pipeline] ?? session.pipeline}
                  </span>
                  <span className="session-row-time">{formatTimestamp(session.updatedAt)}</span>
                  <span className="session-row-tokens">{formatTokens(session.costTokens)} tokens</span>
                  {runBusy === session.id
                    ? <LoaderCircle size={14} className="is-spinning" />
                    : <ChevronDown size={14} className={open ? "is-flipped" : undefined} />}
                </button>
                {open && (
                  <table className="session-run-table">
                    <thead>
                      <tr>
                        <th>阶段</th>
                        <th>模型服务</th>
                        <th className="is-numeric">tokens</th>
                        <th className="is-numeric">耗时</th>
                      </tr>
                    </thead>
                    <tbody>
                      {stageRuns ? stageRuns.map((run) => (
                        <tr key={run.id}>
                          <td>{stageLabel(run.stageId)}</td>
                          <td>{run.provider ?? "—"}</td>
                          <td className="is-numeric">{formatTokens(run.tokens)}</td>
                          <td className="is-numeric">{run.durationMs} ms</td>
                        </tr>
                      )) : (
                        <tr>
                          <td colSpan={4}>
                            <LoaderCircle size={13} className="is-spinning" />
                            正在加载阶段明细
                          </td>
                        </tr>
                      )}
                      {stageRuns && stageRuns.length === 0 && (
                        <tr>
                          <td colSpan={4} className="session-empty-detail">本次未记录阶段明细</td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                )}
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}
