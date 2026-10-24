import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipcErrorMessage } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export type ExitChoice = "save" | "discard" | "cancel";

interface LifecycleOptions {
  desktop: boolean;
  autosaveEnabled: boolean;
  busy: boolean;
  save: () => Promise<void>;
  onError: (message: string) => void;
}

/** All deliberate exits share a save barrier; unload itself cannot await an IPC save. */
export function useDocumentLifecycle(options: LifecycleOptions) {
  const latest = useRef(options);
  latest.current = options;
  const running = useRef(false);
  const allowUnload = useRef(false);
  const resolveChoice = useRef<((choice: ExitChoice) => void) | null>(null);
  const [leaving, setLeaving] = useState(false);
  const [prompt, setPrompt] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const decide = useCallback((choice: ExitChoice) => {
    resolveChoice.current?.(choice);
    resolveChoice.current = null;
    setPrompt(false);
  }, []);

  const runGuarded = useCallback(async (action: () => void | Promise<void>) => {
    if (running.current) return;
    if (latest.current.busy) {
      latest.current.onError("任务正在运行，请完成后再关闭、重载或更新。");
      return;
    }
    running.current = true;
    allowUnload.current = false;
    setFailure(null);
    setLeaving(true);
    try {
      if (useWorkbenchStore.getState().dirty) {
        let ask = !latest.current.autosaveEnabled;
        while (true) {
          const choice = ask ? await new Promise<ExitChoice>((resolve) => {
              resolveChoice.current = resolve;
              setPrompt(true);
            }) : "save";
          if (choice === "cancel") return;
          if (choice === "discard") break;
          try {
            await latest.current.save();
            if (useWorkbenchStore.getState().dirty) {
              throw new Error("仍有未保存修改，请保存后重试。");
            }
            break;
          } catch (error) {
            setFailure(ipcErrorMessage(error));
            ask = true;
          }
        }
      }
      allowUnload.current = true;
      await action();
    } catch (error) {
      allowUnload.current = false;
      latest.current.onError(ipcErrorMessage(error));
    } finally {
      running.current = false;
      setLeaving(false);
    }
  }, []);

  useEffect(() => {
    const unsubscribe = useWorkbenchStore.subscribe((state, previous) => {
      if (state.document !== previous.document) allowUnload.current = false;
    });
    function beforeUnload(event: BeforeUnloadEvent) {
      if (!allowUnload.current && (useWorkbenchStore.getState().dirty || latest.current.busy)) {
        event.preventDefault();
        event.returnValue = "";
      }
    }
    function reload(event: KeyboardEvent) {
      if (event.key === "F5" || ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "r")) {
        event.preventDefault();
        event.stopImmediatePropagation();
        void runGuarded(() => window.location.reload());
      }
    }
    window.addEventListener("beforeunload", beforeUnload);
    window.addEventListener("keydown", reload, true);
    return () => {
      unsubscribe();
      window.removeEventListener("beforeunload", beforeUnload);
      window.removeEventListener("keydown", reload, true);
      resolveChoice.current?.("cancel");
      resolveChoice.current = null;
    };
  }, [runGuarded]);

  useEffect(() => {
    if (!options.desktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    Promise.resolve().then(() => {
      const currentWindow = getCurrentWindow();
      return currentWindow.onCloseRequested((event) => {
        event.preventDefault();
        return runGuarded(() => currentWindow.destroy());
      });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((error) => latest.current.onError(ipcErrorMessage(error)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [options.desktop, runGuarded]);

  return { leaving, prompt, failure, decide, runGuarded };
}
