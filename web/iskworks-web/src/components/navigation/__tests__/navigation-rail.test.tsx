import { act, fireEvent, render, screen, within } from "@testing-library/react";
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

  describe("peek", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => {
      vi.useRealTimers();
      vi.unstubAllGlobals();
    });

    test("hovering for 150ms opens labels over the page without widening the slot", () => {
      renderRail();
      const rail = screen.getByRole("complementary", { name: "App navigation" });

      fireEvent.mouseEnter(rail);
      act(() => vi.advanceTimersByTime(149));
      expect(rail).toHaveAttribute("data-peek", "false");
      act(() => vi.advanceTimersByTime(1));

      expect(rail).toHaveAttribute("data-peek", "true");
      expect(rail).toHaveStyle({ width: "220px" });
      expect(screen.getByTestId("nav-rail-slot")).toHaveStyle({ width: "52px" });
      expect(within(screen.getByRole("link", { name: "Builds" })).getByText("Builds")).not.toHaveClass("sr-only");
    });

    test("leaving for 200ms closes it", () => {
      renderRail();
      const rail = screen.getByRole("complementary", { name: "App navigation" });
      fireEvent.mouseEnter(rail);
      act(() => vi.advanceTimersByTime(150));

      fireEvent.mouseLeave(rail);
      act(() => vi.advanceTimersByTime(199));
      expect(rail).toHaveAttribute("data-peek", "true");
      act(() => vi.advanceTimersByTime(1));
      expect(rail).toHaveAttribute("data-peek", "false");
    });

    test("keyboard focus opens it at once and leaving the rail closes it", () => {
      renderRail();
      const rail = screen.getByRole("complementary", { name: "App navigation" });

      fireEvent.focusIn(screen.getByRole("link", { name: "Builds" }));
      expect(rail).toHaveAttribute("data-peek", "true");
      fireEvent.focusOut(screen.getByRole("link", { name: "Builds" }), { relatedTarget: document.body });
      expect(rail).toHaveAttribute("data-peek", "false");
    });

    test("touch-only screens never peek", () => {
      vi.stubGlobal("matchMedia", (query: string) => ({ matches: query === "(hover: none)" }));
      renderRail();
      const rail = screen.getByRole("complementary", { name: "App navigation" });

      fireEvent.mouseEnter(rail);
      act(() => vi.advanceTimersByTime(500));
      expect(rail).toHaveAttribute("data-peek", "false");
    });
  });

  test("[ toggles the pin, except while typing", () => {
    const onTogglePinned = vi.fn();
    render(
      <MemoryRouter initialEntries={["/inventory"]}>
        <input aria-label="Filter" />
        <NavigationRail isAdmin={false} onTogglePinned={onTogglePinned} pinned={false} workspaceName="Aura Industries" />
      </MemoryRouter>,
    );

    fireEvent.keyDown(document.body, { key: "[" });
    expect(onTogglePinned).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(screen.getByRole("textbox", { name: "Filter" }), { key: "[" });
    fireEvent.keyDown(document.body, { key: "[", metaKey: true });
    expect(onTogglePinned).toHaveBeenCalledTimes(1);
  });
});
