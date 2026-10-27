import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { assets, cost, FIT_ID, meta, stubFetch } from "./fixtures";
import { renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

// Price is the table's last column.
const priceCell = (row: HTMLElement) => [...row.querySelectorAll("td")].at(-1);

describe("Fittings › price and assets", () => {
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

  test("assets list where each character holds the fit, expandable to what's short", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const panel = (await screen.findByRole("heading", { name: "Assets" })).closest("section")!;
    expect(await within(panel).findByText("10 / 110 items")).toBeInTheDocument();
    expect(within(panel).getByText("1 / 110 items")).toBeInTheDocument();
    fireEvent.click(
      within(panel).getByRole("button", { name: /What's short at Amarr VIII .* for Aura/ }),
    );
    const short = within(panel).getByRole("list", { name: /Short at Amarr VIII/ });
    expect(within(short).getByText("Bustard")).toBeInTheDocument();
    expect(within(short).getByText("0 / 100")).toBeInTheDocument();
  });

  test("no holdings say so", async () => {
    stubFetch({ assets: { ...assets(), locations: [] } });
    renderPage(`/fittings/${FIT_ID}`);

    expect(await screen.findByText("None of your characters hold any of this fit.")).toBeInTheDocument();
  });

  test("a fully covered location has nothing to expand", async () => {
    const full = assets();
    full.locations = [{ ...full.locations[0], covered: 110, short: [] }];
    stubFetch({ assets: full, cost: cost() });
    renderPage(`/fittings/${FIT_ID}`);

    const panel = (await screen.findByRole("heading", { name: "Assets" })).closest("section")!;
    expect(await within(panel).findByText("110 / 110 items")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: /What's short/ })).not.toBeInTheDocument();
  });
});
