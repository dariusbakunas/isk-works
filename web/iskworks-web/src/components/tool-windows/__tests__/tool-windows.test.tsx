import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { Calculator } from "lucide-react";
import { afterEach, beforeAll, beforeEach, describe, expect, test, vi } from "vitest";

import { ToolWindowLayer } from "../tool-window-layer";
import { STORAGE_KEY, ToolWindowsProvider, useToolWindows, type ToolWindowKind } from "../tool-windows-context";
import { WindowDock } from "../window-dock";

// jsdom has no PointerEvent; a MouseEvent carries the coordinates.
beforeAll(() => {
  if (typeof globalThis.PointerEvent === "undefined") {
    class PointerEventPolyfill extends MouseEvent {
      pointerId: number;
      constructor(type: string, init: PointerEventInit = {}) {
        super(type, init);
        this.pointerId = init.pointerId ?? 1;
      }
    }
    globalThis.PointerEvent = PointerEventPolyfill as unknown as typeof PointerEvent;
  }
});

interface CounterState {
  label: string;
  count: number;
}

const counter: ToolWindowKind<CounterState> = {
  kind: "counter",
  icon: Calculator,
  title: (state) => state.label,
  defaultSize: { w: 480, h: 560 },
  minSize: { w: 360, h: 280 },
  merge: (current, incoming) => ({ ...current, label: incoming.label, count: current.count + 1 }),
  render: ({ state, setState }) => (
    <div>
      <p>Count {state.count}</p>
      <input aria-label="Notes" />
      <button onClick={() => setState({ ...state, count: state.count + 10 })} type="button">
        Add ten
      </button>
    </div>
  ),
};

function Opener() {
  const windows = useToolWindows();
  return (
    <>
      <button onClick={() => windows?.open("counter", { label: "Harpy", count: 0 })} type="button">
        Open Harpy
      </button>
      <button onClick={() => windows?.open("counter", { label: "Rifter", count: 0 })} type="button">
        Open Rifter
      </button>
      <button onClick={() => windows?.open("counter", { label: "Merlin", count: 0 }, { newWindow: true })} type="button">
        New Merlin
      </button>
    </>
  );
}

function renderWindows(expanded = true) {
  return render(
    <ToolWindowsProvider kinds={[counter]}>
      <Opener />
      <WindowDock expanded={expanded} />
      <ToolWindowLayer />
    </ToolWindowsProvider>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1440 });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 900 });
});
afterEach(() => vi.useRealTimers());

describe("tool windows", () => {
  test("a window is a labelled, non-modal dialog showing its kind's content", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));

    const dialog = screen.getByRole("dialog", { name: "Harpy" });
    expect(dialog).toHaveAttribute("aria-modal", "false");
    expect(within(dialog).getByText("Count 0")).toBeInTheDocument();
    expect(dialog).toHaveStyle({ left: `${1440 - 480 - 24}px`, top: "72px", width: "480px", height: "560px" });
  });

  test("a plain open reuses the front window of the kind; a new-window open doesn't", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "Open Rifter" }));

    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.getByRole("dialog", { name: "Rifter" })).toHaveTextContent("Count 1");

    fireEvent.click(screen.getByRole("button", { name: "New Merlin" }));
    expect(screen.getAllByRole("dialog")).toHaveLength(2);
  });

  test("the front window is marked focused", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "New Merlin" }));

    expect(screen.getByRole("dialog", { name: "Merlin" })).toHaveAttribute("data-focused", "true");
    expect(screen.getByRole("dialog", { name: "Harpy" })).toHaveAttribute("data-focused", "false");

    fireEvent.pointerDown(screen.getByRole("dialog", { name: "Harpy" }));
    expect(screen.getByRole("dialog", { name: "Harpy" })).toHaveAttribute("data-focused", "true");
    expect(screen.getByRole("dialog", { name: "Merlin" })).toHaveAttribute("data-focused", "false");
  });

  test("kinds update their own state", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "Add ten" }));
    expect(screen.getByRole("dialog", { name: "Harpy" })).toHaveTextContent("Count 10");
  });

  test("minimising moves the window to the dock; the dock restores and closes it", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const dock = screen.getByRole("navigation", { name: "Windows" });
    expect(within(dock).getByText("Windows")).toBeInTheDocument();
    fireEvent.click(within(dock).getByRole("button", { name: "Restore Harpy" }));
    expect(screen.getByRole("dialog", { name: "Harpy" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Windows" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "Close Harpy" }));
    expect(screen.queryByRole("navigation", { name: "Windows" })).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("with more than four minimised the dock shows three and a menu for the rest", () => {
    renderWindows(false);
    for (let index = 0; index < 5; index += 1) {
      fireEvent.click(screen.getByRole("button", { name: "New Merlin" }));
      fireEvent.click(screen.getAllByRole("button", { name: "Minimise Merlin" })[0]);
    }
    const dock = screen.getByRole("navigation", { name: "Windows" });
    expect(within(dock).getAllByRole("button", { name: "Restore Merlin" })).toHaveLength(3);
    fireEvent.click(within(dock).getByRole("button", { name: "2 more windows" }));
    expect(screen.getByRole("menu", { name: "More windows" })).toBeInTheDocument();
    expect(within(screen.getByRole("menu")).getAllByRole("menuitem")).toHaveLength(2);
  });

  test("Esc closes the window, but first only blurs an input inside it", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    const input = screen.getByRole("textbox", { name: "Notes" });
    input.focus();

    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Harpy" })).toBeInTheDocument();
    expect(input).not.toHaveFocus();

    fireEvent.keyDown(screen.getByRole("dialog", { name: "Harpy" }), { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("dragging the title bar moves the window", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    const dialog = screen.getByRole("dialog", { name: "Harpy" });
    const bar = within(dialog).getByTestId("tool-window-title-bar");

    fireEvent.pointerDown(bar, { clientX: 1000, clientY: 80, pointerId: 1, button: 0 });
    fireEvent.pointerMove(bar, { clientX: 900, clientY: 180, pointerId: 1 });
    fireEvent.pointerUp(bar, { clientX: 900, clientY: 180, pointerId: 1 });

    expect(dialog).toHaveStyle({ left: `${1440 - 480 - 24 - 100}px`, top: "172px" });
  });

  test("dragging the grip resizes, not below the kind's minimum", () => {
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    const dialog = screen.getByRole("dialog", { name: "Harpy" });
    const grip = within(dialog).getByRole("separator", { name: "Resize Harpy" });

    fireEvent.pointerDown(grip, { clientX: 500, clientY: 500, pointerId: 2, button: 0 });
    fireEvent.pointerMove(grip, { clientX: 100, clientY: 600, pointerId: 2 });
    fireEvent.pointerUp(grip, { pointerId: 2 });

    expect(dialog).toHaveStyle({ width: "360px", height: "660px" });
  });

  test("windows are remembered across reloads", () => {
    vi.useFakeTimers();
    const first = renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));
    act(() => vi.runAllTimers());
    first.unmount();

    renderWindows();
    expect(screen.getByRole("button", { name: "Restore Harpy" })).toBeInTheDocument();
  });

  test("unreadable storage still gives working windows", () => {
    const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.useFakeTimers();
    renderWindows();
    fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
    act(() => vi.runAllTimers());
    expect(screen.getByRole("dialog", { name: "Harpy" })).toBeInTheDocument();
    getItem.mockRestore();
    setItem.mockRestore();
  });

  test("a stored window of a kind that no longer exists is dropped", () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ windows: [{ id: "w1", kind: "gone", state: {}, x: 0, y: 0, w: 400, h: 300, minimized: true }], nextId: 2 }),
    );
    renderWindows();
    expect(screen.queryByRole("navigation", { name: "Windows" })).not.toBeInTheDocument();
  });

  test("without a provider the dock renders nothing", () => {
    const { container } = render(<WindowDock expanded />);
    expect(container).toBeEmptyDOMElement();
  });
});
