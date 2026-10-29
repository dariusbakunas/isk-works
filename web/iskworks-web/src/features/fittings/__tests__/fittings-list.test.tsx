import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { AURA, FIT_ID, meta, stubFetch } from "./fixtures";
import { renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

describe("Fittings › list", () => {
  test("the tree and the table list the same fits, with agreeing counts", async () => {
    stubFetch();
    renderPage();

    const table = await screen.findByRole("table", { name: "Fits" });
    expect(within(table).getAllByRole("row").filter((row) => row.dataset.rowKey)).toHaveLength(3);
    expect(screen.getByText("3 saved fits · 2 hulls")).toBeInTheDocument();

    const tree = screen.getByRole("navigation", { name: "Hulls and fits" });
    expect(within(tree).getByText("Deep Space Transport")).toBeInTheDocument();
    expect(within(tree).getByText("Battleship")).toBeInTheDocument();
    fireEvent.click(within(tree).getByRole("button", { name: "Deep Space Transport" }));
    expect(within(tree).getByRole("link", { name: "Null Transport" })).toHaveAttribute("href", `/fittings/${FIT_ID}`);
    expect(within(tree).getByTestId("ship-count-12731")).toHaveTextContent("2");
  });

  test("each fit says who can use it all, who flies the hull only, or no one", async () => {
    stubFetch();
    renderPage();

    const row = await screen.findByRole("row", { name: /Null Transport/ });
    expect(within(row).getByLabelText("Can use all: Mira")).toBeInTheDocument();
    expect(within(row).getByText("+1 hull only")).toBeInTheDocument();
    expect(within(screen.getByRole("row", { name: /L4 Cruise/ })).getByText("No one")).toBeInTheDocument();
  });

  test("filtering by a character keeps the fits it can fly", async () => {
    stubFetch();
    renderPage();

    await screen.findByRole("table", { name: "Fits" });
    fireEvent.change(screen.getByLabelText("Flyable by"), { target: { value: AURA } });
    const rows = screen.getAllByRole("row").filter((row) => row.dataset.rowKey);
    expect(rows.map((row) => row.dataset.rowKey)).toHaveLength(2);
    expect(screen.queryByRole("row", { name: /L4 Cruise/ })).not.toBeInTheDocument();
  });

  test("hull groups start collapsed and open on click", async () => {
    stubFetch();
    renderPage();

    await screen.findByRole("table", { name: "Fits" });
    const tree = screen.getByRole("navigation", { name: "Hulls and fits" });
    const group = within(tree).getByRole("button", { name: "Deep Space Transport" });
    expect(group).toHaveAttribute("aria-expanded", "false");
    expect(within(tree).queryAllByRole("link")).toHaveLength(0);

    fireEvent.click(group);
    expect(group).toHaveAttribute("aria-expanded", "true");
    expect(within(tree).getByRole("link", { name: "Null Transport" })).toBeInTheDocument();
  });

  test("searching filters the tree and the table by fit or ship name", async () => {
    stubFetch();
    renderPage();

    await screen.findByRole("table", { name: "Fits" });
    fireEvent.change(screen.getByRole("searchbox", { name: "Search hulls and fits" }), { target: { value: "mega" } });
    expect(screen.getAllByRole("row").filter((row) => row.dataset.rowKey)).toHaveLength(1);
    const tree = screen.getByRole("navigation", { name: "Hulls and fits" });
    expect(within(tree).queryByText("Bustard")).not.toBeInTheDocument();
    expect(within(tree).getAllByRole("link")).toHaveLength(1);
  });

  test("the tree switches to tags, remembers it, and a fit shows under each of its tags", async () => {
    window.localStorage.clear();
    stubFetch();
    const { unmount } = renderPage();

    await screen.findByRole("table", { name: "Fits" });
    expect(screen.getByRole("button", { name: "Hulls" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Tags" }));
    expect(screen.getByRole("button", { name: "Tags" })).toHaveAttribute("aria-pressed", "true");
    const tree = screen.getByRole("navigation", { name: "Hulls and fits" });

    expect(within(tree).getAllByRole("button").map((group) => group.textContent)).toEqual(["Hauling", "Lowsec", "Untagged"]);
    fireEvent.click(within(tree).getByRole("button", { name: "Hauling" }));
    fireEvent.click(within(tree).getByRole("button", { name: "Lowsec" }));
    expect(within(tree).getAllByRole("link", { name: "Lowsec Brick" })).toHaveLength(2);

    unmount();
    renderPage();
    await screen.findByRole("table", { name: "Fits" });
    expect(screen.getByRole("button", { name: "Tags" })).toHaveAttribute("aria-pressed", "true");
    window.localStorage.clear();
  });

  test("searching matches a fit's tags too", async () => {
    stubFetch();
    renderPage();

    await screen.findByRole("table", { name: "Fits" });
    fireEvent.change(screen.getByRole("searchbox", { name: "Search hulls and fits" }), { target: { value: "hauling" } });
    expect(screen.getAllByRole("row").filter((row) => row.dataset.rowKey).map((row) => row.dataset.rowKey)).toEqual([
      FIT_ID,
      "22222222-2222-2222-2222-222222222222",
    ]);
  });

  test("with no fits, the page asks for a paste and counts nothing", async () => {
    stubFetch({ fits: [] });
    renderPage();

    expect(await screen.findByText("Paste a fit from the game or pyfa")).toBeInTheDocument();
    expect(screen.getByText("0 saved fits · 0 hulls")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Hulls and fits" })).queryAllByRole("link")).toHaveLength(0);
  });

  test("the footer names both data builds", async () => {
    stubFetch();
    renderPage();

    expect(await screen.findByText("SDE build 3482594 · Stats engine data 13.3650286.0")).toBeInTheDocument();
  });

  test("without engine data, import is off and the page says why", async () => {
    stubFetch({ meta: meta(false) });
    renderPage();

    expect(await screen.findByText(/no fitting engine data/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import from clipboard" })).toBeDisabled();
    expect(screen.getByText(/Stats engine data unavailable/)).toBeInTheDocument();
  });
});
