import { useEffect, useRef } from "react";

/**
 * Shared modal behavior for the `role="dialog"` sections: Escape closes (unless `locked`,
 * e.g. while an archive or restore is in flight), focus moves into the dialog when it opens,
 * and returns to the previously focused element when it closes.
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

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    sectionRef.current?.focus();
    return () => previous?.focus();
  }, [open]);

  useEffect(() => {
    if (!open || locked) return;
    function handleKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  }, [locked, onClose, open]);

  return sectionRef;
}
