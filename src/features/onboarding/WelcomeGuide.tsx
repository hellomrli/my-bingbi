import { BookOpen, Check, ChevronRight, Database, FileText, PenLine, ScanSearch } from "lucide-react";
import { useEffect, useState } from "react";
import { useModal } from "../../hooks/useModal";
import { ipcErrorMessage, listProviderProfiles } from "../../lib/ipc";
import type { ProviderProfile } from "../../lib/llm";

interface WelcomeGuideProps {
  open: boolean;
  onComplete: () => void;
  onOpenSettings: () => void;
  /** Called when the user picks a provider in step two; wires the choice into the workbench. */
  onSelectProvider?: (providerId: string) => void;
  /** Dismisses the guide for this session without marking first-run setup as finished. */
  onClose?: () => void;
}

const capabilityHighlights = [
  { icon: FileText, title: "起草", detail: "一句话写清要办的事，自动生成初稿" },
  { icon: PenLine, title: "编辑", detail: "生成后按段落修改，版式自动合规" },
  { icon: ScanSearch, title: "质检", detail: "规则硬校验 + 事实回查，风险标红" },
  { icon: Database, title: "知识库", detail: "对接你的 Obsidian 库，按需引用" },
];

export function WelcomeGuide({ open, onComplete, onOpenSettings, onSelectProvider, onClose }: WelcomeGuideProps) {
  const [step, setStep] = useState(0);
  const [providers, setProviders] = useState<ProviderProfile[]>([]);
  const [providerBusy, setProviderBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<string | null>(null);
  const sectionRef = useModal<HTMLElement>(open, () => onClose?.());

  useEffect(() => {
    if (!open) return;
    setStep(0);
    setError(null);
    setSelectedProvider(null);
  }, [open]);

  useEffect(() => {
    if (!open || step !== 1) return;
    setProviderBusy(true);
    listProviderProfiles()
      .then((loaded) => {
        setProviders(loaded);
        // Default to the previously selected provider when it is still enabled, otherwise the
        // first enabled one — the radio is controlled so the selection actually matters.
        setSelectedProvider((current) =>
          current && loaded.some((provider) => provider.id === current && provider.enabled)
            ? current
            : (loaded.find((provider) => provider.enabled)?.id ?? null),
        );
      })
      .catch((loadError) => setError(ipcErrorMessage(loadError)))
      .finally(() => setProviderBusy(false));
  }, [open, step]);

  if (!open) return null;

  const enabledProviders = providers.filter((provider) => provider.enabled);
  const canProceed = step !== 1 || enabledProviders.length > 0;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose?.()}>
      <section ref={sectionRef} tabIndex={-1} className="modal welcome-guide" role="dialog" aria-modal="true" aria-labelledby="welcome-guide-title">
        <div className="welcome-guide-steps" aria-hidden="true">
          {[0, 1, 2].map((index) => (
            <span className={index <= step ? "is-done" : ""} key={index} />
          ))}
        </div>
        {step === 0 && (
          <div className="welcome-guide-body">
            <div className="brand brand-large" aria-hidden="true">秉<span>笔</span></div>
            <h2 id="welcome-guide-title">公文写作工作台</h2>
            <p className="welcome-guide-lead">
              把规范、语料与模型打通：格式合规、事实可溯、少 AI 味、可直接交付的公文。
            </p>
            <div className="welcome-guide-grid">
              {capabilityHighlights.map(({ icon: Icon, title, detail }) => (
                <div className="welcome-guide-item" key={title}>
                  <Icon size={16} />
                  <div>
                    <strong>{title}</strong>
                    <span>{detail}</span>
                  </div>
                </div>
              ))}
            </div>
            <p className="welcome-guide-note">所有数据保存在本机，模型调用可全程使用本地 Ollama。</p>
          </div>
        )}
        {step === 1 && (
          <div className="welcome-guide-body">
            <h2>选择模型服务</h2>
            <p className="welcome-guide-lead">起草、仿写与审核都通过模型完成。可以稍后在设置中更换。</p>
            {providerBusy && <div className="source-message">正在加载模型服务</div>}
            {error && <div className="source-message is-error" role="alert">{error}</div>}
            {!providerBusy && enabledProviders.length === 0 && !error && (
              <div className="welcome-guide-empty">
                还没有启用的模型服务。
                <button className="command-button" type="button" onClick={onOpenSettings}>去设置中添加</button>
              </div>
            )}
            {!providerBusy && enabledProviders.length > 0 && (
              <div className="welcome-provider-list">
                {enabledProviders.map((provider) => (
                  <label className="welcome-provider-row" key={provider.id}>
                    <input
                      type="radio"
                      name="welcome-provider"
                      checked={provider.id === selectedProvider}
                      onChange={() => {
                        setSelectedProvider(provider.id);
                        onSelectProvider?.(provider.id);
                      }}
                    />
                    <span>
                      <strong>{provider.displayName}</strong>
                      <small>{provider.isLocal ? "本地模型" : "云端模型"} · {provider.model}</small>
                    </span>
                  </label>
                ))}
              </div>
            )}
          </div>
        )}
        {step === 2 && (
          <div className="welcome-guide-body">
            <h2>准备就绪</h2>
            <p className="welcome-guide-lead">现在可以开始第一份公文了：在写作台输入框里写清大致意图，参考文件和知识库都可选，点击「开始写作」即可，无需先写正文。</p>
            <div className="welcome-guide-checklist">
              {["文种路由与事实提取自动完成", "生成的数字与日期会回查标红", "一键导出符合 GB/T 9704 的 DOCX"].map((item) => (
                <div key={item}><Check size={14} />{item}</div>
              ))}
            </div>
          </div>
        )}
        <footer className="welcome-guide-footer">
          <span className="panel-count">{step + 1} / 3</span>
          <button
            className="command-button primary"
            type="button"
            disabled={!canProceed}
            onClick={() => (step < 2 ? setStep(step + 1) : onComplete())}
          >
            {step < 2 ? (<>下一步<ChevronRight size={14} /></>) : (<><BookOpen size={14} />开始使用</>)}
          </button>
        </footer>
      </section>
    </div>
  );
}
