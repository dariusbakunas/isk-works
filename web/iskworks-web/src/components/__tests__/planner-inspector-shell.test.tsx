import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import { PlannerInspectorShell } from "../planner-inspector-shell";

describe("PlannerInspectorShell", () => {
  test("exposes responsive presentations and closes with Escape", async () => {
    const onClose = vi.fn();
    render(
      <PlannerInspectorShell onClose={onClose} open title="Tritanium">
        <div>Inspector body</div>
      </PlannerInspectorShell>,
    );

    const inspector = screen.getByRole("complementary", { name: "Tritanium" });
    expect(inspector).toHaveAttribute("data-mobile-presentation", "bottom-sheet");
    expect(inspector).toHaveAttribute("data-desktop-presentation", "side-panel");
    expect(inspector).toHaveAttribute("data-desktop-breakpoint", "1024px");
    expect(screen.getByRole("button", { name: "Close item inspector" })).toHaveClass("h-7", "w-7");
    expect(screen.getByRole("button", { name: "Close item inspector" })).not.toHaveClass("iw-icon-button");
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("hideDefaultHeader skips the generic header but keeps an accessible name", () => {
    render(
      <PlannerInspectorShell hideDefaultHeader onClose={vi.fn()} open title="Tritanium">
        <div>Inspector body</div>
      </PlannerInspectorShell>,
    );

    expect(screen.getByRole("complementary", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close item inspector" })).not.toBeInTheDocument();
  });

  test("does not render when closed", () => {
    render(
      <PlannerInspectorShell onClose={vi.fn()} open={false} title="Tritanium">
        <div>Inspector body</div>
      </PlannerInspectorShell>,
    );
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  test("dismissLabel names the mobile backdrop button", () => {
    render(
      <PlannerInspectorShell dismissLabel="Dismiss build settings" onClose={vi.fn()} open title="Build settings">
        <div>Inspector body</div>
      </PlannerInspectorShell>,
    );

    expect(screen.getByRole("button", { name: "Dismiss build settings" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Dismiss item inspector" })).not.toBeInTheDocument();
  });

  test("X-button close (not just Escape) restores focus via returnFocusSelector", async () => {
    const user = userEvent.setup();
    render(
      <>
        <button data-build-settings-trigger type="button">Edit build settings</button>
        <PlannerInspectorShell
          closeLabel="Close build settings"
          onClose={vi.fn()}
          open
          returnFocusSelector="[data-build-settings-trigger]"
          title="Build settings"
        >
          <div>Inspector body</div>
        </PlannerInspectorShell>
      </>,
    );

    await user.click(screen.getByRole("button", { name: "Close build settings" }));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Edit build settings" })).toHaveFocus(),
    );
  });
});
