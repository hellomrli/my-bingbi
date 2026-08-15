import { GitCompareArrows, History, LoaderCircle, RotateCcw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  diffDocumentVersions,
  ipcErrorMessage,
  listDocumentVersions,
  restoreDocumentVersion,
  type RestoreVersionResult,
  type VersionDiff,
} from "../../lib/ipc";
import type { DocumentVersionSummary } from "../../lib/store";

interface VersionHistoryDialogProps {
  open: boolean;
  documentId: string | null;
  title: string;
  onClose: () => void;
  onRestored: (result: RestoreVersionResult) => void;
}

interface DiffRow {
  left: string | null;
  right: string | null;
  kind: "same" | "removed" | "added";
}

function formatTimestamp(value: number): string {
  return new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function stageLabel(value: string | null): string {
  if (!value) return "保存";
  if (value.startsWith("restore:")) return `恢复自版本 ${value.slice(8)}`;
  const labels: Record<string, string> = {
    autosave: "自动保存",
    user_save: "手动保存",
    draft_complete: "起草完成",
    export: "导出",
  };
  return labels[value] ?? value;
}

function alignDiffLines(diff: VersionDiff): DiffRow[] {
  const left = diff.leftText.split("\n");
  const right = diff.rightText.split("\n");
  if (left.length * right.length > 2_000_000) {
    return Array.from({ length: Math.max(left.length, right.length) }, (_, index) => ({
      left: left[index] ?? null,
      right: right[index] ?? null,
      kind: left[index] === right[index] ? "same" : left[index] === undefined ? "added" : "removed",
    }));
  }
  const matrix = Array.from({ length: left.length + 1 }, () => new Uint32Array(right.length + 1));
  for (let row = left.length - 1; row >= 0; row -= 1) {
    for (let column = right.length - 1; column >= 0; column -= 1) {
      matrix[row][column] = left[row] === right[column]
        ? matrix[row + 1][column + 1] + 1
        : Math.max(matrix[row + 1][column], matrix[row][column + 1]);
    }
  }
  const rows: DiffRow[] = [];
  let leftIndex = 0;
  let rightIndex = 0;
  while (leftIndex < left.length || rightIndex < right.length) {
    if (leftIndex < left.length && rightIndex < right.length && left[leftIndex] === right[rightIndex]) {
      rows.push({ left: left[leftIndex], right: right[rightIndex], kind: "same" });
      leftIndex += 1;
      rightIndex += 1;
    } else if (rightIndex < right.length
      && (leftIndex >= left.length || matrix[leftIndex][rightIndex + 1] >= matrix[leftIndex + 1][rightIndex])) {
      rows.push({ left: null, right: right[rightIndex], kind: "added" });
      rightIndex += 1;
    } else {
      rows.push({ left: left[leftIndex], right: null, kind: "removed" });
      leftIndex += 1;
    }
  }
  return rows;
}

export function VersionHistoryDialog({
  open,
  documentId,
  title,
  onClose,
  onRestored,
}: VersionHistoryDialogProps) {
  const [versions, setVersions] = useState<DocumentVersionSummary[]>([]);
  const [leftSeq, setLeftSeq] = useState<number | null>(null);
  const [rightSeq, setRightSeq] = useState<number | null>(null);
  const [diff, setDiff] = useState<VersionDiff | null>(null);
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !documentId) return;
    let active = true;
    setLoading(true);
    setMessage(null);
    setDiff(null);
    listDocumentVersions(documentId)
      .then((loaded) => {
        if (!active) return;
        setVersions(loaded);
        setRightSeq(loaded[0]?.seq ?? null);
        setLeftSeq(loaded[1]?.seq ?? loaded[0]?.seq ?? null);
      })
      .catch((error) => active && setMessage(ipcErrorMessage(error)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [documentId, open]);

  useEffect(() => {
    if (!open || !documentId || leftSeq === null || rightSeq === null) return;
    let active = true;
    setLoading(true);
    diffDocumentVersions(documentId, leftSeq, rightSeq)
      .then((result) => active && setDiff(result))
      .catch((error) => active && setMessage(ipcErrorMessage(error)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [documentId, leftSeq, open, rightSeq]);

  const rows = useMemo(() => diff ? alignDiffLines(diff) : [], [diff]);

  if (!open || !documentId) return null;

  async function restoreSelected() {
    if (!documentId || rightSeq === null || !window.confirm(`确定恢复版本 ${rightSeq} 吗？当前内容会作为历史保留，并新建一个恢复版本。`)) return;
    setRestoring(true);
    setMessage(null);
    try {
      const result = await restoreDocumentVersion(documentId, rightSeq);
      onRestored(result);
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setRestoring(false);
    }
  }

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !restoring && onClose()}>
      <section className="modal version-dialog" role="dialog" aria-modal="true" aria-labelledby="version-dialog-title">
        <header className="modal-header">
          <div>
            <h2 id="version-dialog-title">版本历史</h2>
            <span title={title}>{title || "未命名公文"}</span>
          </div>
          <button className="icon-button" type="button" onClick={onClose} disabled={restoring} title="关闭" aria-label="关闭版本历史">
            <X size={17} />
          </button>
        </header>
        <div className="version-layout">
          <aside className="version-list" aria-label="历史版本列表">
            <div className="version-list-heading"><History size={14} /> {versions.length} 个版本</div>
            {versions.map((version) => (
              <button
                className={rightSeq === version.seq ? "version-item is-active" : "version-item"}
                type="button"
                onClick={() => setRightSeq(version.seq)}
                key={version.seq}
              >
                <strong>版本 {version.seq}</strong>
                <span>修订 {version.odocRevision} · {stageLabel(version.stage)}</span>
                <time>{formatTimestamp(version.createdAt)}</time>
              </button>
            ))}
            {!loading && versions.length === 0 && <div className="modal-empty">没有历史版本</div>}
          </aside>
          <section className="version-diff">
            <header className="diff-toolbar">
              <GitCompareArrows size={15} />
              <label>
                基准
                <select value={leftSeq ?? ""} onChange={(event) => setLeftSeq(Number(event.target.value))} disabled={loading}>
                  {versions.map((version) => <option value={version.seq} key={version.seq}>版本 {version.seq}</option>)}
                </select>
              </label>
              <label>
                对比
                <select value={rightSeq ?? ""} onChange={(event) => setRightSeq(Number(event.target.value))} disabled={loading}>
                  {versions.map((version) => <option value={version.seq} key={version.seq}>版本 {version.seq}</option>)}
                </select>
              </label>
              {diff && (
                <span className="diff-summary">
                  <i className="diff-added">+{diff.addedLines}</i>
                  <i className="diff-removed">-{diff.removedLines}</i>
                </span>
              )}
            </header>
            <div className="diff-column-headings"><span>版本 {leftSeq ?? "-"}</span><span>版本 {rightSeq ?? "-"}</span></div>
            <div className="diff-grid" aria-label="版本文本差异">
              {loading && !diff ? <div className="diff-loading"><LoaderCircle className="spin" size={18} /></div> : rows.map((row, index) => (
                <div className={`diff-row is-${row.kind}`} key={`${index}-${row.kind}`}>
                  <pre>{row.left ?? ""}</pre>
                  <pre>{row.right ?? ""}</pre>
                </div>
              ))}
            </div>
            <footer className="version-footer">
              <span role="status">{message}</span>
              <button
                className="command-button primary"
                type="button"
                onClick={() => void restoreSelected()}
                disabled={rightSeq === null || restoring || loading}
              >
                {restoring ? <LoaderCircle className="spin" size={14} /> : <RotateCcw size={14} />}
                恢复对比版本
              </button>
            </footer>
          </section>
        </div>
      </section>
    </div>
  );
}
