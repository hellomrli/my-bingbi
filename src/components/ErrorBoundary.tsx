import { Component, type ErrorInfo, type ReactNode } from "react";
import { ClipboardCopy, Download, RefreshCw } from "lucide-react";
import { useWorkbenchStore } from "../store/workbench";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
  copied: boolean;
}

/** `__APP_VERSION__` is injected by Vite; unit tests run without it. */
function appVersion(): string {
  return typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";
}

/**
 * Last-resort guard around the workbench. React catches render/lifecycle errors only — async
 * failures and event handlers keep their own error paths — but without this a single broken
 * panel would blank the whole window and take the unsaved draft with it.
 *
 * The fallback stays deliberately dependency-free: it must render even when application state
 * is the thing that broke.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null, copied: false };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error, copied: false };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // The desktop kernel has its own opt-in panic log; this one only reaches the webview console.
    console.error("workbench crashed", error, info.componentStack);
  }

  private reload = (): void => {
    window.location.reload();
  };

  private retry = (): void => {
    this.setState({ error: null, copied: false });
  };

  private draftJson = (): string | null => {
    try {
      return JSON.stringify(useWorkbenchStore.getState().document, null, 2);
    } catch {
      return null;
    }
  };

  private downloadDraft = (): void => {
    const json = this.draftJson();
    if (!json) return;
    const blob = new Blob([json], { type: "application/json;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement("a");
    link.href = url;
    link.download = "秉笔-崩溃前草稿.json";
    link.click();
    URL.revokeObjectURL(url);
  };

  private diagnostics = (): string => {
    const error = this.state.error;
    return [
      "秉笔工作台异常报告",
      `时间：${new Date().toISOString()}`,
      `版本：${appVersion()}`,
      `UA：${navigator.userAgent}`,
      "",
      `${error?.name ?? "Error"}: ${error?.message ?? "未知错误"}`,
      error?.stack ?? "（无堆栈）",
    ].join("\n");
  };

  private copyDiagnostics = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(this.diagnostics());
      this.setState({ copied: true });
    } catch {
      // Clipboard permission denied (or a non-secure context): the message is still on screen.
      this.setState({ copied: false });
    }
  };

  render(): ReactNode {
    const { error, copied } = this.state;
    if (!error) return this.props.children;
    const hasDraft = this.draftJson() !== null;

    return (
      <div className="crash-screen" role="alert">
        <div className="crash-card">
          <span className="crash-badge">界面异常</span>
          <h1>工作台遇到了未处理的错误</h1>
          <p>
            出错的是界面渲染，草稿仍在本机存储中。建议先导出草稿备份，再重新加载应用。
          </p>
          <pre className="crash-message">{error.message || error.name || "未知错误"}</pre>
          <div className="crash-actions">
            <button
              className="command-button primary"
              type="button"
              onClick={this.downloadDraft}
              disabled={!hasDraft}
              title={hasDraft ? "下载当前内存中的 ODoc 草稿" : "无法读取当前草稿"}
            >
              <Download size={15} />
              导出草稿 JSON
            </button>
            <button className="command-button" type="button" onClick={this.reload}>
              <RefreshCw size={15} />
              重新加载
            </button>
            <button className="command-button" type="button" onClick={() => void this.copyDiagnostics()}>
              <ClipboardCopy size={15} />
              {copied ? "诊断信息已复制" : "复制诊断信息"}
            </button>
          </div>
          <button className="crash-retry" type="button" onClick={this.retry}>
            忽略错误并返回工作台（可能再次出错）
          </button>
        </div>
      </div>
    );
  }
}
