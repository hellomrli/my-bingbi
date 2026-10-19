import { Info, X } from "lucide-react";
import { useModal } from "../../hooks/useModal";
import { documentTypeLabels, securityLabels } from "../../lib/document";
import type { DocumentType, SecurityLevel } from "../../lib/odoc";
import { useWorkbenchStore } from "../../store/workbench";

interface DocumentInfoDialogProps {
  open: boolean;
  onClose: () => void;
}

/** Optional header/sign-off fields, kept out of the writing flow until someone needs them. */
export function DocumentInfoDialog({ open, onClose }: DocumentInfoDialogProps) {
  const document = useWorkbenchStore((state) => state.document);
  const updateDocumentType = useWorkbenchStore((state) => state.updateDocumentType);
  const updateSecurityLevel = useWorkbenchStore((state) => state.updateSecurityLevel);
  const updateHeader = useWorkbenchStore((state) => state.updateHeader);
  const updateRecipient = useWorkbenchStore((state) => state.updateRecipient);
  const updateSigner = useWorkbenchStore((state) => state.updateSigner);
  const updateDate = useWorkbenchStore((state) => state.updateDate);
  const sectionRef = useModal<HTMLElement>(open, onClose);

  if (!open) return null;

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section ref={sectionRef} className="modal document-info" role="dialog" aria-modal="true" aria-labelledby="document-info-title" tabIndex={-1}>
        <header className="document-info-header">
          <div>
            <span className="quality-eyebrow"><Info size={13} /> 可选信息</span>
            <h2 id="document-info-title">文档信息</h2>
          </div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="关闭文档信息">
            <X size={16} />
          </button>
        </header>

        <p className="document-info-note">都可以留空。不填不影响写作和导出，只是预览与红头版式会更完整。</p>

        <div className="document-info-grid">
          <label className="field">
            <span>文种</span>
            <select value={document.meta.docType} onChange={(event) => updateDocumentType(event.target.value as DocumentType)}>
              {Object.entries(documentTypeLabels).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>密级</span>
            <select value={document.meta.securityLevel} onChange={(event) => updateSecurityLevel(event.target.value as SecurityLevel)}>
              {Object.entries(securityLabels).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>发文机关标志</span>
            <input
              value={document.header?.issuingAuthorityMark ?? ""}
              onChange={(event) => updateHeader("issuingAuthorityMark", event.target.value)}
              placeholder="发文机关名称 + 文件"
            />
          </label>
          <label className="field">
            <span>发文字号</span>
            <input
              value={document.header?.documentNumber ?? ""}
              onChange={(event) => updateHeader("documentNumber", event.target.value)}
              placeholder="机关代字〔年份〕序号"
            />
          </label>
          <label className="field">
            <span>签发人</span>
            <input
              value={document.header?.signer ?? ""}
              onChange={(event) => updateHeader("signer", event.target.value)}
              placeholder="姓名"
            />
          </label>
          <label className="field">
            <span>主送机关</span>
            <input
              value={document.body.mainRecipients[0] ?? ""}
              onChange={(event) => updateRecipient(event.target.value)}
              placeholder="主送机关"
            />
          </label>
          <label className="field">
            <span>发文机关署名</span>
            <input
              value={document.body.signer?.authority ?? ""}
              onChange={(event) => updateSigner(event.target.value)}
              placeholder="落款机关"
            />
          </label>
          <label className="field">
            <span>成文日期</span>
            <input
              value={document.body.date?.value ?? ""}
              onChange={(event) => updateDate(event.target.value)}
              placeholder="2026年10月19日"
            />
          </label>
        </div>

        <footer className="document-info-footer">
          <button className="command-button primary" type="button" onClick={onClose}>
            完成
          </button>
        </footer>
      </section>
    </div>
  );
}
