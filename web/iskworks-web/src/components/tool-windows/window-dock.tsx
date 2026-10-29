import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { ToolWindow } from "./model";
import { useToolWindows, type AnyToolWindowKind } from "./tool-windows-context";

const rowFocus = "focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary";
/** Beyond this many, the collapsed dock shows one fewer and a "+N" menu. */
const MAX_DOCK_ROWS = 4;

function WindowIcon({ kind, window, size }: { kind: AnyToolWindowKind; window: ToolWindow; size: number }) {
  return kind.renderIcon ? (
    <>{kind.renderIcon(window.state, size)}</>
  ) : (
    <kind.icon aria-hidden="true" className="text-primary" style={{ width: size, height: size }} />
  );
}

/**
 * Minimised tool windows, in the navigation rail between the page groups and
 * the footer (spec §4.3, §9.1). Hidden when nothing is minimised; open
 * windows aren't listed.
 */
export function WindowDock({ expanded }: { expanded: boolean }) {
  const api = useToolWindows();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const minimized = api?.windows.filter((window) => window.minimized) ?? [];

  useEffect(() => {
    if (!menuOpen) return;
    function handlePointerDown(event: PointerEvent) {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown);
    return () => document.removeEventListener("pointerdown", handlePointerDown);
  }, [menuOpen]);

  useEffect(() => {
    if (minimized.length <= MAX_DOCK_ROWS) setMenuOpen(false);
  }, [minimized.length]);

  if (!api || minimized.length === 0) return null;
  const shown = minimized.length > MAX_DOCK_ROWS ? minimized.slice(0, MAX_DOCK_ROWS - 1) : minimized;
  const rest = minimized.slice(shown.length);

  return (
    <nav aria-label="Windows" className="relative shrink-0 border-t border-border py-1" data-window-dock="">
      {expanded ? (
        <div className="flex h-5 items-center px-3 text-[8px] uppercase tracking-[.12em] text-subtle">Windows</div>
      ) : (
        <span className="sr-only">Windows</span>
      )}
      {shown.map((window) => {
        const kind = api.kinds.get(window.kind);
        if (!kind) return null;
        const title = kind.title(window.state);
        return (
          <div className="group relative flex h-8 w-full items-center hover:bg-hover" data-window-dock-item={window.id} key={window.id}>
            <button
              aria-label={`Restore ${title}`}
              className={`flex h-full min-w-0 flex-1 items-center text-left ${rowFocus}`}
              onClick={() => api.restore(window.id)}
              title={expanded ? undefined : title}
              type="button"
            >
              <span className="grid w-[51px] shrink-0 place-items-center">
                <WindowIcon kind={kind} size={18} window={window} />
              </span>
              {expanded ? <span className="truncate text-[9px] text-foreground">{title}</span> : null}
            </button>
            {expanded ? (
              <button
                aria-label={`Close ${title}`}
                className={`mr-2 grid h-5 w-5 shrink-0 place-items-center text-subtle opacity-0 hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100 ${rowFocus}`}
                onClick={() => api.close(window.id)}
                type="button"
              >
                <X aria-hidden="true" className="h-2.5 w-2.5" />
              </button>
            ) : null}
          </div>
        );
      })}
      {rest.length > 0 ? (
        <div ref={menuRef}>
          <button
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            aria-label={`${rest.length} more windows`}
            className={`flex h-8 w-full items-center hover:bg-hover ${rowFocus}`}
            data-window-dock-more=""
            onClick={() => setMenuOpen((open) => !open)}
            type="button"
          >
            <span className="grid w-[51px] shrink-0 place-items-center font-mono text-[9px] text-muted">+{rest.length}</span>
            {expanded ? <span className="text-[9px] text-subtle">More windows</span> : null}
          </button>
          {menuOpen ? (
            <div
              aria-label="More windows"
              className="absolute bottom-0 left-[48px] z-50 w-[210px] border border-border bg-[#151b23] py-1 shadow-2xl"
              role="menu"
            >
              {rest.map((window) => {
                const kind = api.kinds.get(window.kind);
                if (!kind) return null;
                const title = kind.title(window.state);
                return (
                  <button
                    className={`flex h-8 w-full items-center gap-2 px-2 text-left hover:bg-[#1a222c] ${rowFocus}`}
                    key={window.id}
                    onClick={() => {
                      setMenuOpen(false);
                      api.restore(window.id);
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <WindowIcon kind={kind} size={18} window={window} />
                    <span className="truncate text-[9px] text-foreground">{title}</span>
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>
      ) : null}
    </nav>
  );
}
