import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { AURA, FIT_ID, FRESH, MIRA, stubFetch } from "./fixtures";
import { location, renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

async function panel() {
  const region = await screen.findByRole("region", { name: "Pilots" });
  await within(region).findByText("Mira");
  return region;
}

const rowOf = (region: HTMLElement, name: string) => {
  const table = within(region).getByRole("table", { name: "Pilots" });
  return within(table).getByText(name, { exact: true }).closest("tr")!;
};

describe("Fittings › pilots", () => {
  test("every character is listed closest first, with stats on its own skills", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    const names = [...region.querySelectorAll<HTMLElement>("tr[data-row-key]")].map((row) => row.dataset.rowKey);
    expect(names).toEqual([MIRA, AURA, FRESH, expect.any(String)]);
    expect(within(region).getByText("Best: Mira")).toBeInTheDocument();

    const aura = rowOf(region, "Aura");
    expect(aura).toHaveTextContent("Hull only");
    expect(aura).toHaveTextContent("6d 4h");
    expect(aura).toHaveTextContent("78,496");
    expect(rowOf(region, "Mira")).toHaveTextContent("Can use all");
    expect(rowOf(region, "Fresh Char")).toHaveTextContent("Can't fly");
    expect(rowOf(region, "Scout")).toHaveTextContent("Skills not synced");
  });

  test("picking a pilot row recalculates the fit for it", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    fireEvent.click(rowOf(region, "Fresh Char"));
    expect(location()).toBe(`/fittings/${FIT_ID}?pilot=${FRESH}`);
  });

  test("a focused pilot row is picked with Enter", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    fireEvent.keyDown(rowOf(region, "Aura"), { key: "Enter" });
    expect(location()).toBe(`/fittings/${FIT_ID}?pilot=${AURA}`);
    await waitFor(() => expect(rowOf(region, "Aura")).toHaveAttribute("aria-selected", "true"));
  });

  test("opening a pilot's missing skills doesn't pick that pilot", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    const toggle = within(region).getByRole("button", { name: "Missing skills for Fresh Char" });
    fireEvent.keyDown(toggle, { key: "Enter" });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(location()).toBe(`/fittings/${FIT_ID}`);
  });

  test("a pilot's missing skills list each level needed and what needs Omega", async () => {
    stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    fireEvent.click(within(region).getByRole("button", { name: "Missing skills for Fresh Char" }));
    const list = within(region).getByRole("list", { name: "Missing skills for Fresh Char" });
    const hauler = within(list).getByText("Caldari Hauler").closest("li")!;
    expect(hauler).toHaveTextContent("— → V");
    expect(hauler).toHaveTextContent("11d 20h");
    expect(within(list).getByText("Transport Ships").closest("li")).toHaveTextContent("Needs Omega");
  });

  test("a skill plan is made for the pilot and linked from the panel", async () => {
    const fetchSpy = stubFetch();
    renderPage(`/fittings/${FIT_ID}`);
    const region = await panel();

    fireEvent.click(within(region).getByRole("button", { name: "Missing skills for Aura" }));
    fireEvent.click(within(region).getByRole("button", { name: "Create skill plan" }));
    expect(await within(region).findByRole("link", { name: "Open Fit: Null Transport" })).toHaveAttribute(
      "href",
      `/skills/plans/plan-1?character=${AURA}`,
    );
    await waitFor(() => {
      const call = fetchSpy.mock.calls.find(([input]) => String(input).includes("/skill-plan"));
      expect(JSON.parse((call![1] as RequestInit).body as string)).toEqual({ connectionId: AURA });
    });
  });
});
