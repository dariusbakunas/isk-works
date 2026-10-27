import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { FIT_ID, fitDetail, item, stubFetch } from "./fixtures";
import { renderPage } from "./render";

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

const puts = (fetchSpy: ReturnType<typeof stubFetch>) =>
  fetchSpy.mock.calls
    .filter(([input, init]) => String(input).includes(`/api/fittings/${FIT_ID}`) && (init as RequestInit | undefined)?.method === "PUT")
    .map(([, init]) => JSON.parse(String((init as RequestInit).body)));

async function gunRow() {
  const table = await screen.findByRole("table", { name: "Modules" });
  return within(table).getByRole("row", { name: /AutoCannon/ });
}

describe("Fittings › charges", () => {
  test("a gun's charge is picked from its row and saved straight away", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(within(await gunRow()).getByRole("button", { name: "Charge: EMP S" }));
    const menu = await screen.findByRole("menu", { name: "Charges for 200mm AutoCannon II" });
    // Grouped by market group above the size, the loaded one marked.
    expect(within(menu).getByRole("group", { name: "Advanced Autocannon Ammo" })).toHaveTextContent("Hail S");
    expect(within(menu).getByRole("group", { name: "Standard Ammo" })).toHaveTextContent("Phased Plasma S");
    expect(within(menu).getByRole("menuitemradio", { name: "EMP S" })).toBeChecked();

    fireEvent.click(within(menu).getByRole("menuitemradio", { name: "Phased Plasma S" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(within(await gunRow()).getByRole("button", { name: "Charge: Phased Plasma S" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Unsaved changes" })).toBeNull();

    await waitFor(() => expect(puts(fetchSpy)).toHaveLength(1));
    expect(puts(fetchSpy)[0].items[3].chargeTypeId).toBe(186);
  });

  test("faction charges fold under their Tech I charge, as pyfa's", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(within(await gunRow()).getByRole("button", { name: "Charge: EMP S" }));
    const menu = await screen.findByRole("menu", { name: "Charges for 200mm AutoCannon II" });
    expect(within(menu).queryByText("Faction Ammo")).toBeNull();
    expect(within(menu).queryByRole("menuitemradio", { name: "Republic Fleet EMP S" })).toBeNull();

    fireEvent.click(within(menu).getByRole("button", { name: "Variants of EMP S" }));
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: "Republic Fleet EMP S" }));

    await waitFor(() => expect(puts(fetchSpy)).toHaveLength(1));
    expect(puts(fetchSpy)[0].items[3].chargeTypeId).toBe(20979);

    // With a variant loaded, its family opens with it.
    fireEvent.click(within(await gunRow()).getByRole("button", { name: "Charge: Republic Fleet EMP S" }));
    expect(await screen.findByRole("menuitemradio", { name: "Republic Fleet EMP S" })).toBeChecked();
  });

  test("Empty unloads the charge", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(within(await gunRow()).getByRole("button", { name: "Charge: EMP S" }));
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "Empty" }));

    expect(within(await gunRow()).getByRole("button", { name: "Charge: none" })).toBeInTheDocument();
    await waitFor(() => expect(puts(fetchSpy)).toHaveLength(1));
    expect(puts(fetchSpy)[0].items[3].chargeTypeId).toBeNull();
  });

  test("a module that loads nothing has no charge picker", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const table = await screen.findByRole("table", { name: "Modules" });
    const dcu = within(table).getByRole("row", { name: /Damage Control II/ });
    expect(within(dcu).queryByRole("button", { name: /^Charge:/ })).toBeNull();
  });

  test("cargo shows no state and no charge picker, even for a module that loads charges", async () => {
    stubFetch({
      fit: {
        ...fitDetail(),
        items: [
          ...fitDetail().items,
          item({ typeId: 1952, slotKind: "cargo", slotIndex: null, quantity: 2, state: "offline", name: "Sensor Booster II", loadsCharges: true }),
        ],
      },
    });
    renderPage(`/fittings/${FIT_ID}`);
    const table = await screen.findByRole("table", { name: "Modules" });
    for (const name of [/Nanite Repair Paste/, /Sensor Booster II/]) {
      const row = within(table).getByRole("row", { name });
      expect(within(row).queryByRole("img", { name: /^(Offline|Online|Active|Overloaded)/ })).toBeNull();
      expect(within(row).queryByRole("button", { name: /^Charge:/ })).toBeNull();
    }
    // Drones still show theirs: in space or in the bay.
    const drones = within(table).getByRole("row", { name: /Hobgoblin II/ });
    expect(within(drones).getByRole("img", { name: /^Active/ })).toBeInTheDocument();
  });

  test("in the editor a picked charge is part of the draft", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    const bar = await screen.findByRole("region", { name: "Unsaved changes" });

    fireEvent.click(within(await gunRow()).getByRole("button", { name: "Charge: EMP S" }));
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "Phased Plasma S" }));

    expect(bar).toHaveTextContent("1 unsaved change");
    expect(puts(fetchSpy)).toHaveLength(0);
  });
});
