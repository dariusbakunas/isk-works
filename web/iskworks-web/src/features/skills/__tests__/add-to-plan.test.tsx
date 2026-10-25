import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SkillsPage } from "../skills-page";
import { catalogue, characterDetail, DRONES, PLAN_ID, planDetail, planSummary, roster, SCOUT_DRONES } from "./fixtures";

const NOW = new Date("2026-10-25T12:00:00Z");

function stubFetch(plans = [planSummary({ name: "Drone basics" })]) {
  const puts: Array<{ entries: Array<{ skillTypeId: number; level: number }> }> = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const ok = (payload: unknown) =>
      new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } });
    if (url.includes(`/api/skill-plans/${PLAN_ID}`)) {
      if (method === "PUT") {
        puts.push(JSON.parse(String(init?.body)));
        return ok({ ...planDetail(), inserted: [{ skillTypeId: DRONES, level: 1 }] });
      }
      return ok(planDetail());
    }
    if (url.includes("/api/skill-plans")) return ok(plans);
    if (url.includes("/api/skills/catalogue")) return ok(catalogue());
    if (url.endsWith("/api/characters")) return ok(roster());
    return ok(characterDetail({ connectionId: url.split("/").pop()! }));
  });
  return puts;
}

function renderInspector(skillId: number) {
  return render(
    <MemoryRouter initialEntries={[`/skills?skill=${skillId}`]}>
      <Routes>
        <Route element={<SkillsPage />} path="/skills/*" />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Skills › Add to plan", () => {
  test("appends the skill's next level to a chosen plan", async () => {
    const puts = stubFetch();
    renderInspector(SCOUT_DRONES);
    const inspector = await screen.findByRole("complementary", { name: "Scout Drone Operation" });

    fireEvent.click(within(inspector).getByRole("button", { name: "Add to plan…" }));
    const plan = await within(inspector).findByRole("combobox", { name: "Plan" });
    expect(plan).toHaveValue(PLAN_ID);
    expect(within(inspector).getByRole("combobox", { name: "Level" })).toHaveValue("1");
    fireEvent.click(within(inspector).getByRole("button", { name: "Add" }));

    expect(await within(inspector).findByText("Added Scout Drone Operation I and 1 prerequisite to Drone basics")).toBeInTheDocument();
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0].entries.at(-1)).toEqual({ skillTypeId: SCOUT_DRONES, level: 1 });
  });

  test("with no plans it points to creating one", async () => {
    stubFetch([]);
    renderInspector(SCOUT_DRONES);
    const inspector = await screen.findByRole("complementary", { name: "Scout Drone Operation" });

    fireEvent.click(within(inspector).getByRole("button", { name: "Add to plan…" }));

    expect(await within(inspector).findByRole("link", { name: "Create a plan" })).toHaveAttribute("href", expect.stringContaining("/skills/plans"));
  });

  test("a skill already at V can't be added", async () => {
    stubFetch();
    renderInspector(DRONES);
    const inspector = await screen.findByRole("complementary", { name: "Drones" });

    expect(within(inspector).queryByRole("button", { name: "Add to plan…" })).not.toBeInTheDocument();
  });
});
