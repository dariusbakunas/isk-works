import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { MemoryRouter } from "react-router";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";
import { CalendarInspector } from "../calendar-inspector";

test("shows truthful Industry fields and uses EVE type and character imagery", () => {
  const milestone = CALENDAR_MILESTONE_FIXTURES.find((item) => item.kind === "industry")!;
  render(<CalendarInspector onClose={vi.fn()} selection={{ kind: "milestone", milestone }} timezone="eve" />);
  expect(screen.getByRole("heading", { name: milestone.title })).toBeInTheDocument();
  expect(screen.getByRole("img", { name: milestone.title })).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Character portrait" })).toBeInTheDocument();
  expect(screen.getByText("5 runs")).toBeInTheDocument();
  expect(screen.getByText(/Jita IV/)).toBeInTheDocument();
  expect(screen.queryByText(/estimated value/i)).not.toBeInTheDocument();
});

test("describes a Skill finish as the current queue projection", () => {
  const milestone = CALENDAR_MILESTONE_FIXTURES.find((item) => item.kind === "skill")!;
  render(<CalendarInspector onClose={vi.fn()} selection={{ kind: "milestone", milestone }} timezone="eve" />);
  expect(screen.getByText("Current queue projection")).toBeInTheDocument();
  expect(screen.getByText("Level V")).toBeInTheDocument();
  expect(screen.getByText("Queue position 1")).toBeInTheDocument();
  expect(screen.getByText("Drone Interfacing")).toBeInTheDocument();
});

test("summarizes a grouped day, drills into a row, returns Back, and closes", () => {
  const milestones = CALENDAR_MILESTONE_FIXTURES.filter((item) => item.occursAt.startsWith("2026-10-02"));
  const onClose = vi.fn();
  render(<CalendarInspector onClose={onClose} selection={{ kind: "day", dateKey: "2026-10-02", milestones }} timezone="eve" />);
  expect(screen.getByRole("heading", { name: "Friday, October 2, 2026" })).toBeInTheDocument();
  expect(screen.getByText("3 Industry · 2 Skill · 0 Planetary")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /Open Scimitar/ }));
  expect(screen.getByRole("heading", { name: "Scimitar" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Back to Friday, October 2, 2026" }));
  expect(screen.getByText("3 Industry · 2 Skill · 0 Planetary")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close calendar inspector" }));
  expect(onClose).toHaveBeenCalledOnce();
});

const planetaryCommon = {
  kind: "planetary" as const,
  connectionId: "11111111-1111-4111-8111-111111111111",
  eveCharacterId: 1815539307,
  characterName: "Valka",
  occursAt: "2026-10-03T06:23:50Z",
  planetId: 40050359,
  planetName: "EUU-4N II",
  planetType: "barren",
  solarSystemName: "EUU-4N",
};

test("shows an exact extractor expiry with what the planet extracts", () => {
  render(
    <MemoryRouter>
      <CalendarInspector
        onClose={vi.fn()}
        selection={{
          kind: "milestone",
          milestone: {
            ...planetaryCommon,
            id: "planetary:extractor:x:40050359:1",
            title: "EUU-4N II extractors",
            estimated: false,
            event: "extractorExpiry",
            extractorCount: 2,
            products: [{ typeId: 2272, name: "Heavy Metals" }],
          },
        }}
        timezone="eve"
      />
    </MemoryRouter>,
  );
  expect(screen.getByText("Extractor expiry")).toBeInTheDocument();
  expect(screen.getByText("Heavy Metals")).toBeInTheDocument();
  expect(screen.queryByText(/Projected from the colony/)).not.toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open Planetary" })).toHaveAttribute("href", "/planetary");
});

test("labels a factory input run-out as an estimate", () => {
  render(
    <MemoryRouter>
      <CalendarInspector
        onClose={vi.fn()}
        selection={{
          kind: "milestone",
          milestone: {
            ...planetaryCommon,
            id: "planetary:import:x:40050359:2398",
            title: "EUU-4N II out of Reactive Metals",
            estimated: true,
            event: "importDepleted",
            typeId: 2398,
            typeName: "Reactive Metals",
            qtyPerHour: "220",
          },
        }}
        timezone="eve"
      />
    </MemoryRouter>,
  );
  expect(screen.getByText("Factory input runs out (estimate)")).toBeInTheDocument();
  expect(screen.getByText(/Projected from the colony/)).toBeInTheDocument();
  expect(screen.getByText("220/h")).toBeInTheDocument();
});
