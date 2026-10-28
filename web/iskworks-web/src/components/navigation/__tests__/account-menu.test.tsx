import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import { AccountMenu, ApiVersionLine } from "../account-menu";

function renderMenu(props: Partial<Parameters<typeof AccountMenu>[0]> = {}) {
  const onSignOut = vi.fn();
  render(<AccountMenu apiVersion="v1.2.3 (a1b2c3d)" characterName="Aura Starfall" expanded onSignOut={onSignOut} signingOut={false} {...props} />);
  return { onSignOut };
}

describe("AccountMenu", () => {
  test("shows the character and version on the trigger, with the version's API title", () => {
    renderMenu();

    const trigger = screen.getByRole("button", { name: /Account/ });
    expect(trigger).toHaveTextContent("Aura Starfall");
    expect(screen.getByTitle("API v1.2.3 (a1b2c3d)")).toHaveTextContent("v1.2.3 (a1b2c3d)");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  test("collapsed, the name stays readable to screen readers only", () => {
    renderMenu({ expanded: false });

    expect(screen.getByText("Aura Starfall").closest(".sr-only")).not.toBeNull();
  });

  test("opens a menu with Sign out, and closes on Escape", async () => {
    const user = userEvent.setup();
    const { onSignOut } = renderMenu();

    await user.click(screen.getByRole("button", { name: /Account/ }));
    const menu = screen.getByRole("menu");
    await user.click(within(menu).getByRole("menuitem", { name: "Sign out" }));
    expect(onSignOut).toHaveBeenCalledTimes(1);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  test("closes on a click outside", async () => {
    const user = userEvent.setup();
    renderMenu();

    await user.click(screen.getByRole("button", { name: /Account/ }));
    await user.click(document.body);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  test("Sign out is disabled while signing out", async () => {
    const user = userEvent.setup();
    renderMenu({ signingOut: true });

    await user.click(screen.getByRole("button", { name: /Account/ }));
    expect(screen.getByRole("menuitem", { name: "Signing out..." })).toBeDisabled();
  });
});

describe("ApiVersionLine", () => {
  test("shows the version with its API title", () => {
    render(<ApiVersionLine apiVersion="dev" expanded />);
    expect(screen.getByTitle("API dev")).toHaveTextContent("dev");
  });
});
