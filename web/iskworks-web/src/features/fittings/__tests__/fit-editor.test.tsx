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
