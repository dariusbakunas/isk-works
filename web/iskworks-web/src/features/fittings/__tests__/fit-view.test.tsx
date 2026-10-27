import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { AURA, FIT_ID, meta, pilots, statsView, stubFetch } from "./fixtures";
import { location, renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

const statsCalls = (fetchSpy: ReturnType<typeof stubFetch>) =>
  fetchSpy.mock.calls.map(([input]) => String(input)).filter((url) => url.includes("/stats"));

describe("Fittings › fit view", () => {
  test("the header names the fit and its ship, with All V chosen by default", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    expect(await screen.findByRole("heading", { name: "Null Transport" })).toBeInTheDocument();
    expect(screen.getByTestId("fit-header")).toHaveTextContent("Bustard");
    expect(screen.getByRole("link", { name: "Fits" })).toHaveAttribute("href", "/fittings");
    expect(screen.getByRole("button", { name: "All V" })).toHaveAttribute("aria-pressed", "true");
    await waitFor(() => expect(statsCalls(fetchSpy)[0]).toContain(`/api/fittings/${FIT_ID}/stats?pilot=allV&profile=uniform`));
  });

  test("pilot chips say who can fly it, and picking one recalculates for that pilot", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const chips = await screen.findByRole("group", { name: "Pilot" });
    expect(await within(chips).findByRole("button", { name: "Mira: can use all" })).toBeInTheDocument();
    expect(within(chips).getByRole("button", { name: "Aura: hull only" })).toBeInTheDocument();
    expect(within(chips).getByRole("button", { name: "Fresh Char: can't fly" })).toBeInTheDocument();
    expect(within(chips).getByRole("button", { name: "Scout: skills unknown" })).toBeInTheDocument();

    fireEvent.click(within(chips).getByRole("button", { name: "Aura: hull only" }));
    expect(location()).toBe(`/fittings/${FIT_ID}?pilot=${AURA}`);
    await waitFor(() => expect(statsCalls(fetchSpy).at(-1)).toContain(`pilot=${AURA}`));
  });

  test("the damage profile is chosen from the presets", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.change(await screen.findByLabelText("Damage profile"), { target: { value: "guristas" } });
    expect(location()).toBe(`/fittings/${FIT_ID}?profile=guristas`);
    await waitFor(() => expect(statsCalls(fetchSpy).at(-1)).toContain("profile=guristas"));
  });

  test("racks list their modules, then empty slots up to the ship's count", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    expect(await within(table).findByText("High slots")).toBeInTheDocument();
    expect(within(table).getByTestId("rack-usage-high")).toHaveTextContent("1 / 2");
    expect(within(table).getByTestId("rack-usage-med")).toHaveTextContent("2 / 3");
    expect(within(table).getAllByText("Empty high slot")).toHaveLength(1);
    expect(within(table).getAllByText("Empty rig slot")).toHaveLength(2);
    expect(within(table).getByText("Drones")).toBeInTheDocument();
    expect(within(table).getByText("Cargo")).toBeInTheDocument();

    const gun = within(table).getByRole("row", { name: /200mm AutoCannon II/ });
    expect(gun).toHaveTextContent("EMP S");
    expect(gun).toHaveTextContent("1.2 km + 8 km");
    expect(gun).toHaveTextContent("116.9 DPS");
    const dcu = within(table).getByRole("row", { name: /Damage Control II/ });
    expect(dcu).toHaveTextContent("—");
  });

  test("each module shows the state it reached, and why when it's lower than asked", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const mwd = await within(table).findByRole("row", { name: /Microwarpdrive/ });
    expect(within(mwd).getByRole("img", { name: "Active" })).toBeInTheDocument();
    const afterburner = within(table).getByRole("row", { name: /Afterburner/ });
    expect(within(afterburner).getByRole("img", { name: "Online (requested Active)" })).toBeInTheDocument();
  });

  test("a module the pilot can't use is locked; broken rules and unknown types are chipped", async () => {
    stubFetch({
      statsView: statsView({ flyability: pilots()[1].flyability }),
    });
    renderPage(`/fittings/${FIT_ID}?pilot=${AURA}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const gun = await within(table).findByRole("row", { name: /200mm AutoCannon II/ });
    expect(within(gun).getByLabelText("This pilot can't use it")).toBeInTheDocument();
    expect(within(gun).getByText("Over CPU")).toBeInTheDocument();
    const paste = within(table).getByRole("row", { name: /Nanite Repair Paste/ });
    expect(within(paste).getByText("Unknown to stats engine")).toBeInTheDocument();
  });

  test("Copy puts the fit's EFT on the clipboard", async () => {
    stubFetch();
    const writeText = vi.fn(async () => undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(await screen.findByRole("button", { name: "Copy for in-game import" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("[Bustard, Null Transport]\n"));
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  test("Duplicate saves a copy and opens it", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(await screen.findByRole("button", { name: "Duplicate" }));
    await waitFor(() => expect(location()).toBe("/fittings/copy-id"));
    const create = fetchSpy.mock.calls.find(
      ([input, init]) => String(input).endsWith("/api/fittings") && (init as RequestInit | undefined)?.method === "POST",
    );
    const body = JSON.parse((create![1] as RequestInit).body as string);
    expect(body.name).toBe("Null Transport (copy)");
    expect(body.items).toHaveLength(6);
    expect(body.items[0]).not.toHaveProperty("name");
  });

  test("Delete asks first, then removes the fit and goes back to the list", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    expect(fetchSpy.mock.calls.some(([, init]) => (init as RequestInit | undefined)?.method === "DELETE")).toBe(false);
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete fit" }));
    await waitFor(() => expect(location()).toBe("/fittings"));
    expect(fetchSpy.mock.calls.some(([, init]) => (init as RequestInit | undefined)?.method === "DELETE")).toBe(true);
  });
});

describe("Fittings › fit view states", () => {
  test("without engine data the layout stays but nothing calculated shows, and copy is off", async () => {
    const fetchSpy = stubFetch({ meta: meta(false) });
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    expect(await within(table).findByText("200mm AutoCannon II")).toBeInTheDocument();
    expect(within(table).queryByText("116.9 DPS")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy for in-game import" })).toBeDisabled();
    expect(screen.getAllByText(/no fitting engine data/i).length).toBeGreaterThan(0);
    expect(fetchSpy.mock.calls.some(([input]) => String(input).includes("/stats"))).toBe(false);
  });

  test("a summary strip carries the headline stats for narrow screens", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const strip = await screen.findByTestId("stats-summary");
    await waitFor(() => expect(strip).toHaveTextContent("EHP 90,790"));
    expect(strip).toHaveTextContent("DPS 215.9");
    expect(strip).toHaveTextContent("Cap 3:10");
    expect(strip).toHaveTextContent("655.3 m/s");
  });
});
