import { Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useModal } from "../../hooks/useModal";

export interface CommandAction {
  id: string;
  label: string;
  hint?: string;
  keywords?: string[];
  run: () => void;
  disabled?: boolean;
}

interface CommandPaletteProps {
  open: boolean;
  actions: CommandAction[];
  onClose: () => void;
}

/**
 * Keyboard-first action menu (Ctrl/⌘+K). It only lists actions the workbench already owns;
 * nothing here bypasses a guard in the underlying handler.
 */
export function CommandPalette({ open, actions, onClose }: CommandPaletteProps) {
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    // The dialog section receives focus first; move it into the search box for typing.
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return actions;
    return actions.filter((action) => {
      const haystack = [action.label, action.hint ?? "", ...(action.keywords ?? [])].join(" ").toLowerCase();
      return haystack.includes(needle);
    });
  }, [actions, query]);

  if (!open) return null;

  function run(action: CommandAction | undefined) {
    if (!action || action.disabled) return;
    onClose();
    action.run();
  }

  function handleKeydown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) => (filtered.length === 0 ? 0 : (current + 1) % filtered.length));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => (filtered.length === 0 ? 0 : (current - 1 + filtered.length) % filtered.length));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      run(filtered[activeIndex]);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <section
        ref={sectionRef}
        tabIndex={-1}
        className="modal command-palette"
        role="dialog"
        aria-modal="true"
        aria-labelledby="command-palette-title"
      >
        <h2 id="command-palette-title" className="visually-hidden">命令面板</h2>
        <div className="command-search">
          <Search size={15} aria-hidden="true" />
          <input
            ref={inputRef}
            aria-label="搜索命令"
            placeholder="搜索命令，例如「向导」「导出」「设置」"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={handleKeydown}
          />
        </div>
        <div className="command-list" role="listbox" aria-label="命令列表">
          {filtered.map((action, index) => (
            <button
              className={index === activeIndex ? "command-item is-active" : "command-item"}
              type="button"
              role="option"
              aria-selected={index === activeIndex}
              key={action.id}
              disabled={action.disabled}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => run(action)}
            >
              <span>{action.label}</span>
              {action.hint && <small>{action.hint}</small>}
            </button>
          ))}
          {filtered.length === 0 && <div className="command-empty">没有匹配的命令</div>}
        </div>
        <footer className="command-footer">
          <span>↑ ↓ 选择</span>
          <span>Enter 执行</span>
          <span>Esc 关闭</span>
        </footer>
      </section>
    </div>
  );
}
