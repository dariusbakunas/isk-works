import { describe, expect, test } from "vitest";

import {
  MAX_WINDOWS,
  clampToViewport,
  emptyToolWindows,
  frontWindow,
  parseStoredWindows,
  toolWindowsReducer,
  type ToolWindowsState,
} from "../model";

const viewport = { width: 1440, height: 900 };
const size = { w: 480, h: 560 };

function open(
  state: ToolWindowsState,
  kind = "type-info",
  windowState: unknown = { typeId: 1 },
  reuse: "front-of-kind" | "new" = "new",
  merge?: (current: unknown, incoming: unknown) => unknown,
) {
  return toolWindowsReducer(state, { type: "open", kind, state: windowState, size, viewport, reuse, merge });
}

describe("opening", () => {
  test("a new window opens near the top right, cascading", () => {
    const one = open(emptyToolWindows);
    const two = open(one);
    expect(one.windows).toHaveLength(1);
    expect(one.windows[0]).toMatchObject({ kind: "type-info", minimized: false, w: 480, h: 560, x: 1440 - 480 - 24, y: 72 });
    expect(two.windows[1]).toMatchObject({ x: 1440 - 480 - 48, y: 96 });
    expect(new Set(two.windows.map((window) => window.id)).size).toBe(2);
    expect(frontWindow(two)?.id).toBe(two.windows[1].id);
  });

  test("reusing merges into the front window of the kind instead of opening one", () => {
    const merge = (current: unknown, incoming: unknown) => ({ history: [current, incoming] });
    const first = open(emptyToolWindows, "type-info", { typeId: 1 });
    const withOther = open(first, "calculator", {});
    const reused = open(withOther, "type-info", { typeId: 2 }, "front-of-kind", merge);

    expect(reused.windows).toHaveLength(2);
    const info = reused.windows.find((window) => window.kind === "type-info");
    expect(info?.state).toEqual({ history: [{ typeId: 1 }, { typeId: 2 }] });
    // It comes to the front.
    expect(frontWindow(reused)?.kind).toBe("type-info");
  });

  test("reusing skips minimised windows and opens a new one", () => {
    const first = open(emptyToolWindows);
    const minimized = toolWindowsReducer(first, { type: "minimize", id: first.windows[0].id });
    const next = open(minimized, "type-info", { typeId: 2 }, "front-of-kind", (_, incoming) => incoming);
    expect(next.windows).toHaveLength(2);
  });

  test(`at most ${MAX_WINDOWS}: the oldest minimised window goes first, then the least recently focused`, () => {
    let state = emptyToolWindows;
    for (let index = 0; index < MAX_WINDOWS; index += 1) state = open(state, "type-info", { typeId: index });
    const second = state.windows[1].id;
    state = toolWindowsReducer(state, { type: "minimize", id: second });
    state = open(state, "type-info", { typeId: 99 });
    expect(state.windows).toHaveLength(MAX_WINDOWS);
    expect(state.windows.some((window) => window.id === second)).toBe(false);

    const oldest = state.windows[0].id;
    state = open(state, "type-info", { typeId: 100 });
    expect(state.windows.some((window) => window.id === oldest)).toBe(false);
  });
});

describe("focus, minimise, restore, close", () => {
  test("focusing brings a window to the front", () => {
    const state = open(open(emptyToolWindows));
    const back = state.windows[0].id;
    const focused = toolWindowsReducer(state, { type: "focus", id: back });
    expect(frontWindow(focused)?.id).toBe(back);
  });

  test("a minimised window is never the front one; restoring brings it back in front", () => {
    const state = open(open(emptyToolWindows));
    const front = state.windows[1].id;
    const minimized = toolWindowsReducer(state, { type: "minimize", id: front });
    expect(frontWindow(minimized)?.id).toBe(state.windows[0].id);
    const restored = toolWindowsReducer(minimized, { type: "restore", id: front, viewport });
    expect(restored.windows.find((window) => window.id === front)?.minimized).toBe(false);
    expect(frontWindow(restored)?.id).toBe(front);
  });

  test("closing removes the window", () => {
    const state = open(emptyToolWindows);
    expect(toolWindowsReducer(state, { type: "close", id: state.windows[0].id }).windows).toEqual([]);
  });

  test("an unknown id changes nothing", () => {
    const state = open(emptyToolWindows);
    expect(toolWindowsReducer(state, { type: "focus", id: "nope" })).toBe(state);
  });
});

describe("geometry", () => {
  test("moving keeps 48px of the title bar on screen", () => {
    const state = open(emptyToolWindows);
    const id = state.windows[0].id;
    const moved = toolWindowsReducer(state, { type: "move", id, x: 5000, y: -200, viewport });
    expect(moved.windows[0]).toMatchObject({ x: 1440 - 48, y: 0 });
    const left = toolWindowsReducer(state, { type: "move", id, x: -5000, y: 5000, viewport });
    expect(left.windows[0]).toMatchObject({ x: 48 - 480, y: 900 - 32 });
  });

  test("resizing respects the minimum size", () => {
    const state = open(emptyToolWindows);
    const id = state.windows[0].id;
    const resized = toolWindowsReducer(state, { type: "resize", id, w: 100, h: 100, min: { w: 360, h: 280 } });
    expect(resized.windows[0]).toMatchObject({ w: 360, h: 280 });
  });

  test("clamping pulls a window saved on a bigger screen back into view", () => {
    expect(clampToViewport({ x: 3000, y: 2000, w: 480, h: 560 }, { width: 1280, height: 720 })).toEqual({
      x: 1280 - 48,
      y: 720 - 32,
    });
  });

  test("updating a window's state keeps its geometry", () => {
    const state = open(emptyToolWindows);
    const id = state.windows[0].id;
    const updated = toolWindowsReducer(state, { type: "setState", id, state: { typeId: 2 } });
    expect(updated.windows[0]).toMatchObject({ state: { typeId: 2 }, x: state.windows[0].x });
  });
});

describe("stored windows", () => {
  test("round-trip, dropping unknown kinds and malformed entries", () => {
    const state = open(open(emptyToolWindows), "gone-kind", {});
    const stored = JSON.parse(JSON.stringify(state));
    stored.windows.push({ id: 7, kind: "type-info" });
    const parsed = parseStoredWindows(JSON.stringify(stored), new Set(["type-info"]));
    expect(parsed.windows.map((window) => window.kind)).toEqual(["type-info"]);
    expect(parsed.nextId).toBe(state.nextId);
  });

  test("garbage reads as no windows", () => {
    expect(parseStoredWindows("not json", new Set(["type-info"]))).toEqual(emptyToolWindows);
    expect(parseStoredWindows(null, new Set(["type-info"]))).toEqual(emptyToolWindows);
    expect(parseStoredWindows('{"windows":5}', new Set(["type-info"]))).toEqual(emptyToolWindows);
  });
});
