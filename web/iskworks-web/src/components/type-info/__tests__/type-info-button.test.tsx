import { fireEvent, render, screen } from "@testing-library/react";
import { Info } from "lucide-react";
import { beforeEach, expect, test, vi } from "vitest";

import { ToolWindowLayer } from "../../tool-windows/tool-window-layer";
import { ToolWindowsProvider, type ToolWindowKind } from "../../tool-windows/tool-windows-context";
import { TypeInfoButton } from "../type-info-button";
import { TYPE_INFO_KIND, currentEntry, mergeTypeInfo, type TypeInfoWindowState } from "../type-info-state";

const stubKind: ToolWindowKind<TypeInfoWindowState> = {
  kind: TYPE_INFO_KIND,
  icon: Info,
  title: (state) => currentEntry(state).name ?? `Type ${currentEntry(state).typeId}`,
  defaultSize: { w: 480, h: 560 },
  minSize: { w: 360, h: 280 },
  merge: mergeTypeInfo,
  render: ({ state }) => <p>History {state.history.map((entry) => entry.typeId).join(",")}</p>,
};

beforeEach(() => window.localStorage.clear());

function renderButtons(onRow = vi.fn()) {
  render(
    <ToolWindowsProvider kinds={[stubKind]}>
      <div className="group" onClick={onRow} onKeyDown={onRow} role="row" tabIndex={0}>
        <TypeInfoButton name="Harpy" typeId={11381} />
        <TypeInfoButton alwaysVisible name="Tritanium" typeId={34} />
      </div>
      <ToolWindowLayer />
    </ToolWindowsProvider>,
  );
  return onRow;
}

test("it opens the type in a Type Info window without activating the row", () => {
  const onRow = renderButtons();
  fireEvent.click(screen.getByRole("button", { name: "Show info: Harpy" }));
  expect(screen.getByRole("dialog", { name: "Harpy" })).toHaveTextContent("History 11381");
  fireEvent.keyDown(screen.getByRole("button", { name: "Show info: Harpy" }), { key: "Enter" });
  expect(onRow).not.toHaveBeenCalled();
});

test("a plain click follows in the front window; shift or cmd opens another", () => {
  renderButtons();
  fireEvent.click(screen.getByRole("button", { name: "Show info: Harpy" }));
  fireEvent.click(screen.getByRole("button", { name: "Show info: Tritanium" }));
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(screen.getByRole("dialog", { name: "Tritanium" })).toHaveTextContent("History 11381,34");

  fireEvent.click(screen.getByRole("button", { name: "Show info: Harpy" }), { shiftKey: true });
  fireEvent.click(screen.getByRole("button", { name: "Show info: Tritanium" }), { metaKey: true });
  expect(screen.getAllByRole("dialog")).toHaveLength(3);
});

test("it waits for row hover unless always visible", () => {
  renderButtons();
  expect(screen.getByRole("button", { name: "Show info: Harpy" })).toHaveClass("opacity-0");
  expect(screen.getByRole("button", { name: "Show info: Tritanium" })).not.toHaveClass("opacity-0");
});

test("outside the tool windows provider there's no button", () => {
  render(<TypeInfoButton name="Harpy" typeId={11381} />);
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});
