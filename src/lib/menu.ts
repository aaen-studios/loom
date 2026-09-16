import { useEffect, type RefObject } from "react";
import { useUi } from "../stores/ui";

/**
 * The top edge a popup must stay below. Menus opened from the composer are
 * clipped by the chat panel's `overflow-hidden`, not the window, so measuring
 * `innerHeight` alone would let a tall menu poke past that line.
 */
export function clipTop(node: HTMLElement | null): number {
  let top = 0;
  for (let el = node?.parentElement ?? null; el; el = el.parentElement) {
    if (getComputedStyle(el).overflowY !== "visible") {
      top = Math.max(top, el.getBoundingClientRect().top);
    }
  }
  return top;
}

/**
 * Shared behaviour for the transient selectors (model, persona, workspace,
 * permission, agent mode): only one is open at a time, a click outside the
 * trigger + popover closes it, and so does Escape.
 */
export function useMenu(id: string, containerRef: RefObject<HTMLElement | null>) {
  const open = useUi((state) => state.openMenu === id);
  const setOpenMenu = useUi((state) => state.setOpenMenu);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpenMenu(null);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpenMenu(null);
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, containerRef, setOpenMenu]);

  return {
    open,
    /** Opening closes whatever other menu was open. */
    setOpen: (next: boolean) => setOpenMenu(next ? id : null),
    close: () => setOpenMenu(null),
  };
}
