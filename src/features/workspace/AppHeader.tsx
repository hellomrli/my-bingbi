import {
  Archive,
  ChevronDown,
  Download,
  FilePlus2,
  FileUp,
  FolderOpen,
  Grid3X3,
  History,
  Info,
  LoaderCircle,
  Moon,
  Printer,
  Save,
  ScanSearch,
  Settings,
  Sun,
  SunMoon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { documentTypeLabels } from "../../lib/document";
import { draftStageLabels, draftStages, type DraftStageId } from "../../lib/engine";
import { reviewStageLabels, reviewStages, type ReviewStageId } from "../../lib/review";
import type { DocumentType } from "../../lib/odoc";
import type { AppTheme } from "../../lib/store";

interface AppHeaderProps {
  composing: boolean;
  title: string;
  docType: DocumentType;
  dirty: boolean;
  saving: boolean;
  savedLabel: string;
  errorCount: number;
  canReview: boolean;
  canArchive: boolean;
  hasVersions: boolean;
  showGrid: boolean;
  theme: AppTheme;
  starting: boolean;
  drafting: boolean;
  stage: DraftStageId | null;
  completedStages: DraftStageId[];
  reviewing: boolean;
  reviewStage: ReviewStageId | null;
  completedReviewStages: ReviewStageId[];
  onTitleChange: (value: string) => void;
  onHome: () => void;
  onNew: () => void;
  onSave: () => void;
  onExport: () => void;
  onImport: () => void;
  onPrint: () => void;
  onOpenDocuments: () => void;
  onOpenHistory: () => void;
  onOpenInfo: () => void;
  onOpenSettings: () => void;
  onReview: () => void;
  onArchive: () => void;
  onToggleGrid: () => void;
  onThemeChange: () => void;
}

function MenuItem({
  icon: Icon,
  label,
  hint,
  disabled = false,
  onSelect,
}: {
  icon: typeof Save;
  label: string;
  hint?: string;
  disabled?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className="app-menu-item"
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onSelect}
    >
      <Icon size={15} />
      <span>{label}</span>
      {hint && <small>{hint}</small>}
    </button>
  );
}

export function AppHeader({
  composing,
  title,
  docType,
  dirty,
  saving,
  savedLabel,
  errorCount,
  canReview,
  canArchive,
  hasVersions,
  showGrid,
  theme,
  starting,
  drafting,
  stage,
  completedStages,
  reviewing,
  reviewStage,
  completedReviewStages,
  onTitleChange,
  onHome,
  onNew,
  onSave,
  onExport,
  onImport,
  onPrint,
  onOpenDocuments,
  onOpenHistory,
  onOpenInfo,
  onOpenSettings,
  onReview,
  onArchive,
  onToggleGrid,
  onThemeChange,
}: AppHeaderProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    function close(event: MouseEvent) {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    }
    function escape(event: KeyboardEvent) {
      if (event.key === "Escape") setMenuOpen(false);
    }
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", escape);
    };
  }, [menuOpen]);

  function run(action: () => void) {
    setMenuOpen(false);
    action();
  }

  const themeLabel = theme === "system" ? "跟随系统" : theme === "light" ? "亮色" : "暗色";
  const ThemeIcon = theme === "system" ? SunMoon : theme === "light" ? Sun : Moon;
  const activeStage = reviewing ? reviewStage : stage;
  const completed = reviewing ? completedReviewStages : completedStages;
  const total = reviewing ? reviewStages.length : draftStages.length;
  const busy = starting || drafting;
  const stageLabel = activeStage
    ? reviewing
      ? reviewStageLabels[activeStage as ReviewStageId]
      : draftStageLabels[activeStage as DraftStageId]
    : starting
      ? "准备写作"
      : "准备中";

  return (
    <>
      <header className="app-header">
        <button className="app-brand" type="button" onClick={onHome} title="回到写作台" aria-label="回到写作台">
          秉<span>笔</span>
        </button>

        {composing ? (
          <span className="header-context">写作台</span>
        ) : (
          <div className="header-document">
            <input
              className="header-title-input"
              aria-label="公文标题"
              value={title}
              onChange={(event) => onTitleChange(event.target.value)}
              placeholder="未命名公文"
            />
            <span className="header-type">{documentTypeLabels[docType]}</span>
          </div>
        )}

        {!composing && (
          <span className={dirty ? "save-state is-dirty" : "save-state"}>
            {saving ? "保存中…" : dirty ? "未保存" : savedLabel}
          </span>
        )}

        <div className="header-actions">
          <button className="header-button icon-only" type="button" onClick={onSave} title="保存（Ctrl+S）" aria-label="保存">
            <Save size={16} />
          </button>
          <button
            className="header-export"
            type="button"
            onClick={onExport}
            disabled={errorCount > 0}
            title={errorCount > 0 ? `存在 ${errorCount} 项错误，暂不能导出` : "导出 DOCX"}
          >
            <Download size={15} />
            导出
          </button>
          <div className="app-menu-wrap" ref={menuRef}>
            <button
              className="header-button icon-only"
              type="button"
              onClick={() => setMenuOpen((current) => !current)}
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              title="更多"
              aria-label="更多"
            >
              <ChevronDown size={16} />
            </button>
            {menuOpen && (
              <div className="app-menu" role="menu" aria-label="更多操作">
                <MenuItem icon={FilePlus2} label="新建写作" hint="从空白开始" onSelect={() => run(onNew)} />
                <MenuItem icon={FolderOpen} label="最近文档" onSelect={() => run(onOpenDocuments)} />
                <MenuItem icon={Save} label="保存到本机" hint="Ctrl+S" onSelect={() => run(onSave)} />
                <MenuItem icon={FileUp} label="导入 DOCX" onSelect={() => run(onImport)} />
                <MenuItem icon={Printer} label="打印 / 导出 PDF" onSelect={() => run(onPrint)} />
                <div className="app-menu-separator" />
                <MenuItem
                  icon={ScanSearch}
                  label="语义审核"
                  disabled={!canReview}
                  onSelect={() => run(onReview)}
                />
                <MenuItem icon={Info} label="文档信息" disabled={composing} onSelect={() => run(onOpenInfo)} />
                <MenuItem
                  icon={History}
                  label="版本历史"
                  disabled={!hasVersions}
                  onSelect={() => run(onOpenHistory)}
                />
                {canArchive && (
                  <MenuItem icon={Archive} label="归档到知识库" onSelect={() => run(onArchive)} />
                )}
                <div className="app-menu-separator" />
                <MenuItem
                  icon={Grid3X3}
                  label={showGrid ? "隐藏字格" : "显示字格"}
                  disabled={composing}
                  onSelect={() => run(onToggleGrid)}
                />
                <MenuItem icon={ThemeIcon} label={`主题：${themeLabel}`} onSelect={() => run(onThemeChange)} />
                <MenuItem icon={Settings} label="设置" onSelect={() => run(onOpenSettings)} />
              </div>
            )}
          </div>
        </div>
      </header>

      {(busy || reviewing) && (
        <div className="writing-progress" role="status" aria-live="polite">
          <LoaderCircle size={14} className="is-spinning" />
          <span>{reviewing ? "正在审核" : "正在写作"} · {stageLabel}</span>
          <div className={activeStage ? "writing-progress-track" : "writing-progress-track is-indeterminate"} aria-hidden="true">
            <span style={activeStage ? { width: `${Math.max(4, Math.round((completed.length / total) * 100))}%` } : undefined} />
          </div>
          <small>{completed.length} / {total}</small>
        </div>
      )}
    </>
  );
}
