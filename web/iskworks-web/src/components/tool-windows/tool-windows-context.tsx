import type { LucideIcon } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, type ReactNode } from "react";

import {
  emptyToolWindows,
  frontWindow,
  parseStoredWindows,
  toolWindowsReducer,
  type Size,
  type ToolWindow,
  type ToolWindowsState,
} from "./model";

// Per-browser convenience, like the pinned rail: open and minimised windows
// come back after a reload. Never needed for anything to work.
export const STORAGE_KEY = "iskworks:tool-windows";
const SAVE_DELAY_MS = 250;

export interface ToolWindowRenderProps<S> {
  id: string;
  state: S;
  setState: (next: S) => void;
  close: () => void;
  /** The window's current width, for layouts that change when narrow. */
  width: number;
}

/** A kind of tool window (Type Info, later calculators), registered with the provider. */
export interface ToolWindowKind<S> {
  kind: string;
  icon: LucideIcon;
  /** Shown in the title bar and the dock. */
  title: (state: S) => string;
  /** The dock and title bar icon; the kind's `icon` otherwise. */
  renderIcon?: (state: S, size: number) => ReactNode;
  /** Kind-specific title bar buttons, before Minimise (Type Info: back/forward). */
  renderTitleControls?: (props: ToolWindowRenderProps<S>) => ReactNode;
  defaultSize: Size;
  minSize: Size;
  /** Folds an incoming open into the front window of the kind (Type Info: push history). */
  merge?: (current: S, incoming: S) => S;
  render: (props: ToolWindowRenderProps<S>) => ReactNode;
}

// The registry holds kinds of different state types side by side.
export type AnyToolWindowKind = ToolWindowKind<any>;

export interface ToolWindowsApi {
  windows: ToolWindow[];
  front: ToolWindow | undefined;
  kinds: ReadonlyMap<string, AnyToolWindowKind>;
  /** Opens `kind`; by default it reuses the front window of the kind (spec §5.1). */
  open: (kind: string, state: unknown, options?: { newWindow?: boolean }) => void;
  focus: (id: string) => void;
  minimize: (id: string) => void;
  restore: (id: string) => void;
  close: (id: string) => void;
  move: (id: string, x: number, y: number) => void;
  resize: (id: string, w: number, h: number) => void;
  setState: (id: string, state: unknown) => void;
}

const ToolWindowsContext = createContext<ToolWindowsApi | null>(null);

function viewport() {
  return { width: window.innerWidth, height: window.innerHeight };
}

function readStored(kinds: ReadonlySet<string>): ToolWindowsState {
  try {
    return parseStoredWindows(window.localStorage.getItem(STORAGE_KEY), kinds);
  } catch {
    return emptyToolWindows;
  }
}

function writeStored(state: ToolWindowsState): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage unavailable: the windows still work, they just aren't remembered.
  }
}

/** Owns the app's tool windows; mounted in the shell so they survive navigation. */
export function ToolWindowsProvider({ kinds, children }: { kinds: AnyToolWindowKind[]; children: ReactNode }) {
  const registry = useMemo(() => new Map(kinds.map((kind) => [kind.kind, kind])), [kinds]);
  const [state, dispatch] = useReducer(toolWindowsReducer, undefined, () => readStored(new Set(registry.keys())));

  // Moves and resizes come in per pointer event; save once they settle.
  const saveTimer = useRef<ReturnType<typeof setTimeout>>();
  useEffect(() => {
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => writeStored(state), SAVE_DELAY_MS);
  }, [state]);
  useEffect(() => () => clearTimeout(saveTimer.current), []);

  const minSize = useCallback((kind: string): Size => registry.get(kind)?.minSize ?? { w: 360, h: 280 }, [registry]);
  const front = frontWindow(state);

  // ⌘` / Ctrl ` moves focus to the next open window.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.key !== "`") return;
      const open = state.windows.filter((window) => !window.minimized);
      if (open.length === 0) return;
      event.preventDefault();
      // The back-most open window comes to the front, cycling through them all.
      const next = open[0];
      dispatch({ type: "focus", id: next.id });
      requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-tool-window="${next.id}"]`)?.focus());
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state.windows]);

  const api = useMemo<ToolWindowsApi>(
    () => ({
      windows: state.windows,
      front,
      kinds: registry,
      open: (kind, windowState, options) => {
        const definition = registry.get(kind);
        if (!definition) return;
        dispatch({
          type: "open",
          kind,
          state: windowState,
          size: {
            w: Math.min(definition.defaultSize.w, window.innerWidth - 8),
            h: Math.min(definition.defaultSize.h, window.innerHeight - 8),
          },
          viewport: viewport(),
          reuse: options?.newWindow ? "new" : "front-of-kind",
          merge: definition.merge as ((current: unknown, incoming: unknown) => unknown) | undefined,
        });
      },
      focus: (id) => dispatch({ type: "focus", id }),
      minimize: (id) => dispatch({ type: "minimize", id }),
      restore: (id) => dispatch({ type: "restore", id, viewport: viewport() }),
      close: (id) => dispatch({ type: "close", id }),
      move: (id, x, y) => dispatch({ type: "move", id, x, y, viewport: viewport() }),
      resize: (id, w, h) => {
        const kind = state.windows.find((window) => window.id === id)?.kind;
        dispatch({ type: "resize", id, w, h, min: minSize(kind ?? "") });
      },
      setState: (id, windowState) => dispatch({ type: "setState", id, state: windowState }),
    }),
    [front, minSize, registry, state.windows],
  );

  return <ToolWindowsContext.Provider value={api}>{children}</ToolWindowsContext.Provider>;
}

/** The tool windows API; `null` outside a provider (isolated component tests). */
export function useToolWindows(): ToolWindowsApi | null {
  return useContext(ToolWindowsContext);
}
