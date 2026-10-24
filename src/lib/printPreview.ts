/** Print a kernel-rendered document without popup handles or an opener relationship. */
export function printPreviewHtml(html: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const frame = document.createElement("iframe");
    frame.title = "公文打印预览";
    frame.setAttribute("sandbox", "allow-same-origin allow-modals");
    frame.style.cssText = "position:fixed;left:-10000px;top:0;width:210mm;height:297mm;border:0";
    let cleanupTimer: ReturnType<typeof setTimeout> | undefined;
    let finished = false;
    const cleanup = () => {
      clearTimeout(loadTimer);
      clearTimeout(cleanupTimer);
      frame.remove();
    };
    const fail = (error: unknown) => {
      if (finished) return;
      finished = true;
      cleanup();
      reject(error);
    };
    const loadTimer = setTimeout(() => fail(new Error("打印预览加载超时，请重试。")), 30_000);
    frame.onerror = () => fail(new Error("无法加载打印预览。"));
    frame.onload = async () => {
      if (finished) return;
      try {
        const preview = frame.contentWindow;
        if (!preview) throw new Error("无法创建打印预览。请重试。");
        await preview.document.fonts?.ready;
        if (finished) return;
        clearTimeout(loadTimer);
        preview.addEventListener("afterprint", cleanup, { once: true });
        // Some WebViews omit afterprint; bound the lifetime without removing a loading frame.
        cleanupTimer = setTimeout(cleanup, 300_000);
        preview.focus();
        preview.print();
        finished = true;
        resolve();
      } catch (error) {
        fail(error);
      }
    };
    frame.srcdoc = html;
    document.body.append(frame);
  });
}
