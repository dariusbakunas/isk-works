import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { SkillCatalogue } from "../../../api/skills";
import { SkillsPage } from "../skills-page";
import { AURA, catalogue, characterDetail, MIRA, roster } from "./fixtures";

const NOW = new Date("2026-10-25T12:00:00Z");

function stubFetch(body: SkillCatalogue = catalogue()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const url = String(input);
    const payload = url.includes("/api/skills/catalogue")
      ? body
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

function renderPage(path = "/skills") {
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

describe("Skills › Catalogue", () => {
  test("shows the first character's skills by group with states and times", async () => {
    const fetchSpy = stubFetch();
    renderPage();

    const table = await screen.findByRole("table", { name: "Skills" });
    expect(screen.getByTestId("skills-character-header")).toHaveTextContent("112,822,873 SP");
    expect(screen.getByTestId("skills-character-header")).toHaveTextContent("25,000 unallocated");
    expect(fetchSpy.mock.calls.some(([input]) => String(input).includes(`characterId=${AURA}`))).toBe(true);

    const drones = screen.getByRole("button", { name: /Drones 2\/4/ });
    expect(drones).toBeInTheDocument();
    const interfacing = within(table).getByText("Drone Interfacing").closest("tr")!;
    expect(within(interfacing).getByText("Training")).toBeInTheDocument();
    expect(within(interfacing).getByText("13d 21h")).toBeInTheDocument();
    expect(within(interfacing).getByText("×5")).toBeInTheDocument();
    const leadership = table.querySelector<HTMLElement>('[data-row-key="3348"]')!;
    expect(within(leadership).getByText("3h 41m")).toBeInTheDocument();
    const scout = within(table).getByText("Scout Drone Operation").closest("tr")!;
    expect(within(scout).getByText("—")).toBeInTheDocument();
    expect(within(scout).getByText("Can inject")).toBeInTheDocument();
  });

  test("filters like the client and keeps the filter in the URL", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Skills" });

    fireEvent.click(screen.getByRole("button", { name: "Have prerequisites for" }));

    expect(location()).toContain("filter=havePrerequisites");
    expect(within(table).getByText("Scout Drone Operation")).toBeInTheDocument();
    expect(within(table).queryByText("Drone Interfacing")).not.toBeInTheDocument();
  });

  test("a group tile narrows the list and All groups resets it", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Skills" });

    fireEvent.click(screen.getByRole("button", { name: /Leadership 1\/1/ }));
    expect(location()).toContain("group=258");
    expect(within(table).queryByText("Drones")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "All groups" }));
    expect(location()).not.toContain("group=");
    expect(within(table).getByText("Drone Interfacing")).toBeInTheDocument();
  });

  test("the inspector shows SP per level, the prerequisite tree and what it unlocks", async () => {
    stubFetch();
    renderPage("/skills?skill=23566");

    const inspector = await screen.findByRole("complementary", { name: "EW Drone Interfacing" });
    expect(within(inspector).getByText("1,280,000 SP")).toBeInTheDocument();
    const tree = within(inspector).getByRole("tree", { name: "Prerequisites" });
    expect(within(tree).getByText("Drone Interfacing V").closest("li")).toHaveAttribute("data-met", "false");
    expect(within(tree).getByText("Drones V").closest("li")).toHaveAttribute("data-met", "true");

    fireEvent.click(within(inspector).getByRole("button", { name: "Close skill inspector" }));
    expect(location()).not.toContain("skill=");
  });

  test("required-for links open the skill that needs it", async () => {
    stubFetch();
    renderPage(`/skills?skill=3436`);
    const inspector = await screen.findByRole("complementary", { name: "Drones" });

    fireEvent.click(within(inspector).getByRole("button", { name: "Required for (2)" }));
    fireEvent.click(within(inspector).getByRole("button", { name: /^Scout Drone Operation needs I$/ }));

    expect(location()).toContain("skill=3437");
  });

  test("switching character updates the URL", async () => {
    stubFetch();
    renderPage();
    await screen.findByRole("table", { name: "Skills" });

    fireEvent.change(screen.getByRole("combobox", { name: "Character" }), { target: { value: MIRA } });

    expect(location()).toContain(`character=${MIRA}`);
  });

  test("explains an SDE without skill data instead of an empty grid", async () => {
    stubFetch(catalogue({ skillDataAvailable: false, groups: [], skills: [] }));
    renderPage();

    expect(await screen.findByText("No skill data available")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Skills" })).not.toBeInTheDocument();
  });

  test("a missing skills permission asks to reconnect but keeps the catalogue", async () => {
    const base = catalogue();
    stubFetch(
      catalogue({
        character: { ...base.character!, skillsScopeMissing: true, totalSp: null, unallocatedSp: null, rate: null },
        skills: base.skills.map((skill) => ({ ...skill, progress: null })),
      }),
    );
    renderPage();

    expect(await screen.findByText(/missing the skills permission/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Skills" })).toBeInTheDocument();
  });
});
