import { Grip, Minus, X } from "lucide-react";
import { useId, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";

import type { ToolWindow as ToolWindowModel } from "./model";
import type { AnyToolWindowKind, ToolWindowsApi } from "./tool-windows-context";

const buttonClass =
  "grid h-6 w-6 place-items-center text-muted hover:bg-[#1a222c] hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary";

/** A title bar button: 24px, muted (spec §9.1). */
export function TitleButton({
  label,
  onClick,
  disabled = false,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button aria-label={label} className={`${buttonClass} disabled:opacity-30 disabled:hover:bg-transparent`} disabled={disabled} onClick={onClick} title={label} type="button">
      {children}
    </button>
  );
}

function isTyping(target: EventTarget | null): target is HTMLElement {
  return target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

/** Small screens get a full-screen sheet: no dragging or resizing there. */
function isSheet(): boolean {
  return window.matchMedia?.("(max-width: 767px)").matches === true;
}

type Gesture = { kind: "move" | "resize"; startX: number; startY: number; x: number; y: number; w: number; h: number };

/**
 * One floating, non-modal tool window: a title bar to drag it by, the kind's
 * content, and a grip to resize it. Esc closes it (an input inside it only
 * loses focus first).
 */
export function ToolWindow({
  window: model,
  kind,
  api,
  focused,
  zIndex,
}: {
  window: ToolWindowModel;
  kind: AnyToolWindowKind;
  api: ToolWindowsApi;
  focused: boolean;
  zIndex: number;
}) {
  const titleId = useId();
  const gesture = useRef<Gesture | null>(null);
  const [dragging, setDragging] = useState(false);
  const title = kind.title(model.state);
  const props = {
    id: model.id,
    state: model.state,
    setState: (next: unknown) => api.setState(model.id, next),
    close: () => api.close(model.id),
    width: model.w,
  };

  function start(kindOfGesture: Gesture["kind"], event: PointerEvent<HTMLElement>) {
    if (event.button !== 0 || isSheet()) return;
    if (kindOfGesture === "move" && (event.target as HTMLElement).closest("button")) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    gesture.current = { kind: kindOfGesture, startX: event.clientX, startY: event.clientY, x: model.x, y: model.y, w: model.w, h: model.h };
    setDragging(kindOfGesture === "move");
  }

  function track(event: PointerEvent<HTMLElement>) {
    const current = gesture.current;
    if (!current) return;
    const dx = event.clientX - current.startX;
    const dy = event.clientY - current.startY;
    if (current.kind === "move") api.move(model.id, current.x + dx, current.y + dy);
    else api.resize(model.id, current.w + dx, current.h + dy);
  }

  function end(event: PointerEvent<HTMLElement>) {
    if (!gesture.current) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    gesture.current = null;
    setDragging(false);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    if (isTyping(event.target)) {
      event.target.blur();
      return;
    }
    api.close(model.id);
  }

  return (
    <div
      aria-labelledby={titleId}
      aria-modal="false"
      className="pointer-events-auto absolute flex flex-col overflow-hidden rounded border border-border bg-panel text-foreground outline-none max-md:!inset-0 max-md:!h-full max-md:!w-full max-md:rounded-none"
      data-focused={focused}
      data-tool-window={model.id}
      onKeyDown={handleKeyDown}
      onPointerDownCapture={() => {
        if (!focused) api.focus(model.id);
      }}
      role="dialog"
      style={{
        left: model.x,
        top: model.y,
        width: model.w,
        height: model.h,
        maxWidth: "calc(100vw - 8px)",
        maxHeight: "calc(100vh - 8px)",
        zIndex,
        opacity: dragging ? 0.96 : 1,
        boxShadow: dragging ? "0 20px 56px rgba(0,0,0,.72)" : "0 14px 42px rgba(0,0,0,.58)",
      }}
      tabIndex={-1}
    >
      <div
        className={`flex h-8 shrink-0 select-none items-center border-b border-border md:cursor-grab ${dragging ? "md:cursor-grabbing" : ""}`}
        data-testid="tool-window-title-bar"
        onPointerCancel={end}
        onPointerDown={(event) => start("move", event)}
        onPointerMove={track}
        onPointerUp={end}
        style={{ borderTop: `1px solid ${focused ? "var(--color-primary)" : "transparent"}` }}
      >
        <span aria-hidden="true" className="grid w-8 shrink-0 place-items-center">
          {kind.renderIcon ? kind.renderIcon(model.state, 16) : <kind.icon className="h-4 w-4 text-primary" />}
        </span>
        <h2 className={`min-w-0 flex-1 truncate text-[11px] font-bold ${focused ? "text-bright" : "text-muted"}`} id={titleId}>
          {title}
        </h2>
        <div className="flex shrink-0">
          {kind.renderTitleControls?.(props)}
          <TitleButton label={`Minimise ${title}`} onClick={() => api.minimize(model.id)}>
            <Minus aria-hidden="true" className="h-[13px] w-[13px]" />
          </TitleButton>
          <TitleButton label={`Close ${title}`} onClick={() => api.close(model.id)}>
            <X aria-hidden="true" className="h-[13px] w-[13px]" />
          </TitleButton>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">{kind.render(props)}</div>
      <div
        aria-label={`Resize ${title}`}
        aria-orientation="horizontal"
        className="absolute bottom-0 right-0 grid h-4 w-4 cursor-nwse-resize place-items-center text-subtle max-md:hidden"
        onPointerCancel={end}
        onPointerDown={(event) => start("resize", event)}
        onPointerMove={track}
        onPointerUp={end}
        role="separator"
      >
        <Grip aria-hidden="true" className="h-3 w-3" />
      </div>
    </div>
  );
}
