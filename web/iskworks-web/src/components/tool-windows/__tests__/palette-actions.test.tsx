import { act, fireEvent, render, screen } from "@testing-library/react";
import { Calculator } from "lucide-react";
import { beforeEach, expect, test } from "vitest";

import type { PaletteEntry } from "../../navigation/palette-search";
import { toolWindowPaletteActions } from "../palette-actions";
import { ToolWindowLayer } from "../tool-window-layer";
import { ToolWindowsProvider, useToolWindows, type ToolWindowKind } from "../tool-windows-context";

const note: ToolWindowKind<{ label: string }> = {
  kind: "note",
  icon: Calculator,
  title: (state) => state.label,
  defaultSize: { w: 400, h: 300 },
  minSize: { w: 360, h: 280 },
  render: ({ state }) => <p>{state.label}</p>,
};

let actions: PaletteEntry[] = [];

function Harness() {
  const api = useToolWindows();
  actions = toolWindowPaletteActions(api);
  return (
    <>
      <button onClick={() => api?.open("note", { label: "Harpy" }, { newWindow: true })} type="button">
        Open Harpy
      </button>
      <button onClick={() => api?.open("note", { label: "Rifter" }, { newWindow: true })} type="button">
        Open Rifter
      </button>
    </>
  );
}

beforeEach(() => {
  window.localStorage.clear();
  actions = [];
});

test("there are no window actions without windows or outside the provider", () => {
  expect(toolWindowPaletteActions(null)).toEqual([]);
  render(
    <ToolWindowsProvider kinds={[note]}>
      <Harness />
    </ToolWindowsProvider>,
  );
  expect(actions).toEqual([]);
});

test("each window is an action that brings it forward, restoring it if minimised", () => {
  render(
    <ToolWindowsProvider kinds={[note]}>
      <Harness />
      <ToolWindowLayer />
    </ToolWindowsProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Open Harpy" }));
  fireEvent.click(screen.getByRole("button", { name: "Open Rifter" }));
  fireEvent.click(screen.getByRole("button", { name: "Minimise Harpy" }));

  expect(actions.map((action) => [action.label, action.group, action.kind])).toEqual([
    ["Window: Harpy", "Actions", "action"],
    ["Window: Rifter", "Actions", "action"],
  ]);
  expect(actions[0].keywords).toContain("window");

  act(() => actions[0].run?.());
  expect(screen.getByRole("dialog", { name: "Harpy" })).toHaveAttribute("data-focused", "true");
  act(() => actions.find((action) => action.label === "Window: Rifter")?.run?.());
  expect(screen.getByRole("dialog", { name: "Rifter" })).toHaveAttribute("data-focused", "true");
});
