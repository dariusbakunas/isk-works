import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { SavedPlansPage } from "../saved-plans-page";
import { SUMMARY } from "./saved-plans-fixture";

const api = vi.hoisted(() => ({
  listSavedPlans: vi.fn(),
  updateSavedPlan: vi.fn(),
  deleteSavedPlan: vi.fn(),
}));
vi.mock("../../../../../api/pi-plans", async () => ({
  ...(await vi.importActual("../../../../../api/pi-plans")),
  ...api,
}));

let location = "";
function Spy() {
  location = useLocation().pathname;
  return null;
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/planetary/plans"]}>
      <Routes>
        <Route path="/planetary/plans" element={<SavedPlansPage />} />
        <Route path="/planetary/plans/:id" element={<p>Opened</p>} />
      </Routes>
      <Spy />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  Object.values(api).forEach((mock) => mock.mockReset());
  api.listSavedPlans.mockResolvedValue([SUMMARY, { ...SUMMARY, id: "plan-2", name: "Older", setUp: 0 }]);
});

describe("SavedPlansPage", () => {
  test("lists each plan with its progress and opens one", async () => {
    const user = userEvent.setup();
    renderPage();
    const table = await screen.findByRole("table", { name: "Saved plans" });
    expect(within(table).getByText("Q-3HS5 · Mechanical Parts")).toBeInTheDocument();
    expect(within(table).getByText("1 of 3 set up")).toBeInTheDocument();
    expect(within(table).getByText("0 of 3 set up")).toBeInTheDocument();
    expect(within(table).getAllByText("Q-3HS5").length).toBeGreaterThan(0);
    expect(within(table).getAllByText("1.4B")[0].closest("[title]")).toHaveAttribute(
      "title",
      expect.stringContaining("1,420,000,000"),
    );

    await user.click(within(table).getByText("Q-3HS5 · Mechanical Parts"));
    expect(location).toBe("/planetary/plans/plan-1");
  });

  test("renames a plan from its menu", async () => {
    const user = userEvent.setup();
    api.updateSavedPlan.mockResolvedValue({ ...SUMMARY, name: "Renamed" });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Actions for Q-3HS5 · Mechanical Parts" }));
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    const dialog = screen.getByRole("dialog", { name: "Rename plan" });
    const name = within(dialog).getByLabelText("Name");
    expect(name).toHaveValue("Q-3HS5 · Mechanical Parts");
    await user.clear(name);
    await user.type(name, "Renamed");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(api.updateSavedPlan).toHaveBeenCalledWith("plan-1", { name: "Renamed" });
    expect(await screen.findByText("Renamed")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("deletes a plan after confirming", async () => {
    const user = userEvent.setup();
    api.deleteSavedPlan.mockResolvedValue(undefined);
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Actions for Older" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(api.deleteSavedPlan).not.toHaveBeenCalled();
    await user.click(within(screen.getByRole("dialog", { name: "Delete Older?" })).getByRole("button", { name: "Delete" }));

    expect(api.deleteSavedPlan).toHaveBeenCalledWith("plan-2");
    await waitFor(() => expect(screen.queryByText("Older")).not.toBeInTheDocument());
  });

  test("with nothing saved it points at the Planner", async () => {
    api.listSavedPlans.mockResolvedValue([]);
    renderPage();
    expect(await screen.findByText(/No saved plans yet/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the Planner" })).toHaveAttribute("href", "/planetary/planner");
  });
});
