import { render, screen, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { HoldingsTable, type HoldingsTableRow } from "../holdings-table";

const rows: HoldingsTableRow[] = [
  { key: "a:1", characterName: "Mira", location: "Jita IV - Moon 4 - Caldari Navy Assembly Plant", quantity: 1_200 },
  {
    key: "b:2",
    characterName: "Aura",
    location: "Unknown location 1000000000001",
    locationTitle: "Location 1000000000001 · Hangar",
    quantity: 34,
    dimmed: true,
  },
];

describe("HoldingsTable", () => {
  test("lists each character and location with its quantity, then the total", () => {
    render(<HoldingsTable ariaLabel="Holdings" rows={rows} total={1_234} />);

    const table = screen.getByRole("table", { name: "Holdings" });
    expect(within(table).getAllByRole("columnheader").map((header) => header.textContent)).toEqual([
      "Character",
      "Location",
      "Quantity",
    ]);
    const mira = within(table).getByRole("row", { name: /Mira/ });
    expect(mira.querySelector('[data-column="location"]')).toHaveTextContent("Jita IV - Moon 4 - Caldari Navy Assembly Plant");
    expect(mira.querySelector('[data-column="quantity"]')).toHaveTextContent("1,200");
    expect(within(table).getByText("Mira")).toHaveAttribute("data-private");
    expect(within(table).getByText("Unknown location 1000000000001")).toHaveAttribute("title", "Location 1000000000001 · Hangar");
    expect(within(table).getByRole("row", { name: /Total/ })).toHaveTextContent("1,234");
  });

  test("a dimmed row fades its values but not its action", () => {
    render(
      <HoldingsTable
        action={{ label: "Reconcile", width: "8rem" }}
        ariaLabel="Holdings"
        rows={rows.map((row) => ({ ...row, action: <button type="button">Toggle {row.characterName}</button> }))}
        total={1_234}
      />,
    );

    expect(screen.getByRole("columnheader", { name: "Reconcile" })).toBeInTheDocument();
    const aura = screen.getByRole("row", { name: /Aura/ });
    expect(aura.querySelector('[data-column="quantity"] > *')).toHaveClass("opacity-60");
    expect(screen.getByRole("button", { name: "Toggle Aura" }).closest(".opacity-60")).toBeNull();
    const mira = screen.getByRole("row", { name: /Mira/ });
    expect(mira.querySelector('[data-column="quantity"] > *')).not.toHaveClass("opacity-60");
  });
});
