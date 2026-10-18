import {
  Archive,
  Download,
  FilePlus2,
  FileUp,
  FolderOpen,
  Grid3X3,
  History,
  Moon,
  Printer,
  RotateCw,
  Save,
  ScanSearch,
  Settings,
  Sun,
  SunMoon,
  WandSparkles,
} from "lucide-react";
import { documentTypeLabels } from "../../lib/document";
import { draftStageLabels, draftStages, imitateStages, polishStages, type DraftStageId } from "../../lib/engine";
import type { Finding } from "../../lib/lint";
import { reviewStageLabels, reviewStages, type ReviewStageId } from "../../lib/review";
import type { AppTheme } from "../../lib/store";
import { useWorkbenchStore } from "../../store/workbench";

interface ToolbarProps {
  findings: Finding[];
  onNew: () => void;
  onImport: () => void;
  onSave: () => void;
  onExport: () => void;
  onPrint: () => void;
  onReview: () => void;
  onArchive: () => void;
  onOpenHistory: () => void;
  onOpenDocuments: () => void;
  onOpenSettings: () => void;
  onOpenWizard: () => void;
  onThemeChange: (theme: AppTheme) => void;
  exportFormat?: "docx" | "odoc";
  pipeline?: "draft" | "review" | "imitate" | "polish";
  activeStage?: DraftStageId | null;
  completedStages?: DraftStageId[];
  failedStage?: DraftStageId | null;
  activeReviewStage?: ReviewStageId | null;
  completedReviewStages?: ReviewStageId[];
  failedReviewStage?: ReviewStageId | null;
  canReview?: boolean;
  canArchive?: boolean;
  hasVersions?: boolean;
  /** Completed draft stages call this to re-run that single stage from its recorded snapshot. */
  onRerunStage?: (stage: DraftStageId) => void;
  canRerunStage?: boolean;
}

export function Toolbar({
  findings,
  onNew,
  onImport,
  onSave,
  onExport,
  onPrint,
  onReview,
  onArchive,
  onOpenHistory,
  onOpenDocuments,
  onOpenSettings,
  onOpenWizard,
  onThemeChange,
  exportFormat = "odoc",
  pipeline = "draft",
  activeStage = null,
  completedStages = [],
  failedStage = null,
  activeReviewStage = null,
  completedReviewStages = [],
  failedReviewStage = null,
  canReview = false,
  canArchive = false,
  hasVersions = false,
  onRerunStage,
  canRerunStage = false,
}: ToolbarProps) {
  const document = useWorkbenchStore((state) => state.document);
  const showGrid = useWorkbenchStore((state) => state.showGrid);
  const theme = useWorkbenchStore((state) => state.theme);
  const toggleGrid = useWorkbenchStore((state) => state.toggleGrid);
  const errorCount = findings.filter((finding) => finding.severity === "error").length;
  const title = document.body.title.text || "未命名公文";
  const reviewing = pipeline === "review";
  const pipelineStages: { id: string; label: string }[] = reviewing
    ? reviewStages.map((id) => ({ id, label: reviewStageLabels[id] }))
    : pipeline === "imitate"
      ? imitateStages.map((id) => ({ id, label: draftStageLabels[id] }))
      : pipeline === "polish"
        ? polishStages.map((id) => ({ id, label: draftStageLabels[id] }))
        : draftStages.map((id) => ({ id, label: draftStageLabels[id] }));
  const current: string | null = reviewing ? activeReviewStage : activeStage;
  const completed: readonly string[] = reviewing ? completedReviewStages : completedStages;
  const failed: string | null = reviewing ? failedReviewStage : failedStage;

  const nextTheme = theme === "system" ? "light" : theme === "light" ? "dark" : "system";
  const ThemeIcon = theme === "system" ? SunMoon : theme === "light" ? Sun : Moon;

  return (
    <header className="toolbar">
      <div className="toolbar-top">
        <div className="brand" aria-label="秉笔">
          秉<span>笔</span>
        </div>
        <div className="document-identity">
          <span className="chip chip-neutral">{documentTypeLabels[document.meta.docType]}</span>
          <strong title={title}>{title}</strong>
        </div>
        <div className="toolbar-primary">
          <button
            className="command-button wizard-button"
            type="button"
            onClick={onOpenWizard}
            title="向导模式：一步一步填写要素，再交给模型起草"
            aria-label="向导模式"
          >
            <WandSparkles size={15} />
            <span>向导模式</span>
          </button>
          <button
            className="command-button"
            type="button"
            onClick={onReview}
            disabled={!canReview}
            title={canReview ? "审核当前公文" : "公文审核仅在桌面版可用，请先选择模型服务"}
            aria-label="审核当前公文"
          >
            <ScanSearch size={15} />
            审核
          </button>
          <button
            className="command-button primary"
            type="button"
            onClick={onExport}
            disabled={errorCount > 0}
            title={errorCount ? `存在 ${errorCount} 项错误，暂不能导出` : exportFormat === "docx" ? "导出 DOCX" : "导出 ODoc JSON"}
          >
            <Download size={15} />
            {exportFormat === "docx" ? "导出 DOCX" : "导出 ODoc"}
          </button>
        </div>
      </div>

      <div className="toolbar-bottom">
        <div className="pipeline" aria-label={reviewing ? "审核流水线" : "起草流水线"}>
          {pipelineStages.map((stage, index) => {
            const stateClass = failed === stage.id
              ? "is-failed"
              : current === stage.id
                ? "is-active"
                : completed.includes(stage.id)
                  ? "is-complete"
                  : index < 2 && !current && completed.length === 0
                    ? "is-ready"
                    : "";
            const rerunnable = !reviewing
              && canRerunStage
              && stateClass === "is-complete"
              && current === null;
            return rerunnable ? (
              <button
                className={`pipeline-stage is-complete is-rerunnable`}
                type="button"
                key={stage.id}
                onClick={() => onRerunStage?.(stage.id as DraftStageId)}
                title={`仅重跑「${stage.label}」这一阶段`}
                aria-label={`重跑${stage.label}阶段`}
              >
                <i />
                {stage.label}
                <RotateCw size={10} />
              </button>
            ) : (
              <span className={`pipeline-stage ${stateClass}`.trim()} key={stage.id}>
                <i />
                {stage.label}
              </span>
            );
          })}
        </div>
        <div className="toolbar-actions">
          <button className="icon-button" type="button" onClick={onNew} title="新建公文" aria-label="新建公文">
            <FilePlus2 size={16} />
          </button>
          <button className="icon-button mobile-secondary-action" type="button" onClick={onImport} title="导入 DOCX" aria-label="导入 DOCX">
            <FileUp size={16} />
          </button>
          <button className="icon-button mobile-secondary-action" type="button" onClick={onSave} title="保存到本机" aria-label="保存到本机">
            <Save size={16} />
          </button>
          <button className="icon-button" type="button" onClick={onOpenDocuments} title="最近文档" aria-label="最近文档">
            <FolderOpen size={16} />
          </button>
          <button
            className="icon-button mobile-secondary-action"
            type="button"
            onClick={onOpenHistory}
            disabled={!hasVersions}
            title={hasVersions ? "版本历史" : "请先保存文档"}
            aria-label="打开版本历史"
          >
            <History size={16} />
          </button>
          {canArchive && (
            <button
              className="icon-button mobile-secondary-action"
              type="button"
              onClick={onArchive}
              title="归档到知识库"
              aria-label="归档到知识库"
            >
              <Archive size={16} />
            </button>
          )}
          <button
            className={showGrid ? "icon-button is-active" : "icon-button"}
            type="button"
            onClick={toggleGrid}
            title="切换 22 × 28 字格"
            aria-label="切换 22 × 28 字格"
            aria-pressed={showGrid}
          >
            <Grid3X3 size={16} />
          </button>
          <button
            className="icon-button"
            type="button"
            onClick={() => onThemeChange(nextTheme)}
            title={`主题：${theme === "system" ? "跟随系统" : theme === "light" ? "亮色" : "暗色"}`}
            aria-label="切换主题"
          >
            <ThemeIcon size={16} />
          </button>
          <button className="icon-button" type="button" onClick={onOpenSettings} title="设置" aria-label="设置">
            <Settings size={16} />
          </button>
          <button
            className="icon-button"
            type="button"
            onClick={onPrint}
            disabled={errorCount > 0}
            title={errorCount ? `存在 ${errorCount} 项错误，暂不能打印` : "打印 / 导出 PDF"}
            aria-label="打印或导出 PDF"
          >
            <Printer size={16} />
          </button>
        </div>
      </div>
    </header>
  );
}
