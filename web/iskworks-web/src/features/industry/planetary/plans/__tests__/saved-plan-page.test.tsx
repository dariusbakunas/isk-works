import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ApiError } from "../../../../../api/workspace";
import { SavedPlanPage } from "../saved-plan-page";
import { SAVED } from "./saved-plans-fixture";

const api = vi.hoisted(() => ({
  getSavedPlan: vi.fn(),
  updateSavedPlan: vi.fn(),
  deleteSavedPlan: vi.fn(),
}));
vi.mock("../../../../../api/pi-plans", async () => ({
  ...(await vi.importActual("../../../../../api/pi-plans")),
  ...api,
}));

let location = "";
function Spy() {
  const current = useLocation();
  location = `${current.pathname}${current.search}`;
  return null;
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/planetary/plans/plan-1"]}>
      <Routes>
        <Route path="/planetary/plans/:id" element={<SavedPlanPage />} />
        <Route path="/planetary/plans" element={<p>All plans</p>} />
        <Route path="/planetary/planner" element={<p>Planner</p>} />
      </Routes>
      <Spy />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  Object.values(api).forEach((mock) => mock.mockReset());
  api.getSavedPlan.mockResolvedValue(SAVED);
});

describe("SavedPlanPage", () => {
  test("shows the saved plan read-only with its set-up ticks", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { name: "Q-3HS5 · Mechanical Parts" })).toBeInTheDocument();
    expect(api.getSavedPlan).toHaveBeenCalledWith("plan-1");
    expect(screen.getByText("1 of 3 set up")).toBeInTheDocument();

    const colonies = screen.getByRole("table", { name: "Colonies" });
    expect(within(colonies).getByText("Valka")).toBeInTheDocument();
    expect(within(colonies).getByText(/1 of 2 set up/)).toBeInTheDocument();
    const boxes = within(colonies).getAllByRole("checkbox");
    expect(boxes).toHaveLength(3);
    expect(boxes[0]).toBeChecked();
    expect(boxes[1]).not.toBeChecked();

    // Read-only: no cadence to change, no readings to jump to.
    expect(screen.queryByRole("button", { name: /Reset every/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Q-3HS5 I · Heavy Metals/ })).not.toBeInTheDocument();
    // Templates still copy.
    expect(screen.getAllByRole("button", { name: "Show the template for Q-3HS5 IV" })[0]).toBeInTheDocument();
  });

  test("ticking a colony saves the whole list", async () => {
    const user = userEvent.setup();
    api.updateSavedPlan.mockResolvedValue({ ...SAVED, setUpColonies: [0, 2], setUp: 2 });
    renderPage();
    await screen.findByRole("table", { name: "Colonies" });
    await user.click(screen.getByRole("checkbox", { name: "Q-3HS5 IV set up (Alt)" }));

    expect(api.updateSavedPlan).toHaveBeenCalledWith("plan-1", { setUpColonies: [0, 2] });
    expect(screen.getByRole("checkbox", { name: "Q-3HS5 IV set up (Alt)" })).toBeChecked();
    expect(screen.getByText("2 of 3 set up")).toBeInTheDocument();
  });

  test("a tick that doesn't save is undone", async () => {
    const user = userEvent.setup();
    api.updateSavedPlan.mockRejectedValue(new ApiError(500, { code: "x", message: "Database down" }));
    renderPage();
    await screen.findByRole("table", { name: "Colonies" });
    await user.click(screen.getByRole("checkbox", { name: "Q-3HS5 V set up (Valka)" }));

    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Q-3HS5 V set up (Valka)" })).not.toBeChecked());
    expect(screen.getByText("Database down")).toBeInTheDocument();
  });

  test("a snapshot from a newer version isn't rendered", async () => {
    api.getSavedPlan.mockResolvedValue({ ...SAVED, snapshotVersion: 99 });
    renderPage();
    expect(await screen.findByText(/saved by a newer version/)).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Colonies" })).not.toBeInTheDocument();
  });

  test("load scenario fills the planner and opens its Plan tab", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Load scenario" }));

    await waitFor(() => expect(location).toMatch(/^\/planetary\/planner\?/));
    const query = new URLSearchParams(location.split("?")[1]);
    expect(Object.fromEntries(query)).toEqual({ system: "30000797", radius: "2", tax: "5", haul: "7", sys: "1", estimate: "1" });
    expect(JSON.parse(window.localStorage.getItem("iskworks.piPlanner.characters") ?? "[]")).toEqual([
      { name: "Valka", interplanetaryConsolidation: 1, commandCenterUpgrades: 4 },
      { name: "Alt", interplanetaryConsolidation: 0, commandCenterUpgrades: 4 },
    ]);
  });

  test("load scenario asks before replacing other characters", async () => {
    const user = userEvent.setup();
    const mine = [{ name: "Someone else", interplanetaryConsolidation: 5, commandCenterUpgrades: 5 }];
    window.localStorage.setItem("iskworks.piPlanner.characters", JSON.stringify(mine));
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Load scenario" }));
    const dialog = screen.getByRole("dialog", { name: "Replace your planner characters?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(location).toBe("/planetary/plans/plan-1");
    expect(JSON.parse(window.localStorage.getItem("iskworks.piPlanner.characters") ?? "[]")).toEqual(mine);

    await user.click(screen.getByRole("button", { name: "Load scenario" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Replace" }));
    await waitFor(() => expect(location).toMatch(/^\/planetary\/planner\?/));
    expect(JSON.parse(window.localStorage.getItem("iskworks.piPlanner.characters") ?? "[]")[0].name).toBe("Valka");
  });

  test("deleting goes back to the list", async () => {
    const user = userEvent.setup();
    api.deleteSavedPlan.mockResolvedValue(undefined);
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Delete" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }));
    expect(api.deleteSavedPlan).toHaveBeenCalledWith("plan-1");
    await waitFor(() => expect(location).toBe("/planetary/plans"));
  });
});
