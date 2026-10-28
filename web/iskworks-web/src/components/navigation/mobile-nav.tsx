import { Menu, Search } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useLocation } from "react-router";

import mascotHead from "../../assets/mascot-head.png";
import { RailContent } from "./navigation-rail";
import { resolveNavEntry } from "./registry";

const iconButton =
  "grid h-8 w-8 shrink-0 place-items-center text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary";

const FOCUSABLE = "a[href], button:not([disabled]), input, select, textarea, [tabindex]:not([tabindex='-1'])";

/**
 * Narrow-screen navigation (below md): a 44px top bar with the current
 * section's name, and a drawer holding the rail's contents fully expanded.
 */
export function MobileNav({
  isAdmin,
  workspaceName,
  renderAccount,
  onOpenSearch,
}: {
  isAdmin: boolean;
  workspaceName: string;
  renderAccount?: (expanded: boolean) => ReactNode;
  onOpenSearch?: () => void;
}) {
  const { pathname } = useLocation();
  const [open, setOpen] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  const drawer = useRef<HTMLDivElement>(null);
  const entry = resolveNavEntry(pathname);

  function close() {
    setOpen(false);
    menuButton.current?.focus();
  }

  // Following a link closes the drawer; the new page is what you came for.
  useEffect(() => setOpen(false), [pathname]);

  useEffect(() => {
    if (open) drawer.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
  }, [open]);

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.stopPropagation();
      close();
      return;
    }
    if (event.key !== "Tab" || !drawer.current) return;
    const focusable = [...drawer.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }

  return (
    <>
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-sidebar px-2 md:hidden">
        <button aria-label="Open navigation" className={iconButton} onClick={() => setOpen(true)} ref={menuButton} type="button">
          <Menu aria-hidden="true" className="h-[17px] w-[17px]" />
        </button>
        <img alt="" aria-hidden="true" className="h-[25px] w-[25px]" src={mascotHead} />
        <strong className="truncate text-xs text-bright" data-testid="mobile-nav-title">
          {entry?.parent?.label ?? entry?.label ?? "ISK Works"}
        </strong>
        {onOpenSearch ? (
          <button aria-label="Search" className={`ml-auto ${iconButton}`} onClick={onOpenSearch} type="button">
            <Search aria-hidden="true" className="h-4 w-4" />
          </button>
        ) : null}
      </div>
      {open ? (
        <div className="fixed inset-0 z-[70] flex bg-[rgba(2,4,7,.72)] md:hidden" data-testid="mobile-nav-scrim" onClick={close}>
          <div
            aria-label="Navigation"
            aria-modal="true"
            className="flex h-full w-[280px] flex-col border-r border-border bg-sidebar"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={handleKeyDown}
            ref={drawer}
            role="dialog"
          >
            <RailContent
              expanded
              isAdmin={isAdmin}
              onOpenSearch={
                onOpenSearch
                  ? () => {
                      setOpen(false);
                      onOpenSearch();
                    }
                  : undefined
              }
              renderAccount={renderAccount}
              workspaceName={workspaceName}
            />
          </div>
        </div>
      ) : null}
    </>
  );
}
