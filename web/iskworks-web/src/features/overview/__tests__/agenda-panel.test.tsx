import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AgendaPanel, agendaGroups, inLabel } from "../agenda-panel";
import { OverviewDataProvider } from "../overview-data";
import { customMilestone, fetchByUrl, hoursFromNow, NOW, skillMilestone } from "./overview-fixtures";

const now = NOW.getTime(); // 2026-11-17 12:00 EVE

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function renderPanel() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <AgendaPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

describe("inLabel", () => {
  test("counts down in minutes, then hours and minutes", () => {
    expect(inLabel(now + 42 * 60_000, now)).toBe("in 42m");
    expect(inLabel(now + (2 * 60 + 14) * 60_000, now)).toBe("in 2h 14m");
    expect(inLabel(now + 31 * 3_600_000, now)).toBe("in 31h");
    expect(inLabel(now - 60_000, now)).toBe("now");
  });
});

describe("agendaGroups", () => {
  test("groups by EVE day as Today, Tomorrow, then the date, soonest first", () => {
    const milestones = [
      skillMilestone({ id: "c", occursAt: hoursFromNow(40) }), // Thu 19 Nov 04:00
      skillMilestone({ id: "a", occursAt: hoursFromNow(1) }),
      skillMilestone({ id: "b", occursAt: hoursFromNow(13) }), // Wed 18 Nov 01:00
    ];
    const { groups, hidden } = agendaGroups(milestones, now, "eve", 8);

    expect(groups.map((group) => [group.label, group.rows.map((row) => row.id)])).toEqual([
      ["Today", ["a"]],
      ["Tomorrow", ["b"]],
      ["Thu 19 Nov", ["c"]],
    ]);
    expect(hidden).toBe(0);
  });

  test("past milestones drop out, and rows past the limit are counted", () => {
    const milestones = [
      skillMilestone({ id: "past", occursAt: hoursFromNow(-1) }),
      ...Array.from({ length: 10 }, (_, index) => skillMilestone({ id: `m${index}`, occursAt: hoursFromNow(index + 1) })),
    ];
    const { groups, hidden } = agendaGroups(milestones, now, "eve", 8);

    expect(groups.flatMap((group) => group.rows.map((row) => row.id))).toEqual(["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7"]);
    expect(hidden).toBe(2);
  });
});

describe("AgendaPanel", () => {
  test("a row shows the countdown, title and character; a custom entry with no character reads Workspace", async () => {
    fetchByUrl({
      calendar: [skillMilestone({ occursAt: hoursFromNow(5) }), customMilestone({ occursAt: hoursFromNow(22) })],
    });
    renderPanel();

    const region = await screen.findByRole("region", { name: "Next 48 h" });
    const skill = (await within(region).findByText("Caldari Industrial V finishes")).closest("a")!;
    expect(within(skill).getByText("in 5h")).toHaveAttribute("title", "Nov 17, 17:00 EVE");
    expect(within(skill).getByText("Kira Vantor")).toBeInTheDocument();
    expect(skill).toHaveAttribute("href", "/calendar?view=week&date=2026-11-17&tz=eve");
    const custom = within(region).getByText("Buy fuel blocks").closest("a")!;
    expect(within(custom).getByText("Workspace")).toBeInTheDocument();
    expect(within(region).getByRole("link", { name: "View all Next 48 h" })).toHaveAttribute("href", "/calendar?view=week");
  });

  test("more than eight upcoming milestones end with a +N more link", async () => {
    fetchByUrl({
      calendar: Array.from({ length: 11 }, (_, index) => skillMilestone({ id: `m${index}`, occursAt: hoursFromNow(index + 1) })),
    });
    renderPanel();

    expect(await screen.findByRole("link", { name: "+3 more" })).toHaveAttribute("href", "/calendar?view=week");
  });

  test("nothing scheduled", async () => {
    fetchByUrl({ calendar: [] });
    renderPanel();

    expect(await screen.findByText("Nothing scheduled in the next 48 hours.")).toBeInTheDocument();
  });
});
