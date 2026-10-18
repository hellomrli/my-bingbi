import { Check, ChevronLeft, ChevronRight, Circle, LoaderCircle, Settings2, WandSparkles, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useModal } from "../../hooks/useModal";
import { documentTypeLabels } from "../../lib/document";
import { draftStageLabels, draftStages, type DraftStageId } from "../../lib/engine";
import type { DocumentType } from "../../lib/odoc";
import type { TemplateSummary } from "../../lib/pack";
import {
  WIZARD_STEPS,
  composeWizardMaterial,
  createWizardDraft,
  recipientRequired,
  validateStep,
  type WizardDraft,
} from "./wizardModel";

interface DraftWizardProps {
  open: boolean;
  initialDocType: DocumentType;
  /** Templates from the knowledge pack, already grouped by document type. */
  templates: TemplateSummary[];
  /** Whether the desktop model runtime can actually start a draft right now. */
  canDraft: boolean;
  /** Browser preview cannot call models; it keeps the "fill material only" fallback. */
  desktop?: boolean;
  /** Display name of the selected provider, shown when generation fails. */
  providerName?: string;
  /** Live pipeline state while the wizard owns the generation. */
  drafting?: boolean;
  stage?: DraftStageId | null;
  completedStages?: DraftStageId[];
  generationError?: string | null;
  onConfigureProvider?: () => void;
  onComplete?: () => void;
  onClose: () => void;
  onFinish: (draft: WizardDraft) => void;
}

/**
 * Step-by-step drafting intake for users who do not want to face a blank material box.
 *
 * It only collects facts and constraints; the composed brief is handed to the same kernel
 * pipeline as the free-form material, so the drafting, fact-check and lint behavior is identical.
 * The wizard stays open while that pipeline runs, shows stage progress, then reveals the result
 * in the workbench.
 */
export function DraftWizard({
  open,
  initialDocType,
  templates,
  canDraft,
  desktop = true,
  providerName = "",
  drafting = false,
  stage = null,
  completedStages = [],
  generationError = null,
  onConfigureProvider,
  onComplete,
  onClose,
  onFinish,
}: DraftWizardProps) {
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const [draft, setDraft] = useState<WizardDraft>(() => createWizardDraft(initialDocType));
  const [stepIndex, setStepIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);
  const [sawRunning, setSawRunning] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDraft(createWizardDraft(initialDocType));
    setStepIndex(0);
    setError(null);
    setSubmitted(false);
    setSawRunning(false);
  }, [open, initialDocType]);

  useEffect(() => {
    if (!submitted) return;
    if (drafting) {
      setSawRunning(true);
      return;
    }
    if (generationError || !sawRunning) return;
    onComplete?.();
    onClose();
  }, [drafting, generationError, onClose, onComplete, sawRunning, submitted]);

  if (!open) return null;

  if (submitted) {
    const total = draftStages.length;
    const done = completedStages.filter((item) =>
      (draftStages as readonly string[]).includes(item),
    ).length;
    const percent = Math.round((done / total) * 100);
    const currentStage = stage && (draftStages as readonly string[]).includes(stage) ? stage : null;
    return (
      <div
        className="modal-backdrop"
        role="presentation"
        onMouseDown={(event) => {
          if (event.target === event.currentTarget && (!drafting || generationError)) onClose();
        }}
      >
        <section
          ref={sectionRef}
          tabIndex={-1}
          className="modal wizard-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="draft-wizard-title"
        >
          <header className="wizard-header">
            <div>
              <h2 id="draft-wizard-title">
                <WandSparkles size={16} aria-hidden="true" /> 向导模式
              </h2>
              <span>{generationError ? "生成失败" : "正在生成"}</span>
            </div>
            <button
              className="icon-button"
              type="button"
              onClick={onClose}
              disabled={drafting && !generationError}
              title={drafting ? "生成中不可关闭" : "关闭"}
              aria-label="关闭向导"
            >
              <X size={17} />
            </button>
          </header>

          <div className="wizard-body">
            {generationError ? (
              <div className="wizard-generation-error" role="alert">
                <strong>生成没有完成</strong>
                <p>{providerName ? `当前模型服务：${providerName}。` : ""}{generationError}</p>
                <p className="wizard-step-hint">
                  详细阶段和错误码已写入「设置 → 常规 → 生成日志」，可复制后排查。
                </p>
                <p className="wizard-step-hint">
                  本地模型请确认服务已启动；云端模型请检查 Key、地址和网络。
                </p>
              </div>
            ) : (
              <>
                <div className="wizard-generating-head">
                  <LoaderCircle className="is-spinning" size={18} />
                  <span>材料已提交，正在整理并起草</span>
                </div>
                <progress className="wizard-progress" max={100} value={percent} aria-label="生成进度" />
                <div className="wizard-progress-meta">
                  <span>{percent}%</span>
                  <span>{currentStage ? `正在${draftStageLabels[currentStage]}` : "等待模型响应"}</span>
                </div>
                <ol className="wizard-stage-list" aria-label="起草阶段">
                  {draftStages.map((item) => {
                    const doneStage = completedStages.includes(item);
                    const current = currentStage === item && !doneStage;
                    return (
                      <li
                        className={doneStage ? "is-done" : current ? "is-current" : "is-pending"}
                        key={item}
                      >
                        {doneStage ? <Check size={13} /> : current ? <LoaderCircle className="is-spinning" size={13} /> : <Circle size={11} />}
                        {draftStageLabels[item]}
                      </li>
                    );
                  })}
                </ol>
                <p className="wizard-step-hint">完成后会自动关闭向导，生成结果直接显示在正文编辑器中。</p>
              </>
            )}
          </div>

          <footer className="wizard-footer">
            {generationError ? (
              <>
                <button
                  className="command-button"
                  type="button"
                  onClick={() => {
                    setSubmitted(false);
                    setSawRunning(false);
                    setError(generationError);
                  }}
                >
                  返回修改
                </button>
                {onConfigureProvider && (
                  <button className="command-button" type="button" onClick={onConfigureProvider}>
                    <Settings2 size={14} />
                    切换模型服务
                  </button>
                )}
                <span className="panel-count" />
                <button className="command-button primary" type="button" onClick={onClose}>
                  关闭
                </button>
              </>
            ) : (
              <span className="wizard-step-hint">生成中，请保持应用打开</span>
            )}
          </footer>
        </section>
      </div>
    );
  }

  const step = WIZARD_STEPS[stepIndex];
  const isLast = stepIndex === WIZARD_STEPS.length - 1;
  const progress = ((stepIndex + 1) / WIZARD_STEPS.length) * 100;

  function update<K extends keyof WizardDraft>(field: K, value: WizardDraft[K]) {
    setDraft((current) => ({ ...current, [field]: value }));
    if (error) setError(null);
  }

  const typeTemplates = templates.filter((template) => template.docType === draft.docType);
  const selectedTemplate = typeTemplates.find((template) => template.id === draft.templateId) ?? null;

  function updateDocType(nextType: DocumentType) {
    setDraft((current) => {
      const keepsTemplate = templates.some(
        (template) => template.id === current.templateId && template.docType === nextType,
      );
      return {
        ...current,
        docType: nextType,
        ...(keepsTemplate ? {} : { templateId: "", templateName: "" }),
      };
    });
    if (error) setError(null);
  }

  function updateTemplate(templateId: string) {
    const template = templates.find((candidate) => candidate.id === templateId);
    setDraft((current) => ({
      ...current,
      templateId: template?.id ?? "",
      templateName: template?.name ?? "",
    }));
    if (error) setError(null);
  }

  function goNext() {
    const problem = validateStep(step.id, draft);
    if (problem) {
      setError(problem);
      return;
    }
    setError(null);
    if (isLast) {
      if (!canDraft) {
        if (!desktop) {
          onFinish(draft);
          onClose();
          return;
        }
        setError("当前没有可用的模型服务。请先启用一个服务，再点「交给 AI 起草」。");
        return;
      }
      setSubmitted(true);
      onFinish(draft);
      return;
    }
    setStepIndex((current) => current + 1);
  }

  function goBack() {
    setError(null);
    setStepIndex((current) => Math.max(0, current - 1));
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
        className="modal wizard-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="draft-wizard-title"
        aria-describedby="draft-wizard-step-hint"
      >
        <header className="wizard-header">
          <div>
            <h2 id="draft-wizard-title">
              <WandSparkles size={16} aria-hidden="true" /> 向导模式
            </h2>
            <span className="wizard-step-title">
              第 {stepIndex + 1} / {WIZARD_STEPS.length} 步 · {step.title}
            </span>
          </div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭向导" aria-label="关闭向导">
            <X size={17} />
          </button>
        </header>
        <div className="wizard-progress" aria-hidden="true">
          <span style={{ width: `${progress}%` }} />
        </div>

        <div className="wizard-body">
          <p className="wizard-step-hint" id="draft-wizard-step-hint">{step.hint}</p>

          {step.id === "brief" && (
            <>
              <label className="wizard-field" htmlFor="wizard-doc-type">
                <span>文种</span>
                <select
                  id="wizard-doc-type"
                  value={draft.docType}
                  onChange={(event) => updateDocType(event.target.value as DocumentType)}
                >
                  {Object.entries(documentTypeLabels).map(([value, label]) => (
                    <option key={value} value={value}>{label}</option>
                  ))}
                </select>
              </label>
              <label className="wizard-field" htmlFor="wizard-subject">
                <span>发文事由（必填）</span>
                <input
                  id="wizard-subject"
                  value={draft.subject}
                  onChange={(event) => update("subject", event.target.value)}
                  placeholder="一句话说明为什么发文，例如：申请购置应急排水设备经费"
                  autoComplete="off"
                />
              </label>
              <label className="wizard-field" htmlFor="wizard-recipient">
                <span>
                  主送机关
                  {recipientRequired(draft.docType) ? "（必填）" : "（可选）"}
                </span>
                <input
                  id="wizard-recipient"
                  value={draft.recipient}
                  onChange={(event) => update("recipient", event.target.value)}
                  placeholder="例如：×××人民政府"
                  autoComplete="off"
                />
              </label>
            </>
          )}

          {step.id === "details" && (
            <>
              <p className="wizard-step-hint">这一页都可以留空；只写你确定的事实即可。</p>
              <label className="wizard-field" htmlFor="wizard-facts">
                <span>关键事实（可选，一行一条）</span>
                <textarea
                  id="wizard-facts"
                  rows={6}
                  value={draft.facts}
                  onChange={(event) => update("facts", event.target.value)}
                  placeholder={"例如：\n现有排水泵车 2 台，其中 1 台今年故障 3 次\n拟购置 3 台，经费 15 万元"}
                />
              </label>
              <label className="wizard-field" htmlFor="wizard-deadline">
                <span>完成时限（可选）</span>
                <input
                  id="wizard-deadline"
                  value={draft.deadline}
                  onChange={(event) => update("deadline", event.target.value)}
                  placeholder="例如：2026 年 5 月 20 日前"
                  autoComplete="off"
                />
              </label>
              <label className="wizard-field" htmlFor="wizard-requirements">
                <span>其他要求（可选）</span>
                <textarea
                  id="wizard-requirements"
                  rows={4}
                  value={draft.requirements}
                  onChange={(event) => update("requirements", event.target.value)}
                  placeholder="例如：篇幅不超过 1500 字；只报电子版；需附联系人"
                />
              </label>
              <details className="wizard-advanced">
                <summary>补充材料与模板（可选）</summary>
                <label className="wizard-field" htmlFor="wizard-references">
                  <span>政策依据 / 参考段落</span>
                  <textarea
                    id="wizard-references"
                    rows={5}
                    value={draft.references}
                    onChange={(event) => update("references", event.target.value)}
                    placeholder="粘贴政策依据、领导批示或参考段落；知识库引用可在左侧单独添加。"
                  />
                </label>
                {typeTemplates.length > 0 && (
                  <label className="wizard-field" htmlFor="wizard-template">
                    <span>场景模板</span>
                    <select
                      id="wizard-template"
                      value={draft.templateId}
                      onChange={(event) => updateTemplate(event.target.value)}
                    >
                      <option value="">标准模板</option>
                      {typeTemplates.map((template) => (
                        <option value={template.id} key={template.id}>
                          {template.name}
                        </option>
                      ))}
                    </select>
                    {selectedTemplate?.scenario && (
                      <small className="wizard-field-hint">{selectedTemplate.scenario}</small>
                    )}
                  </label>
                )}
              </details>
            </>
          )}

          {step.id === "review" && (
            <>
              <div className="wizard-summary-label">将交给模型的起草材料</div>
              <pre className="wizard-summary" aria-label="起草材料预览">
                {composeWizardMaterial(draft)}
              </pre>
              {!canDraft && (
                <div className="wizard-provider-hint">
                  <p className="wizard-step-hint">
                    {desktop
                      ? "当前没有可用的桌面模型服务。先在设置中启用一个服务，再回来点「交给 AI 起草」。"
                      : "浏览器预览不能调用模型；完成后会把材料填入写作区，请在桌面版中起草。"}
                  </p>
                  {desktop && onConfigureProvider && (
                    <button className="command-button" type="button" onClick={onConfigureProvider}>
                      <Settings2 size={14} />
                      打开模型服务设置
                    </button>
                  )}
                </div>
              )}
            </>
          )}

          {error && <p className="wizard-error" role="alert">{error}</p>}
        </div>

        <footer className="wizard-footer">
          <button className="command-button" type="button" onClick={goBack} disabled={stepIndex === 0}>
            <ChevronLeft size={14} />
            上一步
          </button>
          <span className="panel-count">{stepIndex + 1} / {WIZARD_STEPS.length}</span>
          <button className="command-button primary" type="button" onClick={goNext}>
            {isLast ? (
              <>
                <WandSparkles size={14} />
                {canDraft ? "交给 AI 起草" : desktop ? "先配置模型服务" : "填入写作材料"}
              </>
            ) : (
              <>
                下一步
                <ChevronRight size={14} />
              </>
            )}
          </button>
        </footer>
      </section>
    </div>
  );
}
