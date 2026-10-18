// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useModal } from "./useModal";

function Dialog({
  open,
  onClose,
  label,
  locked = false,
}: {
  open: boolean;
  onClose: () => void;
  label: string;
  locked?: boolean;
}) {
  const sectionRef = useModal<HTMLElement>(open, onClose, locked);
  if (!open) return null;
  return (
    <section ref={sectionRef} tabIndex={-1} role="dialog" aria-label={label}>
      <button type="button">{label} 一</button>
      <button type="button">{label} 二</button>
    </section>
  );
}

afterEach(() => cleanup());

describe("useModal", () => {
  it("moves focus into the dialog and restores the trigger afterwards", () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>打开</button>
          <Dialog open={open} onClose={() => setOpen(false)} label="测试弹窗" />
        </>
      );
    }

    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "打开" });
    trigger.focus();
    fireEvent.click(trigger);

    const dialog = screen.getByRole("dialog", { name: "测试弹窗" });
    expect(document.activeElement).toBe(dialog);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("traps Tab inside the dialog in both directions", () => {
    render(<Dialog open onClose={() => {}} label="陷阱" />);
    const first = screen.getByRole("button", { name: "陷阱 一" });
    const last = screen.getByRole("button", { name: "陷阱 二" });

    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(document.activeElement).toBe(first);

    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it("ignores Escape while locked but still traps focus", () => {
    const onClose = vi.fn();
    render(<Dialog open onClose={onClose} label="锁定" locked />);
    const dialog = screen.getByRole("dialog", { name: "锁定" });
    expect(document.activeElement).toBe(dialog);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("lets only the top-most dialog handle Escape", () => {
    const closeUnder = vi.fn();
    const closeTop = vi.fn();

    const { rerender } = render(
      <>
        <Dialog open onClose={closeUnder} label="底层" />
        <Dialog open onClose={closeTop} label="上层" />
      </>,
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(closeTop).toHaveBeenCalledTimes(1);
    expect(closeUnder).not.toHaveBeenCalled();

    rerender(<Dialog open onClose={closeUnder} label="底层" />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(closeUnder).toHaveBeenCalledTimes(1);
  });
});
