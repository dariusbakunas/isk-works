import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { PlannerWorkbench } from "../planner-workbench";

describe("PlannerWorkbench", () => {
  test("keeps the planning toolbar and worksheet visible", () => {
    render(
      <PlannerWorkbench
        actionBar={<div>Actions</div>}
        error=""
        previewUpdating={false}
        secondary={<div>Recipe details</div>}
        toolbar={<div>Planning toolbar</div>}
        worksheet={<div>Worksheet</div>}
      />,
    );

    expect(screen.getByText("Planning toolbar")).toBeInTheDocument();
    expect(screen.getByText("Worksheet")).toBeInTheDocument();
    expect(screen.getByText("Recipe details")).toBeInTheDocument();
    expect(screen.getByText("Actions")).toBeInTheDocument();
  });

  test("renders an overlay, error, and updating state", () => {
    render(
      <PlannerWorkbench
        actionBar={<div>Actions</div>}
        error="Price source is unavailable."
        overlay={<div>Blueprint editor</div>}
        previewUpdating
        toolbar={<div>Planning toolbar</div>}
        worksheet={<div>Worksheet</div>}
      />,
    );

    expect(screen.getByText("Blueprint editor")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Price source is unavailable.");
    expect(screen.getByText("Worksheet").parentElement).toHaveAttribute("aria-busy", "true");
  });

  test("surfaces a status message while waiting on a market price retry", () => {
    const { rerender } = render(
      <PlannerWorkbench
        actionBar={<div>Actions</div>}
        error=""
        previewUpdating
        statusMessage="Waiting for fresh market prices to sync (attempt 1 of 4)..."
        toolbar={<div>Planning toolbar</div>}
        worksheet={<div>Worksheet</div>}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent(
      "Waiting for fresh market prices to sync (attempt 1 of 4)...",
    );

    rerender(
      <PlannerWorkbench
        actionBar={<div>Actions</div>}
        error=""
        previewUpdating={false}
        statusMessage={null}
        toolbar={<div>Planning toolbar</div>}
        worksheet={<div>Worksheet</div>}
      />,
    );

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
