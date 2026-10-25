import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillPlanSummary } from "../../../api/skills";
import { SkillsPage } from "../skills-page";
import { AURA, catalogue, characterDetail, PLAN_ID, planDetail, planSummary, roster } from "./fixtures";

const NOW = new Date("2026-10-25T12:00:00Z");

type Handler = (url: string, init: RequestInit | undefined) => unknown;

function stubFetch(handlers: Record<string, Handler> = {}, plans: SkillPlanSummary[] = [planSummary()]) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const custom = handlers[`${method} ${url.replace(/^https?:\/\/[^/]+/, "").split("?")[0]}`];
    const payload = custom
      ? custom(url, init)
      : url.includes("/api/skills/catalogue")
        ? catalogue()
        : url.includes(`/api/skill-plans/${PLAN_ID}`)
          ? planDetail()
          : url.includes("/api/skill-plans")
            ? plans
          : url.endsWith("/api/characters")
            ? roster()
            : characterDetail({ connectionId: url.split("/").pop()! });
    return new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } });
  });
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

function renderPage(path = "/skills/plans") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><SkillsPage /><LocationProbe /></>} path="/skills/*" />
      </Routes>
    </MemoryRouter>,
  );
}

const location = () => screen.getByTestId("location").textContent ?? "";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Skills › Plans", () => {
  test("lists plans with the selected character's progress", async () => {
    const fetchSpy = stubFetch();
    renderPage();

    const card = await screen.findByRole("link", { name: /F2 \| Boosh \+ Boosting/ });
    expect(card).toHaveTextContent("45% complete");
    expect(card).toHaveTextContent("76d 0h");
    expect(card).toHaveTextContent("11 missing skillbooks");
    expect(card).toHaveTextContent("11 skills");
    expect(card).toHaveAttribute("href", `/skills/plans/${PLAN_ID}?character=${AURA}`);
    expect(fetchSpy.mock.calls.some(([input]) => String(input).includes(`/api/skill-plans?characterId=${AURA}`))).toBe(true);
    expect(screen.getByRole("link", { name: "Plans" })).toHaveAttribute("aria-current", "page");
  });

  test("an empty list offers to create or import a plan", async () => {
    stubFetch({}, []);
    renderPage();

    const empty = await screen.findByText("No skill plans yet");
    const panel = empty.closest("div")!;
    expect(within(panel).getByRole("button", { name: "New plan" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Import from clipboard" })).toBeInTheDocument();
  });

  test("a new plan is created empty and opened", async () => {
    const create = vi.fn();
    stubFetch({
      "POST /api/skill-plans": (_url, init) => {
        create(JSON.parse(String(init?.body)));
        return planDetail({ entries: [], plan: { ...planDetail().plan, name: "Scouts" } });
      },
    });
    renderPage();
    await screen.findByRole("link", { name: /F2 \| Boosh/ });

    fireEvent.click(screen.getAllByRole("button", { name: "New plan" })[0]);
    const dialog = screen.getByRole("dialog", { name: "New skill plan" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Scouts" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create plan" }));

    await waitFor(() => expect(location()).toBe(`/skills/plans/${PLAN_ID}?character=${AURA}`));
    expect(create).toHaveBeenCalledWith({ name: "Scouts", description: "", entries: [] });
  });

  test("importing previews the pasted text and creates the plan", async () => {
    const imported = vi.fn();
    stubFetch({
      "POST /api/skill-plans/import/preview": () => ({
        entries: planDetail().entries.slice(0, 2),
        inserted: [{ skillTypeId: 3436, level: 1 }],
        unrecognized: [{ line: 3, text: "Armoured Comand 3", reason: "unknownSkill" }],
        error: null,
      }),
      "POST /api/skill-plans/import": (_url, init) => {
        imported(JSON.parse(String(init?.body)));
        return { ...planDetail(), unrecognized: [] };
      },
    });
    renderPage();
    await screen.findByRole("link", { name: /F2 \| Boosh/ });

    fireEvent.click(screen.getAllByRole("button", { name: "Import from clipboard" })[0]);
    const dialog = screen.getByRole("dialog", { name: "Import plan from clipboard" });
    const create = within(dialog).getByRole("button", { name: "Create plan" });
    expect(create).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Drones" } });
    fireEvent.change(within(dialog).getByLabelText("Plan text"), { target: { value: "Drones 2\nArmoured Comand 3" } });

    expect(await within(dialog).findByText("Recognized skills (2)")).toBeInTheDocument();
    expect(within(dialog).getByText("1 prerequisite will be added automatically: Drones I")).toBeInTheDocument();
    expect(within(dialog).getByText(/Line 3: 'Armoured Comand 3'/)).toBeInTheDocument();
    expect(create).toBeEnabled();
    fireEvent.click(create);

    await waitFor(() => expect(location()).toContain(`/skills/plans/${PLAN_ID}`));
    expect(imported).toHaveBeenCalledWith({ name: "Drones", description: "", text: "Drones 2\nArmoured Comand 3" });
  });

  test("text the server can't import keeps Create disabled and says why", async () => {
    stubFetch({
      "POST /api/skill-plans/import/preview": () => ({
        entries: [],
        inserted: [],
        unrecognized: [],
        error: "Drones I must come before Scout Drone Operation I",
      }),
    });
    renderPage();
    await screen.findByRole("link", { name: /F2 \| Boosh/ });

    fireEvent.click(screen.getAllByRole("button", { name: "Import from clipboard" })[0]);
    const dialog = screen.getByRole("dialog", { name: "Import plan from clipboard" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Bad" } });
    fireEvent.change(within(dialog).getByLabelText("Plan text"), { target: { value: "Scout Drone Operation 1\nDrones 1" } });

    expect(await within(dialog).findByText("Drones I must come before Scout Drone Operation I")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Create plan" })).toBeDisabled();
  });
});
