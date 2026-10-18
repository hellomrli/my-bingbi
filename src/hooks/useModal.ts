import { useEffect, useRef } from "react";

/**
 * Open dialogs share one stack so keyboard handling belongs to the top-most dialog only:
 * Escape must close the dialog the user actually sees, not every dialog underneath it.
 */
const modalStack: symbol[] = [];

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "audio[controls]",
  "video[controls]",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

/** Tab order candidates inside a dialog, skipping nodes explicitly hidden from assistive tech. */
function focusableWithin(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)].filter(
    (element) => element.getAttribute("aria-hidden") !== "true" && !element.closest("[inert]"),
  );
}

/**
 * Shared modal behavior for the `role="dialog"` sections: Escape closes the top-most dialog
 * (unless `locked`, e.g. while an archive or restore is in flight), Tab is trapped inside it,
 * focus moves into the dialog when it opens, and returns to the previously focused element
 * when the dialog closes.
 *
 * Attach the returned ref to the dialog section and give it `tabIndex={-1}` so it can
 * receive focus.
 */
export function useModal<Section extends HTMLElement>(
  open: boolean,
  onClose: () => void,
  locked = false,
): React.RefObject<Section | null> {
  const sectionRef = useRef<Section | null>(null);
  const idRef = useRef<symbol>(Symbol("modal"));
  // Callbacks are read through refs so an inline `onClose` never re-registers the listeners
  // (and never disturbs the stack order) on every render.
  const onCloseRef = useRef(onClose);
  const lockedRef = useRef(locked);
  onCloseRef.current = onClose;
  lockedRef.current = locked;

  useEffect(() => {
    if (!open) return;
    const id = idRef.current;
    modalStack.push(id);
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // Capture the node now: the ref is cleared before the cleanup runs on unmount.
    const section = sectionRef.current;
    // Focus the dialog itself: screen readers announce its labelled title, and the Tab trap
    // starts from a deterministic position even before the user tabs.
    section?.focus();
    return () => {
      const index = modalStack.lastIndexOf(id);
      if (index !== -1) modalStack.splice(index, 1);
      // Do not steal focus from a dialog that opened on top of this one, or from the page
      // after the trigger element disappeared.
      const active = document.activeElement;
      const ownsFocus = !active || active === document.body || section?.contains(active) === true;
      if (ownsFocus && previous?.isConnected) previous.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const id = idRef.current;
    const isTopmost = () => modalStack[modalStack.length - 1] === id;

    function handleKeydown(event: KeyboardEvent) {
      if (!isTopmost()) return;
      if (event.key === "Escape") {
        if (!lockedRef.current) {
          event.preventDefault();
          event.stopPropagation();
          onCloseRef.current();
        }
        return;
      }
      if (event.key !== "Tab") return;
      const section = sectionRef.current;
      if (!section) return;
      const items = focusableWithin(section);
      if (items.length === 0) {
        event.preventDefault();
        section.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      // Focus starts on the section itself (or has escaped it), so both directions have to be
      // resolved explicitly; otherwise the browser would leave the dialog's subtree.
      const outsideItems = !active || active === section || !section.contains(active);
      if (event.shiftKey) {
        if (outsideItems || active === first) {
          event.preventDefault();
          last.focus();
        }
        return;
      }
      if (outsideItems || active === last) {
        event.preventDefault();
        first.focus();
      }
    }

    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  }, [open]);

  return sectionRef;
}
