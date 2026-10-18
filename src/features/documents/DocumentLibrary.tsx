import { FileText, History, Receipt, RefreshCw, Search, Trash2, X } from "lucide-react";
import { useState } from "react";
import { useModal } from "../../hooks/useModal";
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
  onSessions: (id: string, title: string) => void;
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
  onSessions,
  onDelete,
}: DocumentLibraryProps) {
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const [query, setQuery] = useState("");
  if (!open) return null;

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? documents.filter((item) => {
        const haystack = [item.title, documentTypeLabels[item.docType], item.docType].join(" ").toLowerCase();
        return haystack.includes(needle);
      })
    : documents;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section ref={sectionRef} tabIndex={-1} className="modal document-library" role="dialog" aria-modal="true" aria-labelledby="document-library-title">
        <header className="modal-header">
          <div>
            <h2 id="document-library-title">最近文档</h2>
            <span>{visible.length === documents.length ? `${documents.length} 份` : `${visible.length} / ${documents.length} 份`}</span>
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
        <div className="library-search">
          <Search size={14} aria-hidden="true" />
          <input
            aria-label="搜索文档"
            placeholder="按标题或文种搜索"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <div className="document-list">
          {documents.length === 0 && !loading && (
            <div className="empty-state modal-empty">
              <FileText size={25} />
              <span>还没有保存的公文</span>
            </div>
          )}
          {documents.length > 0 && visible.length === 0 && (
            <div className="empty-state modal-empty">
              <Search size={22} />
              <span>没有匹配的文档</span>
            </div>
          )}
          {visible.map((item) => (
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
                className="icon-button small"
                type="button"
                onClick={() => onSessions(item.id, item.title)}
                title="起草记录与成本"
                aria-label={`查看 ${item.title} 的起草记录`}
              >
                <Receipt size={14} />
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
