import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import { DropdownMenu } from "../dropdown-menu";

describe("DropdownMenu", () => {
  test("opens on click, invokes the selected item, and closes", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <DropdownMenu
        items={[{ key: "a", label: "Item A", onSelect }]}
        label="Record"
        variant="primary"
      />,
    );

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Record" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await user.click(screen.getByRole("menuitem", { name: "Item A" }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  test("closes when clicking outside", async () => {
    const user = userEvent.setup();
    render(
      <div>
        <DropdownMenu items={[{ key: "a", label: "Item A", onSelect: vi.fn() }]} label="More actions" variant="icon" />
        <button type="button">Outside</button>
      </div>,
    );

    await user.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Outside" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  test("closes on Escape", async () => {
    const user = userEvent.setup();
    render(<DropdownMenu items={[{ key: "a", label: "Item A", onSelect: vi.fn() }]} label="Record" variant="primary" />);

    await user.click(screen.getByRole("button", { name: "Record" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  test("portal menus render outside a clipping container and still select", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <div data-testid="clip" style={{ overflow: "hidden", width: 20 }}>
        <DropdownMenu
          align="end"
          items={[{ key: "a", label: "Item A", onSelect }]}
          label="Row options"
          portal
          variant="icon"
        />
      </div>,
    );

    await user.click(screen.getByRole("button", { name: "Row options" }));
    const menu = screen.getByRole("menu");
    expect(screen.getByTestId("clip")).not.toContainElement(menu);
    expect(menu.style.position).toBe("fixed");

    await user.click(screen.getByRole("menuitem", { name: "Item A" }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
