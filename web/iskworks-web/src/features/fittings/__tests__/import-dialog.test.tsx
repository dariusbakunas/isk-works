import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { preview, stubFetch } from "./fixtures";
import { location, renderPage } from "./render";

afterEach(() => vi.restoreAllMocks());

const TEXT = "[Bustard, Null Transport]\nDamage Control II";

async function openDialog() {
  fireEvent.click(await screen.findByRole("button", { name: "Import from clipboard" }));
  return screen.getByRole("dialog", { name: "Import EFT fitting" });
}

describe("Fittings › import", () => {
  test("pasted text previews the hull, its racks and the name it will save as", async () => {
    const fetchSpy = stubFetch();
    renderPage();
    const dialog = await openDialog();

    fireEvent.change(within(dialog).getByLabelText("Paste from EVE or pyfa"), { target: { value: TEXT } });
    expect(await within(dialog).findByText("Bustard")).toBeInTheDocument();
    expect(within(dialog).getByText("6 items recognized")).toBeInTheDocument();
    expect(within(dialog).getByTestId("preview-rack-med")).toHaveTextContent("2");
    expect(within(dialog).getByTestId("preview-rack-drone_bay")).toHaveTextContent("5");
    expect(within(dialog).getByTestId("preview-rack-cargo")).toHaveTextContent("100");
    expect(within(dialog).getByLabelText("Fit name")).toHaveValue("Null Transport");

    const parse = fetchSpy.mock.calls.find(([input]) => String(input).includes("/api/fittings/parse"));
    expect(JSON.parse((parse![1] as RequestInit).body as string)).toEqual({ text: TEXT });
  });

  test("unknown lines are listed by line number and block saving", async () => {
    stubFetch({
      preview: {
        fit: null,
        unknownLines: [{ line: 14, text: "Dread Gurista EM Shield Hardener" }],
        error: null,
      },
    });
    renderPage();
    const dialog = await openDialog();

    fireEvent.change(within(dialog).getByLabelText("Paste from EVE or pyfa"), { target: { value: TEXT } });
    expect(
      await within(dialog).findByText("Line 14: “Dread Gurista EM Shield Hardener” — no matching type"),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("1 unknown line")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save fit" })).toBeDisabled();
  });

  test("text that isn't a fit says why", async () => {
    stubFetch({ preview: { fit: null, unknownLines: [], error: "invalid EFT header" } });
    renderPage();
    const dialog = await openDialog();

    fireEvent.change(within(dialog).getByLabelText("Paste from EVE or pyfa"), { target: { value: "hello" } });
    expect(await within(dialog).findByText("invalid EFT header")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save fit" })).toBeDisabled();
  });

  test("saving imports the text under the chosen name and opens the fit", async () => {
    const fetchSpy = stubFetch({ preview: preview() });
    renderPage();
    const dialog = await openDialog();

    fireEvent.change(within(dialog).getByLabelText("Paste from EVE or pyfa"), { target: { value: TEXT } });
    await within(dialog).findByText("Bustard");
    fireEvent.change(within(dialog).getByLabelText("Fit name"), { target: { value: "Hauler" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save fit" }));

    await waitFor(() => expect(location()).toBe("/fittings/new-fit-id"));
    const save = fetchSpy.mock.calls.find(([input]) => String(input).includes("/api/fittings/import"));
    expect(JSON.parse((save![1] as RequestInit).body as string)).toEqual({ text: TEXT, name: "Hauler" });
  });

  test("closing the dialog leaves nothing saved", async () => {
    const fetchSpy = stubFetch();
    renderPage();
    const dialog = await openDialog();

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(fetchSpy.mock.calls.some(([input]) => String(input).includes("/import"))).toBe(false);
  });
});
