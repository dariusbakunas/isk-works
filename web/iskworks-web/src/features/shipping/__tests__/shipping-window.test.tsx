import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

import { toolWindowPaletteActions } from "../../../components/tool-windows/palette-actions";
import { SHIPPING_KIND, shippingStateWith } from "../../../components/shipping/shipping-state";
import { ToolWindowLayer } from "../../../components/tool-windows/tool-window-layer";
import { ToolWindowsProvider, useToolWindows } from "../../../components/tool-windows/tool-windows-context";
import { ToolLaunchers } from "../../../components/tool-windows/window-dock";
import { shippingKind } from "../shipping-kind";

const api = vi.hoisted(() => ({ resolveTypes: vi.fn(), searchTypes: vi.fn() }));

vi.mock("../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../api/sde")>("../../../api/sde");
  return { ...actual, ...api };
});

const TYPES = [
  { typeId: 34, typeName: "Tritanium", packagedVolumeM3: "0.01" },
  { typeId: 1, typeName: "Crate", packagedVolumeM3: "4" },
];

let palette: ReturnType<typeof toolWindowPaletteActions> = [];

function Harness() {
  const windows = useToolWindows();
  palette = toolWindowPaletteActions(windows);
  return (
    <button
      onClick={() => windows?.open(SHIPPING_KIND, shippingStateWith([{ name: "Tritanium", quantity: 2500 }, { name: "Crate", quantity: 2 }]))}
      type="button"
    >
      Send list
    </button>
  );
}

function renderApp() {
  return render(
    <ToolWindowsProvider kinds={[shippingKind]}>
      <Harness />
      <ToolLaunchers expanded />
      <ToolWindowLayer />
    </ToolWindowsProvider>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  api.resolveTypes.mockReset();
  api.searchTypes.mockReset();
  api.resolveTypes.mockImplementation(async (names: string[]) =>
    TYPES.filter((type) => names.some((name) => name.toLowerCase() === type.typeName.toLowerCase())),
  );
  api.searchTypes.mockResolvedValue([]);
  palette = [];
});

test("opens empty from the rail and from the palette", async () => {
  renderApp();
  expect(palette.map((entry) => entry.label)).toContain("Open: Shipping calculator");
  fireEvent.click(screen.getByRole("button", { name: "Shipping calculator" }));
  expect(await screen.findByLabelText("Cargo hold in m³")).toBeInTheDocument();
  expect(screen.getByText(/Add items one at a time/)).toBeInTheDocument();
  expect(api.resolveTypes).not.toHaveBeenCalled();
});

test("a sent list is looked up and split into shipments for the hold", async () => {
  renderApp();
  fireEvent.click(screen.getByRole("button", { name: "Send list" }));

  expect(await screen.findByLabelText("Quantity: Tritanium")).toHaveValue("2,500");
  expect(api.resolveTypes).toHaveBeenCalledWith(["Tritanium", "Crate"]);
  expect(screen.getByText("33 m³")).toBeInTheDocument();
  expect(screen.queryByLabelText("Shipment 1")).not.toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Cargo hold in m³"), { target: { value: "10" } });
  expect(screen.getByText("4 shipments")).toBeInTheDocument();
  const first = within(screen.getByLabelText("Shipment 1"));
  expect(first.getByText("10 / 10 m³")).toBeInTheDocument();
  expect(first.getByText("Crate")).toBeInTheDocument();

  // A shipment's quantity copies as the bare number, to paste into EVE.
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  fireEvent.click(first.getByRole("button", { name: "Copy 200 Tritanium" }));
  expect(writeText).toHaveBeenCalledWith("200");

  // Halving the Tritanium drops a trip.
  fireEvent.change(screen.getByLabelText("Quantity: Tritanium"), { target: { value: "1,200" } });
  expect(screen.getByText("2 shipments")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Remove Crate" }));
  expect(screen.queryByLabelText("Quantity: Crate")).not.toBeInTheDocument();
});

test("a pasted list adds to the items and reports names nothing matches", async () => {
  renderApp();
  fireEvent.click(screen.getByRole("button", { name: "Shipping calculator" }));
  fireEvent.click(await screen.findByRole("button", { name: /Paste list/ }));
  fireEvent.change(screen.getByLabelText("Item list"), { target: { value: "Tritanium 100\ntritanium x50\nNot An Item 3" } });
  fireEvent.click(screen.getByRole("button", { name: "Add 3 lines" }));

  expect(await screen.findByLabelText("Quantity: Tritanium")).toHaveValue("150");
  expect(screen.getByText("Not An Item 3")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss unmatched names" }));
  expect(screen.queryByText("Not An Item 3")).not.toBeInTheDocument();
});

test("a failed lookup keeps the lines and can be retried", async () => {
  api.resolveTypes.mockRejectedValueOnce(new Error("down"));
  renderApp();
  fireEvent.click(screen.getByRole("button", { name: "Send list" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't look up 2 items.");
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.getByLabelText("Quantity: Crate")).toHaveValue("2"));
});
