import { ChevronLeft, X } from "lucide-react";
import { useCallback, useEffect, useId, type ReactNode } from "react";
import { createPortal } from "react-dom";

const widthClasses = {
  default: "lg:w-72",
  wide: "lg:w-[26rem]",
} as const;

export function PlannerInspectorShell({
  backLabel = "Back",
  children,
  closeLabel = "Close item inspector",
  dismissLabel = "Dismiss item inspector",
  eyebrow = "Selected item",
  hideDefaultHeader = false,
  onBack,
  onClose,
  open,
  privateTitle = false,
  returnFocusRowKey,
  returnFocusSelector,
  title,
  width = "default",
}: {
  /** Accessible label for the back button, when `onBack` is provided. */
  backLabel?: string;
  children: ReactNode;
  closeLabel?: string;
  /** Accessible label for the mobile backdrop button. Parameterised so each
   * inspector mode (selected item, build settings, ...) reads correctly. */
  dismissLabel?: string;
  eyebrow?: string;
  /** Skips the generic eyebrow/title/close header bar -- for a consumer
   * (e.g. the character inspector's own portrait/name row) that renders a
   * richer header of its own and would otherwise show the character's name
   * twice. `title` still backs an sr-only label so the panel keeps an
   * accessible name; the consumer is responsible for its own close button. */
  hideDefaultHeader?: boolean;
  /** When provided, renders a back-chevron button in the header before the
   * eyebrow/title block -- for a consumer that opened this inspector from
   * within another one (e.g. a Ticket opened from an Epic) and wants to
   * return to that prior inspector instead of closing entirely. Unlike
   * `onClose`, this does not restore focus to `returnFocusRowKey` /
   * `returnFocusSelector` -- the caller owns what focus means on return. */
  onBack?: () => void;
  onClose: () => void;
  open: boolean;
  /** Marks the title as free-form user text (e.g. an Epic name or a generic
   * Ticket title) so it is masked from LogRocket session replay. */
  privateTitle?: boolean;
  /** Focus is returned to `[data-row-key="..."]` after any close (X, backdrop
   * or Escape), so keyboard users land back on the row they opened. */
  returnFocusRowKey?: string | null;
  /** CSS selector for the control to refocus after any close, when there is
   * no originating row (e.g. the "Edit build settings" trigger). */
  returnFocusSelector?: string;
  title: string;
  width?: "default" | "wide";
}) {
  const titleId = useId();

  const restoreFocus = useCallback(() => {
    const selector = returnFocusRowKey
      ? `[data-row-key="${returnFocusRowKey}"]`
      : returnFocusSelector;
    if (!selector) return;
    requestAnimationFrame(() => {
      document.querySelector<HTMLElement>(selector)?.focus();
    });
  }, [returnFocusRowKey, returnFocusSelector]);

  const handleClose = useCallback(() => {
    onClose();
    restoreFocus();
  }, [onClose, restoreFocus]);

  useEffect(() => {
    if (!open) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      handleClose();
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleClose, open]);

  if (!open) return null;
  const inspector = (
    <>
      <button
        aria-label={dismissLabel}
        className="fixed inset-0 z-40 bg-black/60 lg:hidden"
        onClick={handleClose}
        type="button"
      />
      <aside
        aria-labelledby={titleId}
        className={`iw-planner-inspector fixed inset-x-3 bottom-3 z-50 max-h-[70vh] overflow-y-auto border border-border bg-panel shadow-2xl lg:sticky lg:top-0 lg:z-auto lg:max-h-screen lg:border-0 lg:shadow-none ${widthClasses[width]}`}
        data-desktop-breakpoint="1024px"
        data-desktop-presentation="side-panel"
        data-mobile-presentation="bottom-sheet"
      >
        {hideDefaultHeader ? (
          <span className="sr-only" data-private={privateTitle ? "" : undefined} id={titleId}>
            {title}
          </span>
        ) : (
          <div className="sticky top-0 z-10 flex items-start justify-between gap-2 bg-panel px-3 pb-2 pt-3">
            <div className="flex min-w-0 items-start gap-1.5">
              {onBack ? (
                <button
                  aria-label={backLabel}
                  className="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
                  onClick={onBack}
                  type="button"
                >
                  <ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" />
                </button>
              ) : null}
              <div className="min-w-0">
                <span className="block text-[10px] font-semibold uppercase text-muted">{eyebrow}</span>
                <h2
                  className="truncate text-sm font-semibold"
                  data-private={privateTitle ? "" : undefined}
                  id={titleId}
                >
                  {title}
                </h2>
              </div>
            </div>
            <button
              aria-label={closeLabel}
              className="grid h-7 w-7 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
              onClick={handleClose}
              type="button"
            >
              <X aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
        {children}
      </aside>
    </>
  );
  const appRightRail = typeof document === "undefined" ? null : document.getElementById("app-right-rail");
  return appRightRail ? createPortal(inspector, appRightRail) : inspector;
}
