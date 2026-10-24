import { useModal } from "../../hooks/useModal";
import type { ExitChoice } from "../../hooks/useDocumentLifecycle";

export function DocumentExitDialog({ open, prompt, failure, onChoice }: {
  open: boolean;
  prompt: boolean;
  failure: string | null;
  onChoice: (choice: ExitChoice) => void;
}) {
  const ref = useModal<HTMLElement>(open, () => onChoice("cancel"), !prompt);
  if (!open) return null;
  return (
    <div className="modal-backdrop" style={{ zIndex: 1000 }}>
      <section ref={ref} className="modal" role="dialog" aria-modal="true" aria-labelledby="exit-dialog-title" tabIndex={-1}>
        <header className="modal-header"><h2 id="exit-dialog-title">{prompt ? "保存未完成的修改？" : "正在完成退出操作"}</h2></header>
        <div className="settings-form">
          {failure && <p role="alert">保存失败：{failure}。可以重试、明确放弃修改或取消退出。</p>}
          <p>{prompt ? "当前文档有未保存修改。保存成功后才能继续关闭、重载或更新。" : "正在保存文档或安装更新，请稍候。"}</p>
          {prompt && <div className="modal-header-actions">
            <button className="command-button" onClick={() => onChoice("cancel")}>取消</button>
            <button className="command-button" onClick={() => onChoice("discard")}>不保存并继续</button>
            <button className="command-button primary" onClick={() => onChoice("save")}>保存并继续</button>
          </div>}
        </div>
      </section>
    </div>
  );
}
