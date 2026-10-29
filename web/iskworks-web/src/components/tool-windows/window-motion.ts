import { flushSync } from "react-dom";

/**
 * The minimise/restore motion: a window shrinks into its row in the dock and
 * grows back out of it. Shared by every tool window kind, and by every way
 * in (title bar, dock, ⌘K). Purely visual: the state change never waits on
 * it. The change is flushed so the dock row (or window) can be measured and
 * the motion starts in the same frame, with no blank one between.
 */

const DURATION_MS = 220;

function canAnimate(): boolean {
  return (
    typeof Element.prototype.animate === "function" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches !== true &&
    // Below md a window is a full-screen sheet and the rail is off-canvas.
    window.matchMedia?.("(max-width: 767px)").matches !== true
  );
}

function windowNode(id: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-tool-window="${id}"]`);
}

/** Where a minimised window sits in the dock: its row, else "+N", else the dock itself. */
function dockRect(id: string): DOMRect | null {
  const target =
    document.querySelector(`[data-window-dock-item="${id}"]`) ??
    document.querySelector("[data-window-dock-more]") ??
    document.querySelector("[data-window-dock]");
  const rect = target?.getBoundingClientRect();
  return rect && rect.width > 0 && rect.height > 0 ? rect : null;
}

/** The keyframe that lays a box of `from`'s size over `to`. */
function onto(from: DOMRect, to: DOMRect, opacity: number): Keyframe {
  const sx = to.width / from.width;
  const sy = to.height / from.height;
  return {
    transform: `translate(${to.left - from.left}px, ${to.top - from.top}px) scale(${sx}, ${sy})`,
    opacity,
  };
}

/** Minimises `id`, flying a copy of the window into its new dock row. */
export function minimizeWithMotion(id: string, minimize: () => void): void {
  const node = canAnimate() ? windowNode(id) : null;
  if (!node) {
    minimize();
    return;
  }
  // Minimised mid-restore: measure the window where it's going, not where it is.
  node.getAnimations().forEach((animation) => animation.finish());
  const from = node.getBoundingClientRect();
  const ghost = node.cloneNode(true) as HTMLElement;
  ghost.removeAttribute("data-tool-window");
  ghost.removeAttribute("id");
  ghost.querySelectorAll("[id]").forEach((element) => element.removeAttribute("id"));
  ghost.setAttribute("data-tool-window-ghost", id);
  ghost.setAttribute("aria-hidden", "true");
  ghost.setAttribute("inert", "");
  Object.assign(ghost.style, {
    position: "fixed",
    left: `${from.left}px`,
    top: `${from.top}px`,
    width: `${from.width}px`,
    height: `${from.height}px`,
    margin: "0",
    zIndex: "36",
    pointerEvents: "none",
    transformOrigin: "0 0",
  });
  flushSync(minimize);
  const to = dockRect(id);
  if (!to) return;
  document.body.appendChild(ghost);
  const animation = ghost.animate([{ transform: "none", opacity: 1 }, onto(from, to, 0.15)], {
    duration: DURATION_MS,
    easing: "cubic-bezier(0.4, 0, 1, 1)",
  });
  animation.onfinish = animation.oncancel = () => ghost.remove();
}

/** Restores `id`, growing the window out of the dock row it leaves. */
export function restoreWithMotion(id: string, restore: () => void): void {
  // Restored mid-minimise: the copy still flying to the dock goes.
  document.querySelector(`[data-tool-window-ghost="${id}"]`)?.remove();
  const from = canAnimate() ? dockRect(id) : null;
  if (!from) {
    restore();
    return;
  }
  flushSync(restore);
  const node = windowNode(id);
  if (!node) return;
  const to = node.getBoundingClientRect();
  node.animate(
    [{ ...onto(to, from, 0.15), transformOrigin: "0 0" }, { transform: "none", opacity: 1, transformOrigin: "0 0" }],
    { duration: DURATION_MS, easing: "cubic-bezier(0, 0, 0.2, 1)" },
  );
}
