import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { FIT_ID, stubFetch } from "./fixtures";
import { renderPage } from "./render";

const STORAGE_KEY = `iw:fit-draft:${FIT_ID}`;

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

async function startEditing() {
  fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
  return screen.findByRole("region", { name: "Unsaved changes" });
}

const calls = (fetchSpy: ReturnType<typeof stubFetch>, part: string, method = "POST") =>
  fetchSpy.mock.calls.filter(
    ([input, init]) => String(input).includes(part) && ((init as RequestInit | undefined)?.method ?? "GET") === method,
  );

describe("Fittings › editor", () => {
  test("editing starts with nothing to save", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const bar = await startEditing();
    expect(bar).toHaveTextContent("No unsaved changes");
    expect(within(bar).getByRole("button", { name: "Save fit" })).toBeDisabled();
  });

  test("a change recalculates the draft and the stats show the difference", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}?pilot=allZero&profile=em`);
    const bar = await startEditing();

    const table = screen.getByRole("table", { name: "Modules" });
    const dcu = within(table).getByRole("row", { name: /Damage Control II/ });
    fireEvent.click(within(dcu).getByRole("button", { name: /^State:/ }));
    expect(bar).toHaveTextContent("1 unsaved change");

    await waitFor(() => expect(calls(fetchSpy, "/api/fittings/calculate")).toHaveLength(1));
    const body = JSON.parse(String((calls(fetchSpy, "/api/fittings/calculate")[0][1] as RequestInit).body));
    expect(body.pilot).toBe("allZero");
    expect(body.profile).toBe("em");
    expect(body.fit.shipTypeId).toBe(12731);
    // The fixture's Damage Control reaches online at most, so a click from
    // online wraps it to offline.
    expect(body.fit.items[0]).toEqual({ typeId: 2048, slotKind: "low", slotIndex: 0, quantity: 1, state: "offline", chargeTypeId: null });

    expect(await screen.findByTestId("total-ehp")).toHaveTextContent("91,790");
    expect(screen.getByTestId("delta-ehp")).toHaveTextContent("+1,000");
  });

  test("outside editing, a module's state toggles and saves straight away, in order", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const row = (name: RegExp) => within(table).getByRole("row", { name });
    const state = (name: RegExp) => within(row(name)).getByRole("button", { name: /^State:/ });
    const saved = async (count: number) => {
      await waitFor(() => expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT").length).toBeGreaterThanOrEqual(count));
      const put = calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")[count - 1];
      return JSON.parse(String((put[1] as RequestInit).body)).items.map((item: { state: string }) => item.state);
    };

    // Click turns the microwarpdrive off; shift-click overheats the gun;
    // alt-click then takes the gun offline, each before the last save is back.
    fireEvent.click(await waitFor(() => state(/Microwarpdrive/)));
    expect(state(/Microwarpdrive/)).toHaveAccessibleName(/^State: Online/);
    fireEvent.click(state(/AutoCannon/), { shiftKey: true });
    fireEvent.click(state(/AutoCannon/), { altKey: true });

    // No editor: no draft bar, no module browser.
    expect(screen.queryByRole("region", { name: "Unsaved changes" })).toBeNull();
    expect(screen.queryByRole("complementary", { name: "Module browser" })).toBeNull();

    // Each save carries the changes before it.
    expect((await saved(1))[1]).toBe("online");
    expect((await saved(2)).slice(1, 4)).toEqual(["online", "active", "overload"]);
    expect((await saved(3)).slice(1, 4)).toEqual(["online", "active", "offline"]);
    expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")).toHaveLength(3);
    expect(calls(fetchSpy, "/api/fittings/calculate")).toHaveLength(0);
  });

  test("outside editing, a drone stack goes back to the bay and launches again", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const drones = () => within(within(table).getByRole("row", { name: /Hobgoblin II/ })).getByRole("button", { name: /^Drones:/ });
    const savedDrones = (count: number) => {
      const put = calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")[count - 1];
      const items = JSON.parse(String((put[1] as RequestInit).body)).items as Array<{ typeId: number; state: string }>;
      return items.find((item) => item.typeId === 2456)?.state;
    };

    expect(await waitFor(drones)).toHaveAccessibleName("Drones: Launched — change");
    fireEvent.click(drones());
    expect(drones()).toHaveAccessibleName("Drones: In the bay — change");
    await waitFor(() => expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")).toHaveLength(1));
    expect(savedDrones(1)).toBe("offline");

    // Shift or alt make no difference to a drone.
    fireEvent.click(drones(), { shiftKey: true });
    await waitFor(() => expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")).toHaveLength(2));
    expect(savedDrones(2)).toBe("active");
  });

  test("a mouse click on a state leaves no focus ring behind; the keyboard keeps focus", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const button = () =>
      within(within(table).getByRole("row", { name: /AutoCannon/ })).getByRole("button", { name: /^State:/ });
    const target = await waitFor(button);

    target.focus();
    fireEvent.click(target, { detail: 1, shiftKey: true });
    expect(document.activeElement).not.toBe(button());

    button().focus();
    // Enter or Space on a focused button clicks with detail 0.
    fireEvent.click(button(), { detail: 0 });
    expect(document.activeElement).toBe(button());
  });

  test("a state toggle that fails to save goes back and says so", async () => {
    stubFetch({ failSave: true });
    renderPage(`/fittings/${FIT_ID}`);

    const table = await screen.findByRole("table", { name: "Modules" });
    const button = () =>
      within(within(table).getByRole("row", { name: /Microwarpdrive/ })).getByRole("button", { name: /^State:/ });
    fireEvent.click(await waitFor(button));

    expect(await screen.findByText("Couldn't save the module state")).toBeInTheDocument();
    expect(button()).toHaveAccessibleName(/^State: Active/);
  });

  test("saving puts the whole fit and leaves editing", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const bar = await startEditing();

    fireEvent.change(screen.getByLabelText("Fit name"), { target: { value: "Renamed" } });
    expect(bar).toHaveTextContent("1 unsaved change");
    fireEvent.click(within(bar).getByRole("button", { name: "Save fit" }));

    await waitFor(() => expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")).toHaveLength(1));
    const body = JSON.parse(String((calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")[0][1] as RequestInit).body));
    expect(body.name).toBe("Renamed");
    expect(body.items).toHaveLength(6);
    await waitFor(() => expect(screen.queryByRole("region", { name: "Unsaved changes" })).not.toBeInTheDocument());
    expect(sessionStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  test("discarding drops the changes", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const bar = await startEditing();

    fireEvent.change(screen.getByLabelText("Fit name"), { target: { value: "Renamed" } });
    expect(sessionStorage.getItem(STORAGE_KEY)).not.toBeNull();
    fireEvent.click(within(bar).getByRole("button", { name: "Discard" }));

    expect(screen.queryByRole("region", { name: "Unsaved changes" })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Null Transport" })).toBeInTheDocument();
    expect(sessionStorage.getItem(STORAGE_KEY)).toBeNull();
    expect(calls(fetchSpy, `/api/fittings/${FIT_ID}`, "PUT")).toHaveLength(0);
  });

  test("an unsaved draft left behind comes back when the fit is opened again", async () => {
    stubFetch();
    sessionStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ name: "Half done", description: "", items: [] }),
    );
    renderPage(`/fittings/${FIT_ID}`);

    const bar = await screen.findByRole("region", { name: "Unsaved changes" });
    expect(bar).toHaveTextContent("Unsaved changes restored");
    expect(screen.getByLabelText("Fit name")).toHaveValue("Half done");
  });

  test("closing the tab with unsaved changes asks first", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    await startEditing();

    const clean = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(clean);
    expect(clean.defaultPrevented).toBe(false);

    fireEvent.change(screen.getByLabelText("Fit name"), { target: { value: "Renamed" } });
    const dirty = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(dirty);
    expect(dirty.defaultPrevented).toBe(true);
  });
});
