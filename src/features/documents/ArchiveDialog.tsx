import { Archive, Check, LoaderCircle, X } from "lucide-react";
import { useModal } from "../../hooks/useModal";
import { type FormEvent, useEffect, useState } from "react";
import type { ArchiveResult } from "../../lib/ipc";

interface ArchiveDialogProps {
  open: boolean;
  title: string;
  vault: string;
  busy: boolean;
  onClose: () => void;
  onArchive: (path: string, overwrite: boolean) => Promise<ArchiveResult>;
}

function safeArchiveName(value: string): string {
  const normalized = value
    .trim()
    .replace(/[\\/:*?"<>|]/g, "-")
    .replace(/\s+/g, " ")
    .replace(/[. ]+$/g, "")
    .slice(0, 120);
  return normalized || "未命名公文";
}

function suggestedPath(title: string): string {
  return `公文/${new Date().getFullYear()}/${safeArchiveName(title)}.md`;
}

export function ArchiveDialog({
  open,
  title,
  vault,
  busy,
  onClose,
  onArchive,
}: ArchiveDialogProps) {
  const [path, setPath] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setPath(suggestedPath(title));
    setOverwrite(false);
    setMessage(null);
  }, [open, title]);

  const sectionRef = useModal<HTMLElement>(open, onClose, busy);
  if (!open) return null;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setMessage(null);
    try {
      const result = await onArchive(path, overwrite);
      setMessage(result.overwritten ? "归档已更新" : "归档已写入");
      onClose();
    } catch (error) {
      const code = typeof error === "object" && error !== null && "code" in error ? error.code : null;
      if (code === "kb_archive_exists") {
        setMessage("同名笔记已存在，请更换路径或确认覆盖");
        return;
      }
      setMessage(
        typeof error === "object" && error !== null && "message" in error && typeof error.message === "string"
          ? error.message
          : typeof error === "string" ? error : "归档未完成，请稍后重试。",
      );
    }
  }

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !busy && onClose()}>
      <section ref={sectionRef} tabIndex={-1} className="modal archive-dialog" role="dialog" aria-modal="true" aria-labelledby="archive-dialog-title">
        <header className="modal-header">
          <div>
            <h2 id="archive-dialog-title">归档到知识库</h2>
            <span>{vault}</span>
          </div>
          <button className="icon-button" type="button" onClick={onClose} disabled={busy} title="关闭" aria-label="关闭归档窗口">
            <X size={17} />
          </button>
        </header>
        <form className="archive-form" onSubmit={submit}>
          <div className="settings-group">
            <label htmlFor="archive-path">笔记路径</label>
            <input
              id="archive-path"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              maxLength={1024}
              placeholder="公文/2026/文件标题.md"
              disabled={busy}
              required
              autoFocus
            />
          </div>
          <label className="toggle-row archive-overwrite-row">
            <span>
              <b>覆盖同名笔记</b>
              <small>仅覆盖上述路径，不影响知识库中的其他素材</small>
            </span>
            <input
              type="checkbox"
              checked={overwrite}
              onChange={(event) => setOverwrite(event.target.checked)}
              disabled={busy}
            />
          </label>
          <div className="provider-status" role="status">{message && <span>{message}</span>}</div>
          <footer className="modal-footer">
            <button className="command-button" type="button" onClick={onClose} disabled={busy}>取消</button>
            <button className="command-button primary" type="submit" disabled={busy || !path.trim()}>
              {busy ? <LoaderCircle className="spin" size={14} /> : overwrite ? <Check size={15} /> : <Archive size={15} />}
              {overwrite ? "确认覆盖" : "写入归档"}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
