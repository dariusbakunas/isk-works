import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";
import { CalendarWeekView } from "../calendar-week";

const weekRows = CALENDAR_MILESTONE_FIXTURES.filter((row) => row.occursAt >= "2026-09-28" && row.occursAt < "2026-10-05");
const now = new Date("2026-10-03T12:00:00Z");
const props = { anchor: "2026-10-02", milestones: weekRows, now, onSelectMilestone: vi.fn(), timezone: "eve" as const };

test("renders Monday through Sunday with cross-month full-date accessible names", () => {
  const { container } = render(<CalendarWeekView {...props} />);
  expect(screen.getAllByRole("region").map((region) => region.getAttribute("aria-label"))).toEqual([
    "Monday, September 28, 2026", "Tuesday, September 29, 2026", "Wednesday, September 30, 2026",
    "Thursday, October 1, 2026", "Friday, October 2, 2026", "Saturday, October 3, 2026", "Sunday, October 4, 2026",
  ]);
  expect(screen.getByText("Monday, September 28, 2026")).toHaveClass("iw-calendar-week-day-full-label");
  expect(container.querySelector(".iw-calendar-week-grid")).toHaveClass("iw-calendar-week-scroll-area");
});

test("sorts every day chronologically and renders quiet empty-day markers", () => {
  render(<CalendarWeekView {...props} milestones={[...weekRows].reverse()} />);
  const friday = screen.getByRole("region", { name: "Friday, October 2, 2026" });
  expect(within(friday).getAllByRole("button").map((button) => button.textContent)).toEqual([
    expect.stringContaining("Scimitar"), expect.stringContaining("Drones V"),
    expect.stringContaining("An extremely long production"), expect.stringContaining("Drone Interfacing IV"),
    expect.stringContaining("Vagabond"),
  ]);
  expect(screen.getByRole("region", { name: "Monday, September 28, 2026" })).toHaveTextContent("—");
});

test("renders Manufacturing Reaction Skill past Today and selected identities", () => {
  const reaction = { ...weekRows.find((row) => row.kind === "industry")!, activity: "reaction" as const };
  const skill = weekRows.find((row) => row.kind === "skill")!;
  render(<CalendarWeekView {...props} milestones={[reaction, skill]} selectedMilestoneId={skill.id} />);
  expect(screen.getByRole("button", { name: /Reaction/ })).toHaveAttribute("data-past", "true");
  expect(screen.getByRole("button", { name: /skill completion/i })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("region", { name: "Saturday, October 3, 2026" })).toHaveAttribute("data-today", "true");
});

test("dims milestones completed earlier today but not later today", () => {
  const seed = weekRows.find((row) => row.kind === "industry")!;
  const completed = { ...seed, id: `${seed.id}:completed`, occursAt: "2026-10-03T08:00:00Z", title: "Completed today" };
  const upcoming = { ...seed, id: `${seed.id}:upcoming`, occursAt: "2026-10-03T13:00:00Z", title: "Upcoming today" };
  render(<CalendarWeekView {...props} milestones={[completed, upcoming]} />);
  expect(screen.getByRole("button", { name: /Completed today/ })).toHaveAttribute("data-past", "true");
  expect(screen.getByRole("button", { name: /Upcoming today/ })).toHaveAttribute("data-past", "false");
});

test("selects a milestone with a stable week trigger id", () => {
  const onSelectMilestone = vi.fn();
  const selected = weekRows[0];
  render(<CalendarWeekView {...props} onSelectMilestone={onSelectMilestone} />);
  fireEvent.click(screen.getByRole("button", { name: new RegExp(selected.title) }));
  expect(onSelectMilestone).toHaveBeenCalledWith(selected, `week-milestone-${selected.id}`);
});

test("distinguishes source-empty from filtered-empty once for the whole week", () => {
  const { rerender } = render(<CalendarWeekView {...props} hasCalendarData={false} milestones={[]} />);
  expect(screen.getAllByText("No calendar milestones yet")).toHaveLength(1);
  rerender(<CalendarWeekView {...props} hasCalendarData milestones={[]} />);
  expect(screen.getAllByText("No milestones match these filters")).toHaveLength(1);
});

test("exposes the same seven dates without duplicating milestone buttons", () => {
  render(<CalendarWeekView {...props} />);
  expect(screen.getAllByRole("region")).toHaveLength(7);
  expect(screen.getAllByRole("button")).toHaveLength(weekRows.length);
});
