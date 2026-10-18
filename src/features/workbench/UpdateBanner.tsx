import { Download, LoaderCircle, X } from "lucide-react";
import type { UpdateCheckResult } from "../../lib/ipc";

interface UpdateBannerProps {
  update: UpdateCheckResult;
  installing: boolean;
  onInstall: () => void;
  onDismiss: () => void;
}

/** Startup update notice; it never installs without an explicit click. */
export function UpdateBanner({ update, installing, onInstall, onDismiss }: UpdateBannerProps) {
  return (
    <aside className="update-banner" role="status" aria-live="polite">
      <div className="update-banner-copy">
        <Download size={17} aria-hidden="true" />
        <div>
          <strong>发现新版本 v{update.version}</strong>
          <span>当前 v{update.currentVersion}，下载安装后自动重启。</span>
        </div>
      </div>
      <div className="update-banner-actions">
        <button className="command-button primary" type="button" onClick={onInstall} disabled={installing}>
          {installing ? <LoaderCircle className="is-spinning" size={14} /> : <Download size={14} />}
          {installing ? "正在更新" : "立即更新"}
        </button>
        <button
          className="icon-button small"
          type="button"
          onClick={onDismiss}
          disabled={installing}
          title="稍后提醒"
          aria-label="稍后提醒"
        >
          <X size={15} />
        </button>
      </div>
    </aside>
  );
}
