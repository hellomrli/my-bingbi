// @vitest-environment jsdom

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDocumentLifecycle } from "./useDocumentLifecycle";
import { useWorkbenchStore } from "../store/workbench";

function setup(overrides: Partial<Parameters<typeof useDocumentLifecycle>[0]> = {}) {
  const save = vi.fn(async () => {
    useWorkbenchStore.getState().markSaved();
  });
  const onError = vi.fn();
  const hook = renderHook(() => useDocumentLifecycle({
    desktop: false,
    autosaveEnabled: true,
    busy: false,
    save,
    onError,
    ...overrides,
  }));
  return { save, onError, hook };
}

beforeEach(() => {
  useWorkbenchStore.setState(useWorkbenchStore.getInitialState(), true);
});

afterEach(() => cleanup());

describe("useDocumentLifecycle", () => {
  it("runs the exit action directly when there is nothing to save", async () => {
    const { save, hook } = setup();
    const action = vi.fn();

    await act(async () => {
      await hook.result.current.runGuarded(action);
    });

    expect(action).toHaveBeenCalledTimes(1);
    expect(save).not.toHaveBeenCalled();
    expect(hook.result.current.leaving).toBe(false);
  });

  it("saves dirty work before continuing when autosave is on", async () => {
    useWorkbenchStore.setState({ dirty: true });
    const { save, hook } = setup();
    const action = vi.fn();

    await act(async () => {
      await hook.result.current.runGuarded(action);
    });

    expect(save).toHaveBeenCalledTimes(1);
    expect(useWorkbenchStore.getState().dirty).toBe(false);
    expect(action).toHaveBeenCalledTimes(1);
  });

  it("asks before discarding dirty work when autosave is off", async () => {
    useWorkbenchStore.setState({ dirty: true });
    const { save, hook } = setup({ autosaveEnabled: false });
    const action = vi.fn();
    let pending: Promise<void> | undefined;

    act(() => {
      pending = hook.result.current.runGuarded(action);
    });
    await waitFor(() => expect(hook.result.current.prompt).toBe(true));
    expect(save).not.toHaveBeenCalled();

    act(() => hook.result.current.decide("discard"));
    await act(async () => {
      await pending;
    });

    expect(action).toHaveBeenCalledTimes(1);
    expect(hook.result.current.prompt).toBe(false);
  });

  it("keeps the document open and offers a retry when saving fails", async () => {
    useWorkbenchStore.setState({ dirty: true });
    const failingSave = vi.fn(async () => {
      throw new Error("磁盘空间不足");
    });
    const { hook } = setup({ save: failingSave });
    const action = vi.fn();
    let pending: Promise<void> | undefined;

    act(() => {
      pending = hook.result.current.runGuarded(action);
    });
    await waitFor(() => expect(hook.result.current.failure).toBe("磁盘空间不足"));
    expect(hook.result.current.prompt).toBe(true);
    expect(action).not.toHaveBeenCalled();
    expect(hook.result.current.leaving).toBe(true);

    act(() => hook.result.current.decide("cancel"));
    await act(async () => {
      await pending;
    });

    expect(failingSave).toHaveBeenCalledTimes(1);
    expect(action).not.toHaveBeenCalled();
    expect(hook.result.current.leaving).toBe(false);
  });

  it("refuses to exit while a generation is running", async () => {
    const { onError, hook } = setup({ busy: true });
    const action = vi.fn();

    await act(async () => {
      await hook.result.current.runGuarded(action);
    });

    expect(action).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("任务正在运行"));
  });

  it("reports an action failure and releases the exit guard", async () => {
    const { onError, hook } = setup();
    const action = vi.fn(async () => {
      throw new Error("窗口关闭失败");
    });

    await act(async () => {
      await hook.result.current.runGuarded(action);
    });

    expect(onError).toHaveBeenCalledWith("窗口关闭失败");
    expect(hook.result.current.leaving).toBe(false);
  });

  it("blocks a native unload while the document is dirty", () => {
    useWorkbenchStore.setState({ dirty: true });
    setup();

    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });
});
