import { ChevronLeft, ChevronRight, WandSparkles, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useModal } from "../../hooks/useModal";
import { documentTypeLabels } from "../../lib/document";
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
  onClose: () => void;
  onFinish: (draft: WizardDraft) => void;
}

/**
 * Step-by-step drafting intake for users who do not want to face a blank material box.
 *
 * It only collects facts and constraints; the composed brief is handed to the same kernel
 * pipeline as the free-form material, so the drafting, fact-check and lint behavior is identical.
 */
export function DraftWizard({ open, initialDocType, templates, canDraft, onClose, onFinish }: DraftWizardProps) {
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const [draft, setDraft] = useState<WizardDraft>(() => createWizardDraft(initialDocType));
  const [stepIndex, setStepIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setDraft(createWizardDraft(initialDocType));
    setStepIndex(0);
    setError(null);
  }, [open, initialDocType]);

  if (!open) return null;

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

          {step.id === "type" && (
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
            </>
          )}

          {step.id === "subject" && (
            <>
              <label className="wizard-field" htmlFor="wizard-subject">
                <span>发文事由（必填）</span>
                <input
                  id="wizard-subject"
                  value={draft.subject}
                  onChange={(event) => update("subject", event.target.value)}
                  placeholder="例如：申请购置应急排水设备经费"
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

          {step.id === "facts" && (
            <label className="wizard-field" htmlFor="wizard-facts">
              <span>关键事实（一行一条）</span>
              <textarea
                id="wizard-facts"
                rows={8}
                value={draft.facts}
                onChange={(event) => update("facts", event.target.value)}
                placeholder={"例如：\n现有排水泵车 2 台，其中 1 台今年故障 3 次\n拟购置 3 台，经费 15 万元\n经费从×××专项列支"}
              />
            </label>
          )}

          {step.id === "constraints" && (
            <>
              <label className="wizard-field" htmlFor="wizard-deadline">
                <span>完成时限</span>
                <input
                  id="wizard-deadline"
                  value={draft.deadline}
                  onChange={(event) => update("deadline", event.target.value)}
                  placeholder="例如：2026 年 5 月 20 日前"
                  autoComplete="off"
                />
              </label>
              <label className="wizard-field" htmlFor="wizard-requirements">
                <span>其他要求</span>
                <textarea
                  id="wizard-requirements"
                  rows={5}
                  value={draft.requirements}
                  onChange={(event) => update("requirements", event.target.value)}
                  placeholder="例如：篇幅不超过 1500 字；只报电子版；需附联系人"
                />
              </label>
            </>
          )}

          {step.id === "materials" && (
            <label className="wizard-field" htmlFor="wizard-references">
              <span>补充材料（可选）</span>
              <textarea
                id="wizard-references"
                rows={8}
                value={draft.references}
                onChange={(event) => update("references", event.target.value)}
                placeholder="粘贴政策依据、领导批示、历史公文的参考段落。知识库引用可在左侧「知识库引用」中单独添加。"
              />
            </label>
          )}

          {step.id === "review" && (
            <>
              <div className="wizard-summary-label">将交给模型的起草材料</div>
              <pre className="wizard-summary" aria-label="起草材料预览">
                {composeWizardMaterial(draft)}
              </pre>
              {!canDraft && (
                <p className="wizard-step-hint">
                  当前没有可用的桌面模型服务：完成后会先把材料填入写作区，选择模型后再点「开始起草」。
                </p>
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
                {canDraft ? "交给 AI 起草" : "填入写作材料"}
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
