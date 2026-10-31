import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import type { PlannerResource } from "../../../../../api/pi-planner";
import { DensityCell } from "../density-cell";

const NOW = Date.parse("2026-11-01T00:00:00Z");

function resource(overrides: Partial<PlannerResource> = {}): PlannerResource {
  return {
    typeId: 2267,
    name: "Base Metals",
    densityPct: null,
    headValue: null,
    updatedAt: null,
    defaultDensityPct: "15",
    ...overrides,
  };
}

function renderCell(entry: PlannerResource, saved = false) {
  const onCommit = vi.fn();
  const onEditHeadValue = vi.fn();
  render(
    <DensityCell
      index={0}
      nowMs={NOW}
      onCommit={onCommit}
      onEditHeadValue={onEditHeadValue}
      resource={entry}
      saved={saved}
    />,
  );
  return { onCommit, onEditHeadValue, input: screen.queryByLabelText("Base Metals density percent") };
}

describe("DensityCell", () => {
  test("blank shows the band default as a placeholder and an empty bar", () => {
    const { input } = renderCell(resource());
    expect(input).toHaveAttribute("placeholder", "~15");
    expect(input).toHaveValue("");
    expect(screen.getByTestId("density-cell")).toHaveAttribute(
      "title",
      expect.stringContaining("not scanned — using 15 %"),
    );
    expect(document.querySelector("[data-segments]")).toHaveAttribute("data-segments", "0");
  });

  test("typed shows the value and fills the bar", () => {
    const { input } = renderCell(resource({ densityPct: "58", updatedAt: "2026-10-31T00:00:00Z" }));
    expect(input).toHaveValue("58");
    expect(document.querySelector("[data-segments]")).toHaveAttribute("data-segments", "6");
    expect(screen.queryByLabelText("Stale reading")).toBeNull();
  });

  test("typing commits on blur and Escape reverts", async () => {
    const user = userEvent.setup();
    const { input, onCommit } = renderCell(resource({ densityPct: "40" }));
    await user.click(input!);
    await user.keyboard("62");
    await user.tab();
    expect(onCommit).toHaveBeenCalledWith("62");

    onCommit.mockClear();
    await user.click(input!);
    await user.keyboard("9{Escape}");
    expect(input).toHaveValue("40");
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("Delete clears and commits null", async () => {
    const user = userEvent.setup();
    const { input, onCommit } = renderCell(resource({ densityPct: "40" }));
    await user.click(input!);
    fireEvent.keyDown(input!, { key: "Delete" });
    fireEvent.blur(input!);
    expect(onCommit).toHaveBeenCalledWith(null);
  });

  test("saved shows a check", () => {
    renderCell(resource({ densityPct: "40" }), true);
    expect(screen.getByLabelText("Saved")).toBeInTheDocument();
  });

  test("a head value replaces the input and opens its editor", async () => {
    const user = userEvent.setup();
    const { input, onEditHeadValue } = renderCell(resource({ headValue: "146.1", densityPct: "52" }));
    expect(input).toBeNull();
    await user.click(screen.getByRole("button", { name: "Base Metals head value 146.1, edit" }));
    expect(onEditHeadValue).toHaveBeenCalled();
    expect(screen.getByText(/HV 146/)).toBeInTheDocument();
  });

  test("a reading over 30 days old is stale", () => {
    renderCell(resource({ densityPct: "40", updatedAt: "2026-09-20T00:00:00Z" }));
    expect(screen.getByLabelText("Stale reading")).toBeInTheDocument();
    expect(screen.getByTestId("density-cell")).toHaveAttribute("title", expect.stringContaining("42 days ago"));
  });

  test("over 100 is invalid and never commits", async () => {
    const user = userEvent.setup();
    const { input, onCommit } = renderCell(resource());
    await user.click(input!);
    await user.keyboard("120");
    expect(input).toHaveAttribute("aria-invalid", "true");
    await user.tab();
    expect(onCommit).not.toHaveBeenCalled();
  });
});
