import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillPlanDetail } from "../../../api/skills";
import { SkillsPage } from "../skills-page";
import {
  AURA,
  catalogue,
  characterDetail,
  DRONE_INTERFACING,
  DRONES,
  MIRA,
  PLAN_ID,
  planDetail,
  roster,
  SCOUT_DRONES,
} from "./fixtures";

const NOW = new Date("2026-10-25T12:00:00Z");

interface Call {
  method: string;
  url: string;
  body: unknown;
}

function stubFetch(options: { detail?: SkillPlanDetail; onPut?: (body: { entries: unknown[] }) => Response | SkillPlanDetail } = {}) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const body = init?.body ? JSON.parse(String(init.body)) : null;
    calls.push({ method, url, body });
    const ok = (payload: unknown) =>
      new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } });
    if (url.includes("/export")) return new Response("Drones 1\nDrones 2", { status: 200 });
    if (url.includes(`/api/skill-plans/${PLAN_ID}`)) {
      if (method === "PUT") {
        const result = options.onPut?.(body) ?? planDetail();
        return result instanceof Response ? result : ok(result);
      }
      if (method === "DELETE") return new Response(null, { status: 204 });
      return ok(options.detail ?? planDetail());
    }
    if (url.includes("/api/skill-plans")) return ok([]);
    if (url.includes("/api/skills/catalogue")) return ok(catalogue());
    if (url.endsWith("/api/characters")) return ok(roster());
    return ok(characterDetail({ connectionId: url.split("/").pop()! }));
  });
  return calls;
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

function renderEditor() {
  return render(
    <MemoryRouter initialEntries={[`/skills/plans/${PLAN_ID}?character=${AURA}`]}>
      <Routes>
        <Route element={<><SkillsPage /><LocationProbe /></>} path="/skills/*" />
      </Routes>
    </MemoryRouter>,
  );
}

const location = () => screen.getByTestId("location").textContent ?? "";
const entries = () => screen.getByRole("list", { name: "Plan entries" });
const entryNames = () =>
  within(entries())
    .getAllByRole("listitem")
    .map((item) => item.getAttribute("data-entry"));
const puts = (calls: Call[]) => calls.filter((call) => call.method === "PUT");

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Skills › Plan editor", () => {
  test("shows the header character's progress through the plan", async () => {
    stubFetch();
    renderEditor();

    expect(await screen.findByRole("heading", { name: "Drone basics" })).toBeInTheDocument();
    expect(screen.getByTestId("plan-completion")).toHaveTextContent("71%");
    expect(screen.getByTestId("plan-completion")).toHaveTextContent("30m 52s");
    expect(screen.getByText("Missing skillbooks (1)")).toBeInTheDocument();
    expect(entryNames()).toEqual(["Drones I", "Drones II", "Drones III", "Drones IV", "Drones V", "Drone Interfacing I"]);
    const drones = within(entries()).getAllByRole("listitem")[0];
    expect(drones).toHaveAttribute("data-status", "done");
    expect(within(entries()).getAllByRole("listitem")[5]).toHaveTextContent("30m 52s");
    expect(screen.getByTestId("plan-footer")).toHaveTextContent("6 skills");

    const progress = screen.getByRole("list", { name: "Character progress" });
    const rows = within(progress).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Aura Valex");
    expect(rows[0]).toHaveTextContent("Closest");
    expect(rows[1]).toHaveTextContent("Mira Caldari");
    expect(rows[1]).toHaveTextContent("No skills data");
  });

  test("Hide trained leaves only what is left to train", async () => {
    stubFetch();
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(screen.getByLabelText("Hide trained"));

    expect(entryNames()).toEqual(["Drone Interfacing I"]);
  });

  test("moving an entry saves the new order, and a refused order is explained", async () => {
    const calls = stubFetch({
      onPut: () =>
        new Response(
          JSON.stringify({ error: { code: "validation_failed", message: "Drones IV must come before Drones V" } }),
          { status: 400, headers: { "content-type": "application/json" } },
        ),
    });
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(screen.getByRole("button", { name: "Move Drones V up" }));

    await screen.findByText("Drones IV must come before Drones V");
    const sent = puts(calls)[0].body as { entries: Array<{ skillTypeId: number; level: number }> };
    expect(sent.entries.slice(3, 5)).toEqual([
      { skillTypeId: DRONES, level: 5 },
      { skillTypeId: DRONES, level: 4 },
    ]);
    expect(entryNames()[3]).toBe("Drones IV");
  });

  test("removing an entry others need asks first and removes them too", async () => {
    const calls = stubFetch({
      onPut: (body) => ({ ...planDetail(), entries: planDetail().entries.slice(0, body.entries.length) }),
    });
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(screen.getByRole("button", { name: "Remove Drones V" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("Drone Interfacing I needs it");
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove 2 entries" }));

    await waitFor(() => expect(puts(calls)).toHaveLength(1));
    expect((puts(calls)[0].body as { entries: unknown[] }).entries).toHaveLength(4);
  });

  test("adding a skill saves it and announces inserted prerequisites", async () => {
    const calls = stubFetch({ onPut: () => ({ ...planDetail(), inserted: [{ skillTypeId: DRONES, level: 1 }] }) });
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.change(screen.getByRole("combobox", { name: "Add a skill" }), { target: { value: "scout" } });
    fireEvent.click(screen.getByRole("option", { name: "Scout Drone Operation" }));
    fireEvent.click(screen.getByRole("button", { name: "Add to plan" }));

    expect(await screen.findByText("Added Scout Drone Operation I and 1 prerequisite")).toBeInTheDocument();
    const sent = puts(calls)[0].body as { entries: Array<{ skillTypeId: number; level: number }> };
    expect(sent.entries.at(-1)).toEqual({ skillTypeId: SCOUT_DRONES, level: 1 });
  });

  test("copies the plan as game text", async () => {
    stubFetch();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(screen.getByRole("button", { name: "Copy for in-game import" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith("Drones 1\nDrones 2"));
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  test("deleting asks first, then returns to the list", async () => {
    const calls = stubFetch();
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(screen.getByRole("button", { name: "Delete plan" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete plan" }));

    await waitFor(() => expect(location()).toBe(`/skills/plans?character=${AURA}`));
    expect(calls.some((call) => call.method === "DELETE")).toBe(true);
  });

  test("picking a character in the progress column selects it", async () => {
    stubFetch();
    renderEditor();
    await screen.findByRole("heading", { name: "Drone basics" });

    fireEvent.click(within(screen.getByRole("list", { name: "Character progress" })).getByRole("button", { name: /Mira Caldari/ }));

    expect(location()).toContain(`character=${MIRA}`);
    expect(location()).toContain(`/skills/plans/${PLAN_ID}`);
    void DRONE_INTERFACING;
  });
});
