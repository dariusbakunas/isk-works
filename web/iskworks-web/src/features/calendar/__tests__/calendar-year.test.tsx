import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";
import { CalendarYearView } from "../calendar-year";

const industry = CALENDAR_MILESTONE_FIXTURES.find((row) => row.kind === "industry")!;
const skill = CALENDAR_MILESTONE_FIXTURES.find((row) => row.kind === "skill")!;
const milestones = [
  { ...industry, id: `${industry.id}:jan`, occursAt: "2026-01-05T10:00:00Z", title: "January industry" },
  { ...industry, id: `${industry.id}:oct`, occursAt: "2026-10-02T10:00:00Z", title: "October industry" },
  { ...skill, id: `${skill.id}:oct`, occursAt: "2026-10-02T11:00:00Z", title: "October skill" },
];
const props = {
  anchor: "2026-10-02",
  milestones,
  now: new Date("2026-10-03T12:00:00Z"),
  onSelectMonth: vi.fn(),
  timezone: "eve" as const,
};

test("renders twelve accessible month buttons in calendar order", () => {
  render(<CalendarYearView {...props} />);
  const buttons = screen.getAllByRole("button");
  expect(buttons).toHaveLength(12);
  expect(buttons[0]).toHaveAccessibleName("January 2026, 1 Industry, 0 Skill, 0 Planetary");
  expect(buttons[9]).toHaveAccessibleName("October 2026, 1 Industry, 1 Skill, 0 Planetary");
  expect(buttons[11]).toHaveAccessibleName("December 2026, 0 Industry, 0 Skill, 0 Planetary");
});

test("renders Monday-first real month days with mixed type markers", () => {
  render(<CalendarYearView {...props} />);
  const january = screen.getByRole("button", { name: /^January 2026/ });
  expect(within(january).getAllByText(/^(Mon|Tue|Wed|Thu|Fri|Sat|Sun)$/).map((node) => node.textContent)).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
  expect(january.querySelectorAll("[data-calendar-year-day]")).toHaveLength(31);
  const octoberSecond = screen.getByTestId("year-day-2026-10-02");
  expect(within(octoberSecond).getByLabelText("Industry milestones")).toBeInTheDocument();
  expect(within(octoberSecond).getByLabelText("Skill milestones")).toBeInTheDocument();
  expect(screen.getAllByRole("button")).toHaveLength(12);
});

test("marks Today and past dates", () => {
  render(<CalendarYearView {...props} />);
  expect(screen.getByTestId("year-day-2026-10-03")).toHaveAttribute("data-today", "true");
  expect(screen.getByTestId("year-day-2026-10-02")).toHaveAttribute("data-past", "true");
  expect(screen.getByTestId("year-day-2026-10-04")).toHaveAttribute("data-past", "false");
});

test("exposes stable presentation hooks without changing month-button semantics", () => {
  render(<CalendarYearView {...props} />);
  const october = screen.getByRole("button", { name: /^October 2026/ });
  expect(october).toHaveClass("iw-calendar-year-month");
  expect(october).toHaveAttribute("data-calendar-year-month", "2026-10");
  expect(october).not.toHaveAttribute("tabindex", "-1");

  const mixedDay = screen.getByTestId("year-day-2026-10-02");
  expect(mixedDay).toHaveClass("iw-calendar-year-day");
  expect(mixedDay).toHaveAttribute("data-milestone-kinds", "industry skill");
  expect(within(mixedDay).getByLabelText("Industry milestones")).toHaveAttribute("data-kind", "industry");
  expect(within(mixedDay).getByLabelText("Skill milestones")).toHaveAttribute("data-kind", "skill");
  expect(screen.getByTestId("year-day-2026-10-03")).toHaveAttribute("data-today", "true");
  expect(mixedDay).toHaveAttribute("data-past", "true");
});

test("selects the exact month", () => {
  const onSelectMonth = vi.fn();
  render(<CalendarYearView {...props} onSelectMonth={onSelectMonth} />);
  fireEvent.click(screen.getByRole("button", { name: /^November 2026/ }));
  expect(onSelectMonth).toHaveBeenCalledWith({ year: 2026, month: 11 });
});

test("distinguishes source-empty from filtered-empty once", () => {
  const { rerender } = render(<CalendarYearView {...props} hasCalendarData={false} milestones={[]} />);
  expect(screen.getAllByText("No calendar milestones yet")).toHaveLength(1);
  rerender(<CalendarYearView {...props} hasCalendarData milestones={[]} />);
  expect(screen.getAllByText("No milestones match these filters")).toHaveLength(1);
});
