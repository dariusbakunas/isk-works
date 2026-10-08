import { ChevronDown } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface DropdownMenuItem {
  key: string;
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
}

/**
 * Minimal button-triggered menu (Record ▾, overflow ⋯) shared by toolbars
 * that need more actions than fit as flat buttons. No existing app-wide menu
 * primitive existed prior to this -- built to the same iw-* button/panel
 * conventions used elsewhere rather than a new visual language.
 */
export function DropdownMenu({
  align = "start",
  icon,
  items,
  label,
  portal = false,
  variant = "secondary",
}: {
  align?: "start" | "end";
  icon?: ReactNode;
  items: DropdownMenuItem[];
  label: string;
  /** Render the menu in a fixed-position portal anchored to the trigger --
   * for triggers inside containers that clip overflow (table cells, scroll
   * areas), where an in-flow absolute menu would be cut off or invisible. */
  portal?: boolean;
  variant?: "primary" | "secondary" | "icon";
}) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (rootRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    // A fixed-position menu would drift from its trigger on scroll/resize.
    function close() {
      setOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    if (portal) {
      window.addEventListener("scroll", close, true);
      window.addEventListener("resize", close);
    }
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  }, [open, portal]);

  function toggle() {
    if (!open && portal) setAnchor(rootRef.current?.getBoundingClientRect() ?? null);
    setOpen((current) => !current);
  }

  const portalStyle: CSSProperties | undefined = portal && anchor
    ? {
        position: "fixed",
        top: anchor.bottom + 4,
        ...(align === "end" ? { right: window.innerWidth - anchor.right } : { left: anchor.left }),
      }
    : undefined;

  const triggerClass = variant === "primary" ? "iw-button-primary" : variant === "icon" ? "iw-icon-button" : "iw-button-secondary";

  return (
    <div className="relative inline-block" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={variant === "icon" ? label : undefined}
        className={triggerClass}
        onClick={toggle}
        title={variant === "icon" ? label : undefined}
        type="button"
      >
        {icon}
        {variant !== "icon" ? <span className={icon ? "ml-2" : undefined}>{label}</span> : null}
        {variant !== "icon" ? <ChevronDown aria-hidden="true" className="ml-1.5 h-3.5 w-3.5" /> : null}
      </button>
      {open ? renderMenu(
        <div
          className={
            portalStyle
              ? "iw-panel-strong z-50 min-w-48 border p-1 shadow-2xl"
              : `iw-panel-strong absolute z-40 mt-1 min-w-48 border p-1 shadow-2xl ${align === "end" ? "right-0" : "left-0"}`
          }
          ref={menuRef}
          role="menu"
          style={portalStyle}
        >
          {items.map((item) => (
            <button
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs font-semibold text-foreground transition hover:bg-panel disabled:cursor-not-allowed disabled:opacity-50"
              disabled={item.disabled}
              key={item.key}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              role="menuitem"
              type="button"
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>,
        portalStyle !== undefined,
      ) : null}
    </div>
  );
}

function renderMenu(menu: ReactNode, inPortal: boolean) {
  return inPortal ? createPortal(menu, document.body) : menu;
}
