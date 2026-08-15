import { FileText, History, RefreshCw, Trash2, X } from "lucide-react";
import { documentTypeLabels } from "../../lib/document";
import type { DocumentSummary } from "../../lib/store";

interface DocumentLibraryProps {
  open: boolean;
  documents: DocumentSummary[];
  currentDocumentId: string;
  loading: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onOpen: (id: string) => void;
  onHistory: (id: string, title: string) => void;
  onDelete: (id: string) => void;
}

function formatTimestamp(value: number): string {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function DocumentLibrary({
  open,
  documents,
  currentDocumentId,
  loading,
  onClose,
  onRefresh,
  onOpen,
  onHistory,
  onDelete,
}: DocumentLibraryProps) {
  if (!open) return null;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="modal document-library" role="dialog" aria-modal="true" aria-labelledby="document-library-title">
        <header className="modal-header">
          <div>
            <h2 id="document-library-title">最近文档</h2>
            <span>{documents.length} 份</span>
          </div>
          <div className="modal-header-actions">
            <button className="icon-button" type="button" onClick={onRefresh} title="刷新" aria-label="刷新最近文档">
              <RefreshCw size={16} className={loading ? "is-spinning" : undefined} />
            </button>
            <button className="icon-button" type="button" onClick={onClose} title="关闭" aria-label="关闭最近文档">
              <X size={17} />
            </button>
          </div>
        </header>
        <div className="document-list">
          {documents.length === 0 && !loading && (
            <div className="empty-state modal-empty">
              <FileText size={25} />
              <span>还没有保存的公文</span>
            </div>
          )}
          {documents.map((item) => (
            <div className={item.id === currentDocumentId ? "document-row is-current" : "document-row"} key={item.id}>
              <FileText size={17} />
              <button className="document-open" type="button" onClick={() => onOpen(item.id)}>
                <strong>{item.title}</strong>
                <span>
                  {documentTypeLabels[item.docType]} · 修订 {item.revision} · {item.versionCount} 个版本 · {formatTimestamp(item.updatedAt)}
                </span>
              </button>
              {item.id === currentDocumentId && <span className="chip chip-ok">当前</span>}
              <button
                className="icon-button small"
                type="button"
                onClick={() => onHistory(item.id, item.title)}
                disabled={item.versionCount === 0}
                title="版本历史"
                aria-label={`查看 ${item.title} 的版本历史`}
              >
                <History size={14} />
              </button>
              <button
                className="icon-button small danger-action"
                type="button"
                onClick={() => onDelete(item.id)}
                title="删除文档"
                aria-label={`删除 ${item.title}`}
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
