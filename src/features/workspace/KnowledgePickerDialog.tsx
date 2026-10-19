import { Database, FileText, LoaderCircle, Search, X } from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { useModal } from "../../hooks/useModal";
import { ipcErrorMessage, kbGet, kbSearch } from "../../lib/ipc";
import type { KbHit, KbNote } from "../../lib/kb";

interface KnowledgePickerDialogProps {
  open: boolean;
  documentId: string;
  onClose: () => void;
  onInsert: (label: string, content: string) => void;
}

/** Optional knowledge-base quote dialog; the writer's main screen stays a single input. */
export function KnowledgePickerDialog({ open, documentId, onClose, onInsert }: KnowledgePickerDialogProps) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<KbHit[]>([]);
  const [selected, setSelected] = useState<KbNote | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const sectionRef = useModal<HTMLElement>(open, onClose);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setHits([]);
    setSelected(null);
    setBusy(false);
    setError(null);
    setSearched(false);
  }, [open]);

  async function search(event: FormEvent) {
    event.preventDefault();
    if (!query.trim() || busy) return;
    setBusy(true);
    setError(null);
    setSelected(null);
    setSearched(true);
    try {
      setHits(await kbSearch({
        vault: null,
        keyword: query.trim(),
        searchContent: true,
        searchMode: "content",
        pathPrefix: null,
        filters: {},
        limit: 20,
      }, documentId));
    } catch (searchError) {
      setHits([]);
      setError(ipcErrorMessage(searchError));
    } finally {
      setBusy(false);
    }
  }

  async function openNote(path: string) {
    setBusy(true);
    setError(null);
    try {
      setSelected(await kbGet(path, documentId));
    } catch (loadError) {
      setError(ipcErrorMessage(loadError));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section ref={sectionRef} className="modal knowledge-dialog" role="dialog" aria-modal="true" aria-labelledby="knowledge-title" tabIndex={-1}>
        <header className="document-info-header">
          <div>
            <span className="quality-eyebrow"><Database size={13} /> 可选素材</span>
            <h2 id="knowledge-title">从知识库引用</h2>
          </div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="关闭知识库">
            <X size={16} />
          </button>
        </header>

        <form className="knowledge-search" onSubmit={search}>
          <Search size={15} />
          <input
            aria-label="搜索知识库"
            placeholder="搜索历史公文、政策文件或本单位资料"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            disabled={busy}
          />
          <button className="command-button" type="submit" disabled={!query.trim() || busy}>
            {busy ? <LoaderCircle size={14} className="is-spinning" /> : "搜索"}
          </button>
        </form>

        {error && <div className="composer-message is-error" role="alert">{error}</div>}

        <div className="knowledge-body">
          <div className="knowledge-hits">
            {hits.map((hit) => (
              <button
                className={selected?.path === hit.path ? "knowledge-hit is-active" : "knowledge-hit"}
                type="button"
                onClick={() => void openNote(hit.path)}
                key={`${hit.vault}:${hit.path}`}
                title={hit.path}
              >
                <FileText size={14} />
                <span>
                  <strong>{hit.title}</strong>
                  <small>{hit.path}</small>
                  {hit.snippet && <em>{hit.snippet}</em>}
                </span>
              </button>
            ))}
            {searched && !busy && hits.length === 0 && !error && (
              <div className="quality-empty">
                <Search size={20} />
                <strong>没有匹配资料</strong>
                <span>换个关键词试试。</span>
              </div>
            )}
            {!searched && (
              <div className="quality-empty">
                <Database size={20} />
                <strong>搜索你的知识库</strong>
                <span>找到后可以引用到本次写作材料里。</span>
              </div>
            )}
          </div>

          {selected && (
            <article className="knowledge-preview">
              <div className="knowledge-preview-head">
                <strong>{selected.title}</strong>
                <small>{selected.path}</small>
              </div>
              <p>{selected.content}</p>
              <button
                className="command-button primary"
                type="button"
                onClick={() => {
                  onInsert(`${selected.title}｜${selected.path}`, selected.content);
                  onClose();
                }}
              >
                引用到写作材料
              </button>
            </article>
          )}
        </div>
      </section>
    </div>
  );
}
