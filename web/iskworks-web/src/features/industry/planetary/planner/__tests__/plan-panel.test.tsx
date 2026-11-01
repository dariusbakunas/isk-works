import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import { edgeWeight, edges } from "../flow-diagram";
import { PlanPanel } from "../plan-panel";
import { SOLUTION } from "./plan-fixture";

function renderPanel(solving = false) {
  const onCadence = vi.fn();
  const onScanFirst = vi.fn();
  render(
    <PlanPanel
      characterNames={["Valka", "Alt"]}
      haulDays={3}
      onCadence={onCadence}
      onScanFirst={onScanFirst}
      resetDays={3}
      solution={SOLUTION}
      solving={solving}
    />,
  );
  return { onCadence, onScanFirst };
}

describe("PlanPanel", () => {
  test("summarises the plan and lists colonies per character", () => {
    renderPanel();
    expect(screen.getAllByText("1.4B")[0].closest("[title]")).toHaveAttribute(
      "title",
      expect.stringContaining("1,420,000,000"),
    );
    expect(screen.getByText("3 of 4")).toBeInTheDocument();
    expect(screen.getByText("7,800 m³")).toBeInTheDocument();

    const colonies = screen.getByRole("table", { name: "Colonies" });
    expect(within(colonies).getByText("Valka")).toBeInTheDocument();
    expect(within(colonies).getByText("Alt")).toBeInTheDocument();
    expect(within(colonies).getByText("Factory")).toBeInTheDocument();
    expect(within(colonies).getAllByText("Extract + refine")).toHaveLength(2);
    expect(within(colonies).getByText("+1 storage")).toBeInTheDocument();
    // Both extractors on Q-3HS5 IV share the Base Metals hotspot.
    expect(
      within(colonies).getAllByLabelText("Shared hotspot: another character extracts this resource here"),
    ).toHaveLength(2);
  });

  test("hovering a colony lights its flow node", () => {
    renderPanel();
    const node = () => document.querySelector('[data-colony="1"] rect');
    expect(node()).toHaveClass("stroke-border");
    fireEvent.mouseEnter(within(screen.getByRole("table", { name: "Colonies" })).getByText("Q-3HS5 V"));
    expect(node()).toHaveClass("stroke-primary");
    // Cross-character links are dashed.
    expect(document.querySelectorAll("path[data-cross]")).toHaveLength(1);
  });

  test("the grid highlights the current cadence and adopts a clicked one", async () => {
    const user = userEvent.setup();
    const { onCadence } = renderPanel();
    expect(screen.getByRole("button", { name: "Reset every 3d, haul every 3d" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Reset every 7d, haul every 14d" }));
    expect(onCadence).toHaveBeenCalledWith(7, 14);
  });

  test("scan these first points at the reading", async () => {
    const user = userEvent.setup();
    const { onScanFirst } = renderPanel();
    const unscanned = screen.getByRole("button", { name: /Q-3HS5 IV · Base Metals/ });
    // An unscanned reading shows what scanning it could add.
    expect(unscanned).toHaveTextContent("+120M");
    expect(unscanned).toHaveAttribute("title", expect.stringContaining("alone adds 120,000,000"));
    await user.click(unscanned);
    expect(onScanFirst).toHaveBeenCalledWith(4, 2267);
  });

  test("an empty plan points at the readings worth scanning", async () => {
    const user = userEvent.setup();
    const onScanFirst = vi.fn();
    render(
      <PlanPanel
        characterNames={["Valka"]}
        haulDays={3}
        onCadence={vi.fn()}
        onScanFirst={onScanFirst}
        resetDays={3}
        solution={{ ...SOLUTION, plan: { ...SOLUTION.plan, colonies: [], flows: [], inputs: [], sharedHotspots: [] } }}
        solving={false}
      />,
    );
    expect(screen.getByText(/Nothing you have scanned can be planned yet/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Q-3HS5 IV · Base Metals/ }));
    expect(onScanFirst).toHaveBeenCalledWith(4, 2267);
  });

  test("solving dims the previous result under a spinner", () => {
    renderPanel(true);
    expect(screen.getByRole("status")).toHaveTextContent("Optimising");
  });

  test("edges merge per link and weigh in three steps", () => {
    const links = edges(SOLUTION.plan.flows);
    expect(links).toHaveLength(3);
    expect(edgeWeight(30, 30)).toBe(5);
    expect(edgeWeight(15, 30)).toBe(3);
    expect(edgeWeight(5, 30)).toBe(1.5);
  });
});
