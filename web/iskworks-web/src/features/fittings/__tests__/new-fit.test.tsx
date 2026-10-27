import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { meta, stubFetch } from "./fixtures";
import { location, renderPage } from "./render";

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("Fittings › new fit", () => {
  test("a new fit starts from a ship, empty, in the editor", async () => {
    const fetchSpy = stubFetch();
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "New fit" }));
    const dialog = screen.getByRole("dialog", { name: "New fit" });
    fireEvent.change(within(dialog).getByRole("searchbox", { name: "Search ships" }), { target: { value: "rif" } });
    fireEvent.click(await within(dialog).findByRole("button", { name: "Rifter" }));
    expect(within(dialog).getByLabelText("Fit name")).toHaveValue("Rifter fit");
    fireEvent.click(within(dialog).getByRole("button", { name: "Create fit" }));

    await waitFor(() => expect(location()).toBe("/fittings/copy-id"));
    expect(await screen.findByRole("region", { name: "Unsaved changes" })).toBeInTheDocument();
    const create = fetchSpy.mock.calls.find(
      ([input, init]) => String(input).endsWith("/api/fittings") && (init as RequestInit | undefined)?.method === "POST",
    );
    expect(JSON.parse(String((create![1] as RequestInit).body))).toEqual({
      name: "Rifter fit",
      description: "",
      shipTypeId: 587,
      items: [],
    });
    expect(
      fetchSpy.mock.calls.some(([input]) => String(input).includes("/api/fittings/catalogue?kind=ships&q=rif")),
    ).toBe(true);
  });

  test("a new fit needs a ship and a name", async () => {
    stubFetch();
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "New fit" }));
    const dialog = screen.getByRole("dialog", { name: "New fit" });
    expect(within(dialog).getByRole("button", { name: "Create fit" })).toBeDisabled();
  });

  test("without engine data there are no new fits", async () => {
    stubFetch({ meta: meta(false) });
    renderPage();
    expect(await screen.findByRole("button", { name: "New fit" })).toBeDisabled();
  });
});
