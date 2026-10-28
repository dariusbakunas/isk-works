import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { NavigationRail } from "../navigation-rail";

function renderRail(path = "/inventory", props: Partial<Parameters<typeof NavigationRail>[0]> = {}) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <NavigationRail isAdmin={false} onTogglePinned={() => {}} pinned={false} workspaceName="Aura Industries" {...props} />
    </MemoryRouter>,
  );
}

describe("NavigationRail", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ downtime: false, retryAfterSeconds: null }), { status: 200 })),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  test("collapsed, it takes 52px and keeps every label for screen readers", () => {
    renderRail();

    expect(screen.getByTestId("nav-rail-slot")).toHaveStyle({ width: "52px" });
    const builds = screen.getByRole("link", { name: "Builds" });
    expect(within(builds).getByText("Builds")).toHaveClass("sr-only");
    expect(screen.getByRole("button", { name: "Pin navigation" })).toHaveAttribute("aria-pressed", "false");
  });

  test("pinned, it takes 220px, shows labels and group headings", () => {
    renderRail("/inventory", { pinned: true });

    expect(screen.getByTestId("nav-rail-slot")).toHaveStyle({ width: "220px" });
    expect(within(screen.getByRole("link", { name: "Builds" })).getByText("Builds")).not.toHaveClass("sr-only");
    expect(screen.getByText("Industry")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Unpin navigation" })).toHaveAttribute("aria-pressed", "true");
  });

  test("a sub-page marks its parent as the current page", () => {
    renderRail("/inventory/runway");

    const nav = screen.getByRole("navigation", { name: "Primary navigation" });
    expect(within(nav).getByRole("link", { name: "Inventory" })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("link", { name: "Assets" })).not.toHaveAttribute("aria-current");
  });

  test("Admin sits in the footer, for admins only", () => {
    const { unmount } = renderRail("/inventory", { isAdmin: true });
    const footer = screen.getByRole("navigation", { name: "Account and help" });
    expect(within(footer).getByRole("link", { name: "Admin" })).toHaveAttribute("href", "/admin");
    unmount();

    renderRail();
    expect(screen.queryByRole("link", { name: "Admin" })).not.toBeInTheDocument();
  });

  test("the brand links to the Characters home", () => {
    renderRail();

    expect(screen.getByRole("link", { name: /ISK Works/ })).toHaveAttribute("href", "/characters");
    expect(screen.getByRole("link", { name: "Help" })).toHaveAttribute("href", "/help");
  });
});
