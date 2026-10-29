import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { AURA, FIT_ID, meta, pilots, statsView, stubFetch } from "./fixtures";
import { location, renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

const statsCalls = (fetchSpy: ReturnType<typeof stubFetch>) =>
  fetchSpy.mock.calls.map(([input]) => String(input)).filter((url) => url.includes("/stats"));

const tagWrites = (fetchSpy: ReturnType<typeof stubFetch>) =>
  fetchSpy.mock.calls
    .filter(([input, init]) => String(input).endsWith("/tags") && (init as RequestInit | undefined)?.method === "PUT")
    .map(([input, init]) => ({
      url: new URL(String(input), "http://localhost").pathname,
      tags: JSON.parse((init as RequestInit).body as string).tags,
    }));

describe("Fittings › fit view", () => {
  test("the ship, its modules and their loaded charges open in Type Info", async () => {
    stubFetch();
    window.localStorage.clear();
    renderPage(`/fittings/${FIT_ID}`, { toolWindows: true });

    await screen.findByRole("table", { name: "Modules" });
    for (const name of ["Bustard", "200mm AutoCannon II", "EMP S", "Hobgoblin II"]) {
      expect(screen.getAllByRole("button", { name: `Show info: ${name}` }).length).toBeGreaterThan(0);
    }
    fireEvent.click(screen.getAllByRole("button", { name: "Show info: EMP S" })[0]);
    expect(screen.getByRole("dialog", { name: "EMP S" })).toHaveTextContent("Type 185");
    // Opening info doesn't select the row (no inspector change, no navigation).
    expect(location()).toBe(`/fittings/${FIT_ID}`);
  });

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

  test("capacitor per second reads as pyfa's: a drain negative, a nosferatu's gain positive", async () => {
    const view = statsView();
    view.items[1] = { ...view.items[1]!, capacitorPerSecond: 3.75 };
    view.items[3] = { ...view.items[3]!, capacitorPerSecond: -3.6 };
    stubFetch({ statsView: view });
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    expect(await within(table).findByRole("row", { name: /Microwarpdrive/ })).toHaveTextContent("−3.75");
    expect(within(table).getByRole("row", { name: /200mm AutoCannon II/ })).toHaveTextContent("+3.6");
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

  test("a passive module that's on reads apart from an active module left online", async () => {
    stubFetch();
    const { unmount } = renderPage(`/fittings/${FIT_ID}`);

    const passive = await screen.findByRole("row", { name: /Damage Control II/ });
    expect(within(passive).getByRole("img", { name: "Online (passive)" })).toBeInTheDocument();
    unmount();

    const view = statsView();
    view.items[0] = { ...view.items[0]!, maxState: "active" };
    stubFetch({ statsView: view });
    renderPage(`/fittings/${FIT_ID}`);

    // Stats are in once the afterburner shows the state it was held to.
    await screen.findByRole("img", { name: "Online (requested Active)" });
    const idle = screen.getByRole("row", { name: /Damage Control II/ });
    expect(within(idle).getByRole("img", { name: "Online" })).toBeInTheDocument();
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
    // The copy keeps the fit's tags.
    await waitFor(() => expect(tagWrites(fetchSpy)).toEqual([{ url: "/api/fittings/copy-id/tags", tags: ["Hauling"] }]));
  });

  test("tags are added from the header with suggestions, and removed, saving each change at once", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const tags = await screen.findByRole("list", { name: "Tags" });
    expect(within(tags).getByText("Hauling")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Add tag" }));
    const input = screen.getByRole("combobox", { name: "New tag" });
    // The workspace's other tags are offered.
    await waitFor(() =>
      expect([...document.querySelectorAll("datalist option")].map((option) => option.getAttribute("value"))).toEqual(["Lowsec"]),
    );
    fireEvent.change(input, { target: { value: " Cyno " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(await within(tags).findByText("Cyno")).toBeInTheDocument();
    expect(tagWrites(fetchSpy)).toEqual([{ url: `/api/fittings/${FIT_ID}/tags`, tags: ["Hauling", "Cyno"] }]);

    fireEvent.click(within(tags).getByRole("button", { name: "Remove tag Hauling" }));
    await waitFor(() => expect(within(tags).queryByText("Hauling")).not.toBeInTheDocument());
    expect(tagWrites(fetchSpy).at(-1)).toEqual({ url: `/api/fittings/${FIT_ID}/tags`, tags: ["Cyno"] });
  });

  test("Escape closes the new tag field without saving", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    fireEvent.click(await screen.findByRole("button", { name: "Add tag" }));
    const input = screen.getByRole("combobox", { name: "New tag" });
    fireEvent.change(input, { target: { value: "Cyno" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("combobox", { name: "New tag" })).not.toBeInTheDocument();
    expect(tagWrites(fetchSpy)).toEqual([]);
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

describe("Fittings › empty racks", () => {
  test("a rack with only empty slots lists them, not 'No rows'", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    // The fixture has no rigs fitted and two rig slots.
    expect(await within(table).findAllByText("Empty rig slot")).toHaveLength(2);
    expect(within(table).queryByText("No rows")).not.toBeInTheDocument();
  });
});
