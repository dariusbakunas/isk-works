import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AURA, FIT_ID, stubFetch } from "./fixtures";
import { renderPage } from "./render";

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

async function openEditor(path = `/fittings/${FIT_ID}`) {
  const fetchSpy = stubFetch();
  renderPage(path);
  fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
  const browser = await screen.findByRole("complementary", { name: "Module browser" });
  return { fetchSpy, browser };
}

const table = () => screen.getByRole("table", { name: "Modules" });
const rowsNamed = (name: RegExp) => within(table()).queryAllByRole("row", { name });
const bar = () => screen.getByRole("region", { name: "Unsaved changes" });

describe("Fittings › module browser", () => {
  test("searching lists modules the ship fits, grouped by market group", async () => {
    const { fetchSpy, browser } = await openEditor();

    fireEvent.change(within(browser).getByRole("searchbox", { name: "Search modules" }), { target: { value: "dam" } });
    expect(await within(browser).findByRole("button", { name: "Add Damage Control II" })).toBeInTheDocument();
    expect(within(browser).getByText("Damage Controls")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        fetchSpy.mock.calls.some(([input]) =>
          String(input).includes("/api/fittings/catalogue?kind=modules&shipTypeId=12731&q=dam&pilot=allV"),
        ),
      ).toBe(true),
    );
  });

  test("a module goes into the first free slot of its rack, until the rack is full", async () => {
    const { browser } = await openEditor();

    fireEvent.doubleClick(await within(browser).findByText("Damage Control II"));
    expect(rowsNamed(/Damage Control II/)).toHaveLength(2);
    expect(bar()).toHaveTextContent("1 unsaved change");

    fireEvent.click(within(browser).getByRole("button", { name: "Add Damage Control II" }));
    expect(rowsNamed(/Damage Control II/)).toHaveLength(2);
    expect(within(browser).getByRole("status")).toHaveTextContent("No free low slot");
  });

  test("a module dropped on a slot goes there", async () => {
    const { browser } = await openEditor();

    const gun = await within(browser).findByText("200mm AutoCannon II");
    const data = new Map<string, string>();
    const dataTransfer = {
      setData: (type: string, value: string) => data.set(type, value),
      getData: (type: string) => data.get(type) ?? "",
      effectAllowed: "copy",
      dropEffect: "copy",
      types: ["application/x-iskworks-type"],
    };
    fireEvent.dragStart(gun, { dataTransfer });
    const empty = within(table()).getByText("Empty high slot").closest("tr")!;
    fireEvent.dragOver(empty, { dataTransfer });
    fireEvent.drop(empty, { dataTransfer });
    expect(rowsNamed(/200mm AutoCannon II/)).toHaveLength(2);
    expect(within(table()).queryByText("Empty high slot")).not.toBeInTheDocument();
  });

  test("with a character pilot, the browser can keep to what it can use", async () => {
    const { browser } = await openEditor(`/fittings/${FIT_ID}?pilot=${AURA}`);

    expect(await within(browser).findByText("Warp Core Stabilizer II")).toBeInTheDocument();
    fireEvent.click(within(browser).getByRole("checkbox", { name: "Pilot can use" }));
    expect(within(browser).queryByText("Warp Core Stabilizer II")).not.toBeInTheDocument();
    expect(within(browser).getByText("Damage Control II")).toBeInTheDocument();
  });

  test("charges are for the selected module and load into it", async () => {
    const { fetchSpy, browser } = await openEditor();

    fireEvent.click(within(browser).getByRole("tab", { name: "Charges" }));
    expect(within(browser).getByText("Pick a module in the table to see its charges.")).toBeInTheDocument();

    fireEvent.click(within(table()).getByRole("row", { name: /200mm AutoCannon II/ }));
    fireEvent.click(await within(browser).findByRole("button", { name: "Load Phased Plasma S" }));
    expect(within(table()).getByRole("row", { name: /200mm AutoCannon II/ })).toHaveTextContent("Phased Plasma S");
    expect(
      fetchSpy.mock.calls.some(([input]) => String(input).includes("kind=charges&moduleTypeId=2873")),
    ).toBe(true);
  });

  test("drones join their stack and cargo takes any item", async () => {
    const { browser } = await openEditor();

    fireEvent.click(within(browser).getByRole("tab", { name: "Drones" }));
    fireEvent.click(await within(browser).findByRole("button", { name: "Add Hobgoblin II" }));
    expect(within(table()).getByRole("spinbutton", { name: "Quantity of Hobgoblin II" })).toHaveValue(6);

    fireEvent.click(within(browser).getByRole("tab", { name: "Cargo" }));
    fireEvent.change(within(browser).getByRole("searchbox", { name: "Search items" }), { target: { value: "trit" } });
    fireEvent.click(await within(browser).findByRole("button", { name: "Add Tritanium" }));
    expect(within(table()).getByRole("spinbutton", { name: "Quantity of Tritanium" })).toHaveValue(1);
  });
});
