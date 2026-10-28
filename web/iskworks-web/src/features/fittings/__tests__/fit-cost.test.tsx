import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { assets, cost, FIT_ID, meta, stubFetch } from "./fixtures";
import { renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

// Price is the column before Held.
const priceCell = (row: HTMLElement) => [...row.querySelectorAll("td")].at(-2);

describe("Fittings › price and holdings", () => {
  test("the rail prices the fit at the workspace market, unpriced types left out", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const rail = await screen.findByRole("complementary", { name: "Fit stats" });
    // Once the stats are in, the rail stops changing shape.
    await within(rail).findByTestId("total-dps");
    const price = await within(rail).findByRole("heading", { name: "Price" });
    const section = price.closest("section")!;
    expect(await within(section).findByText("101.6M")).toHaveAttribute("title", "101,600,000 ISK");
    expect(within(section).getByText("95M")).toBeInTheDocument();
    expect(within(section).getByText("Drones")).toBeInTheDocument();
    expect(await within(section).findByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();
    expect(within(section).getByText(/1 item unpriced/)).toHaveAttribute("title", "Nanite Repair Paste");
  });

  test("each row shows its price: unit times quantity, a dash when unpriced", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const drones = (await within(table).findByText("5 × Hobgoblin II")).closest("tr")!;
    await waitFor(() => expect(priceCell(drones)).toHaveTextContent("1.6M"));
    const paste = within(table).getByText("100 × Nanite Repair Paste").closest("tr")!;
    expect(priceCell(paste)).toHaveTextContent("—");
  });

  test("a section with nothing priced shows a dash, not zero", async () => {
    const unpricedHull = cost();
    unpricedHull.lines[0] = { ...unpricedHull.lines[0], unitPrice: null, total: null };
    unpricedHull.hull = "0";
    stubFetch({ cost: unpricedHull });
    renderPage(`/fittings/${FIT_ID}`);

    const rail = await screen.findByRole("complementary", { name: "Fit stats" });
    await within(rail).findByTestId("total-dps");
    const price = within(rail).getByRole("heading", { name: "Price" }).closest("section")!;
    const hull = (await within(price).findByText("Hull")).parentElement!;
    expect(hull).toHaveTextContent("Hull—");
  });

  test("prices show without the stats engine", async () => {
    stubFetch({ meta: meta(false) });
    renderPage(`/fittings/${FIT_ID}`);

    const rail = await screen.findByRole("complementary", { name: "Fit stats" });
    expect(await within(rail).findByText("101.6M")).toBeInTheDocument();
    expect(within(rail).getByText("Stats unavailable")).toBeInTheDocument();
  });

  test("each row shows how many you hold, and the magnifier where", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const gun = await within(table).findByRole("row", { name: /200mm AutoCannon II/ });
    const heldCell = (row: HTMLElement) => [...row.querySelectorAll("td")].at(-1)!;
    await waitFor(() => expect(heldCell(gun)).toHaveTextContent("6"));
    // Nothing held: a dash and no magnifier, so there's no need to look.
    const dcu = within(table).getByRole("row", { name: /Damage Control II/ });
    expect(heldCell(dcu)).toHaveTextContent("—");
    expect(within(dcu).queryByRole("button", { name: /Where you have/ })).not.toBeInTheDocument();

    fireEvent.click(within(gun).getByRole("button", { name: "Where you have 200mm AutoCannon II" }));
    const dialog = await screen.findByRole("dialog", { name: "200mm AutoCannon II" });
    const rows = within(dialog).getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Mira");
    expect(rows[1]).toHaveTextContent("Jita IV - Moon 4 - Caldari Navy Assembly Plant");
    expect(rows[1]).toHaveTextContent("3");
    expect(rows[3]).toHaveTextContent("Aura");
    expect(within(dialog).getByRole("row", { name: /Total/ })).toHaveTextContent("6");

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("a large count shows compact, exact on hover", async () => {
    const many = assets();
    many.holdings[0] = { ...many.holdings[0], quantity: 12_338 };
    stubFetch({ assets: many });
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const gun = await within(table).findByRole("row", { name: /200mm AutoCannon II/ });
    const count = await within(gun).findByText("12K");
    expect(count).toHaveAttribute("title", "12,341");
  });

  test("the hull says whether you hold one", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const header = await screen.findByTestId("fit-header");
    expect(await within(header).findByText("· none held")).toBeInTheDocument();
    expect(within(header).queryByRole("button", { name: /Where you have/ })).not.toBeInTheDocument();
  });

  test("a held hull opens where, without starting to edit", async () => {
    const held = assets();
    held.holdings.push({ ...held.holdings[0], typeId: 12731, quantity: 2 });
    stubFetch({ assets: held });
    renderPage(`/fittings/${FIT_ID}`);

    const header = await screen.findByTestId("fit-header");
    expect(await within(header).findByText("· 2 held")).toBeInTheDocument();
    fireEvent.click(within(header).getByRole("button", { name: "Where you have Bustard" }));
    const dialog = await screen.findByRole("dialog", { name: "Bustard" });
    expect(within(dialog).getByRole("row", { name: /Total/ })).toHaveTextContent("2");
    expect(screen.queryByRole("complementary", { name: "Module browser" })).toBeNull();
  });

  test("there's no assets section any more", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    await screen.findByRole("table", { name: "Modules" });
    expect(screen.queryByRole("heading", { name: "Assets" })).not.toBeInTheDocument();
  });
});
