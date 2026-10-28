import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { LogOut } from "lucide-react";
import { useState } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { CommandPalette } from "../command-palette";
import type { PaletteEntry } from "../palette-search";

function LocationProbe() {
  return <output data-testid="location">{useLocation().pathname}</output>;
}

function renderPalette({ isAdmin = false, recents = ["/board", "/builds/blueprints"], run = vi.fn() } = {}) {
  const onClose = vi.fn();
  const actions: PaletteEntry[] = [
    { id: "action:sign-out", kind: "action", label: "Sign out", group: "Actions", icon: LogOut, keywords: [], run },
  ];
  render(
    <MemoryRouter initialEntries={["/inventory"]}>
      <CommandPalette actions={actions} isAdmin={isAdmin} onClose={onClose} recents={recents} />
      <LocationProbe />
    </MemoryRouter>,
  );
  return { onClose, run };
}

const options = () => within(screen.getByRole("listbox")).getAllByRole("option");
const activeOption = () => document.getElementById(screen.getByRole("combobox").getAttribute("aria-activedescendant") ?? "");

describe("CommandPalette", () => {
  afterEach(() => vi.restoreAllMocks());

  test("opens focused, with recents, actions and every page", () => {
    renderPalette();

    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveFocus();
    expect(options()[0]).toHaveTextContent("Board");
    expect(options()[1]).toHaveTextContent("Builds › Blueprints");
    expect(screen.getByText("Recent")).toBeInTheDocument();
    expect(screen.getByText("Actions", { selector: "[role=presentation]" })).toBeInTheDocument();
    expect(screen.getByText("All pages")).toBeInTheDocument();
    expect(activeOption()).toBe(options()[0]);
  });

  test("typing ranks results and the arrows move the selection, wrapping", async () => {
    const user = userEvent.setup();
    renderPalette();

    await user.type(screen.getByRole("combobox"), "mo");
    expect(options()[0]).toHaveTextContent("Market Orders");
    expect(activeOption()).toBe(options()[0]);

    await user.keyboard("{ArrowDown}");
    expect(activeOption()).toBe(options()[1]);
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(activeOption()).toBe(options().at(-1));
    await user.keyboard("{Home}");
    expect(activeOption()).toBe(options()[0]);
  });

  test("Enter opens the selected page and closes", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPalette();

    await user.type(screen.getByRole("combobox"), "orders{Enter}");
    expect(screen.getByTestId("location")).toHaveTextContent("/market-orders");
    expect(onClose).toHaveBeenCalled();
  });

  test("Ctrl+Enter opens the page in a new tab instead", async () => {
    const user = userEvent.setup();
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    renderPalette();

    await user.type(screen.getByRole("combobox"), "orders");
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(open).toHaveBeenCalledWith("/market-orders", "_blank", "noopener");
    expect(screen.getByTestId("location")).toHaveTextContent("/inventory");
  });

  test("Enter on an action runs it", async () => {
    const user = userEvent.setup();
    const { onClose, run } = renderPalette();

    await user.type(screen.getByRole("combobox"), "sign");
    expect(screen.getByText("Actions", { selector: "[role=presentation]" })).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(onClose).toHaveBeenCalled();
    expect(run).toHaveBeenCalledTimes(1);
  });

  test("clicking a result opens it", async () => {
    const user = userEvent.setup();
    renderPalette();

    await user.type(screen.getByRole("combobox"), "assets");
    await user.click(options()[0]);
    expect(screen.getByTestId("location")).toHaveTextContent("/assets");
  });

  test("a keyword match carries its chip", async () => {
    const user = userEvent.setup();
    renderPalette();

    await user.type(screen.getByRole("combobox"), "bp");
    expect(options()[0]).toHaveTextContent("Builds › Blueprints");
    expect(within(options()[0]).getByText("matches: bp")).toBeInTheDocument();
  });

  test("says so when nothing matches, and hides Admin from non-admins", async () => {
    const user = userEvent.setup();
    renderPalette();

    await user.type(screen.getByRole("combobox"), "zzz");
    expect(screen.getByText("No page matches “zzz”")).toBeInTheDocument();

    await user.clear(screen.getByRole("combobox"));
    await user.type(screen.getByRole("combobox"), "admin");
    expect(screen.queryByRole("option", { name: /Admin/ })).not.toBeInTheDocument();
  });

  test("lists Admin for admins", async () => {
    const user = userEvent.setup();
    renderPalette({ isAdmin: true });

    await user.type(screen.getByRole("combobox"), "admin");
    expect(options()[0]).toHaveTextContent("Admin");
  });

  test("Escape closes and gives focus back", async () => {
    const user = userEvent.setup();

    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <MemoryRouter>
          <button onClick={() => setOpen(true)} type="button">
            Search
          </button>
          {open ? <CommandPalette actions={[]} isAdmin={false} onClose={() => setOpen(false)} recents={[]} /> : null}
        </MemoryRouter>
      );
    }
    render(<Harness />);

    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(screen.getByRole("combobox")).toHaveFocus();
    await user.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toHaveFocus();
  });
});
