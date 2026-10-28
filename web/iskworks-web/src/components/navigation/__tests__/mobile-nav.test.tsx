import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { MobileNav } from "../mobile-nav";

function LocationProbe() {
  return <output data-testid="location">{useLocation().pathname}</output>;
}

function renderMobileNav(path = "/inventory/runway") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <MobileNav isAdmin={false} renderAccount={() => <button type="button">Account</button>} workspaceName="Aura Industries" />
      <LocationProbe />
    </MemoryRouter>,
  );
}

describe("MobileNav", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ downtime: false, retryAfterSeconds: null }), { status: 200 })),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  test("the top bar names the current page's section", () => {
    renderMobileNav();
    expect(screen.getByTestId("mobile-nav-title")).toHaveTextContent("Inventory");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("the drawer opens expanded, without a pin toggle, and closes on navigation", async () => {
    const user = userEvent.setup();
    renderMobileNav();

    await user.click(screen.getByRole("button", { name: "Open navigation" }));
    const drawer = screen.getByRole("dialog", { name: "Navigation" });
    expect(within(drawer).getByText("Industry")).toBeInTheDocument();
    expect(within(drawer).queryByRole("button", { name: /Pin navigation/ })).not.toBeInTheDocument();

    await user.click(within(drawer).getByRole("link", { name: "Board" }));
    expect(screen.getByTestId("location")).toHaveTextContent("/board");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("Escape closes the drawer and returns focus to the menu button", async () => {
    const user = userEvent.setup();
    renderMobileNav();
    const menuButton = screen.getByRole("button", { name: "Open navigation" });

    await user.click(menuButton);
    expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);
    await user.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(menuButton).toHaveFocus();
  });

  test("Tab stays inside the open drawer", async () => {
    const user = userEvent.setup();
    renderMobileNav();

    await user.click(screen.getByRole("button", { name: "Open navigation" }));
    const drawer = screen.getByRole("dialog");
    const focusable = drawer.querySelectorAll<HTMLElement>("a[href], button:not([disabled])");
    focusable[focusable.length - 1].focus();

    await user.tab();
    expect(focusable[0]).toHaveFocus();
    await user.tab({ shift: true });
    expect(focusable[focusable.length - 1]).toHaveFocus();
  });

  test("a click on the scrim closes the drawer", async () => {
    const user = userEvent.setup();
    renderMobileNav();

    await user.click(screen.getByRole("button", { name: "Open navigation" }));
    await user.click(screen.getByTestId("mobile-nav-scrim"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
