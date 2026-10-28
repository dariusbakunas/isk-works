// Floating tool windows (spec 2026-10-28-type-info §4): which are open, where,
// in what order, and what each kind keeps in its own state. Pure, so the
// provider only wires it to React and storage.

export interface ToolWindow {
  id: string;
  /** The registered kind that renders it ("type-info", …). */
  kind: string;
  /** Kind-owned and JSON-serialisable. */
  state: unknown;
  x: number;
  y: number;
  w: number;
  h: number;
  minimized: boolean;
}

export interface ToolWindowsState {
  /** Back to front: the last window that isn't minimised is in front. */
  windows: ToolWindow[];
  nextId: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Size {
  w: number;
  h: number;
}

/** Opening a ninth closes one (spec §4.1). */
export const MAX_WINDOWS = 8;
/** How much of the title bar always stays on screen. */
const KEEP_VISIBLE = 48;
const TITLE_BAR_HEIGHT = 32;
const CASCADE_STEP = 24;
const CASCADE_STEPS = 6;
const TOP = 72;
/** Clear of the collapsed rail. */
const LEFT_LIMIT = 60;

export const emptyToolWindows: ToolWindowsState = { windows: [], nextId: 1 };

export type ToolWindowsAction =
  | {
      type: "open";
      kind: string;
      state: unknown;
      size: Size;
      viewport: Viewport;
      /** `front-of-kind` merges into the front window of the kind, if one is open. */
      reuse: "front-of-kind" | "new";
      merge?: (current: unknown, incoming: unknown) => unknown;
    }
  | { type: "focus"; id: string }
  | { type: "minimize"; id: string }
  | { type: "restore"; id: string; viewport: Viewport }
  | { type: "close"; id: string }
  | { type: "move"; id: string; x: number; y: number; viewport: Viewport }
  | { type: "resize"; id: string; w: number; h: number; min: Size }
  | { type: "setState"; id: string; state: unknown };

/** The window in front, if any isn't minimised. */
export function frontWindow(state: ToolWindowsState): ToolWindow | undefined {
  for (let index = state.windows.length - 1; index >= 0; index -= 1) {
    if (!state.windows[index].minimized) return state.windows[index];
  }
  return undefined;
}

export function clampToViewport(
  window: Pick<ToolWindow, "x" | "y" | "w" | "h">,
  viewport: Viewport,
): { x: number; y: number } {
  return {
    x: Math.round(Math.min(Math.max(window.x, KEEP_VISIBLE - window.w), viewport.width - KEEP_VISIBLE)),
    y: Math.round(Math.min(Math.max(window.y, 0), viewport.height - TITLE_BAR_HEIGHT)),
  };
}

function toFront(windows: ToolWindow[], id: string, change: Partial<ToolWindow> = {}): ToolWindow[] {
  const window = windows.find((candidate) => candidate.id === id);
  if (!window) return windows;
  return [...windows.filter((candidate) => candidate.id !== id), { ...window, ...change }];
}

function update(state: ToolWindowsState, id: string, change: (window: ToolWindow) => Partial<ToolWindow>): ToolWindowsState {
  const index = state.windows.findIndex((window) => window.id === id);
  if (index < 0) return state;
  const windows = [...state.windows];
  windows[index] = { ...windows[index], ...change(windows[index]) };
  return { ...state, windows };
}

/** Makes room for one more: the oldest minimised window, else the least recently focused. */
function evict(windows: ToolWindow[]): ToolWindow[] {
  if (windows.length < MAX_WINDOWS) return windows;
  const victim = windows.find((window) => window.minimized) ?? windows[0];
  return windows.filter((window) => window !== victim);
}

export function toolWindowsReducer(state: ToolWindowsState, action: ToolWindowsAction): ToolWindowsState {
  switch (action.type) {
    case "open": {
      if (action.reuse === "front-of-kind") {
        const target = [...state.windows].reverse().find((window) => window.kind === action.kind && !window.minimized);
        if (target) {
          const merged = action.merge ? action.merge(target.state, action.state) : action.state;
          return { ...state, windows: toFront(state.windows, target.id, { state: merged }) };
        }
      }
      const step = state.windows.filter((window) => !window.minimized).length % CASCADE_STEPS;
      const position = clampToViewport(
        {
          x: Math.max(LEFT_LIMIT, action.viewport.width - action.size.w - CASCADE_STEP * (step + 1)),
          y: TOP + CASCADE_STEP * step,
          ...action.size,
        },
        action.viewport,
      );
      const window: ToolWindow = {
        id: `w${state.nextId}`,
        kind: action.kind,
        state: action.state,
        ...action.size,
        ...position,
        minimized: false,
      };
      return { windows: [...evict(state.windows), window], nextId: state.nextId + 1 };
    }
    case "focus": {
      const window = state.windows.find((candidate) => candidate.id === action.id);
      if (!window || frontWindow(state) === window) return state;
      return { ...state, windows: toFront(state.windows, action.id) };
    }
    case "minimize":
      return update(state, action.id, () => ({ minimized: true }));
    case "restore": {
      const window = state.windows.find((candidate) => candidate.id === action.id);
      if (!window) return state;
      return {
        ...state,
        windows: toFront(state.windows, action.id, { minimized: false, ...clampToViewport(window, action.viewport) }),
      };
    }
    case "close": {
      if (!state.windows.some((window) => window.id === action.id)) return state;
      return { ...state, windows: state.windows.filter((window) => window.id !== action.id) };
    }
    case "move":
      return update(state, action.id, (window) => clampToViewport({ ...window, x: action.x, y: action.y }, action.viewport));
    case "resize":
      return update(state, action.id, () => ({
        w: Math.round(Math.max(action.min.w, action.w)),
        h: Math.round(Math.max(action.min.h, action.h)),
      }));
    case "setState":
      return update(state, action.id, () => ({ state: action.state }));
  }
}

function isWindow(value: unknown): value is ToolWindow {
  if (!value || typeof value !== "object") return false;
  const window = value as Record<string, unknown>;
  return (
    typeof window.id === "string" &&
    typeof window.kind === "string" &&
    ["x", "y", "w", "h"].every((key) => typeof window[key] === "number" && Number.isFinite(window[key])) &&
    typeof window.minimized === "boolean"
  );
}

/** Reads stored windows, keeping well-formed ones of a kind that still exists. */
export function parseStoredWindows(raw: string | null, kinds: ReadonlySet<string>): ToolWindowsState {
  if (!raw) return emptyToolWindows;
  try {
    const parsed = JSON.parse(raw) as { windows?: unknown; nextId?: unknown };
    if (!Array.isArray(parsed.windows)) return emptyToolWindows;
    const windows = parsed.windows.filter(isWindow).filter((window) => kinds.has(window.kind)).slice(-MAX_WINDOWS);
    const highest = windows.reduce((max, window) => Math.max(max, Number(window.id.slice(1)) || 0), 0);
    const nextId = typeof parsed.nextId === "number" && parsed.nextId > highest ? parsed.nextId : highest + 1;
    return { windows, nextId };
  } catch {
    return emptyToolWindows;
  }
}
