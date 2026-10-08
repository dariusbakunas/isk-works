import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";
import { CalendarMonthView } from "../calendar-month";

const october = { year: 2026, month: 10 };
const now = new Date("2026-10-15T12:00:00Z");

test("renders a Monday-first five-row month with stable 70px desktop cells", () => {
  render(<CalendarMonthView milestones={[]} month={{ year: 2026, month: 2 }} now={now} onSelectDay={vi.fn()} onSelectMilestone={vi.fn()} selectedDateKey="2026-02-01" timezone="eve" />);
  expect(screen.getAllByRole("columnheader").map((heading) => heading.textContent)).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
  expect(screen.getAllByRole("gridcell")).toHaveLength(35);
  expect(screen.getByRole("grid")).toHaveClass("iw-calendar-month-grid");
});

test("renders a complete six-row month", () => {
  render(<CalendarMonthView milestones={[]} month={{ year: 2026, month: 3 }} now={now} onSelectDay={vi.fn()} onSelectMilestone={vi.fn()} selectedDateKey="2026-03-01" timezone="eve" />);
  expect(screen.getAllByRole("gridcell")).toHaveLength(42);
});

test("marks Today and fades milestones in the past", () => {
  render(<CalendarMonthView milestones={CALENDAR_MILESTONE_FIXTURES} month={october} now={now} onSelectDay={vi.fn()} onSelectMilestone={vi.fn()} selectedDateKey="2026-10-02" timezone="eve" />);
  expect(screen.getByRole("gridcell", { name: /Thursday, October 15, 2026/ })).toHaveAttribute("data-today", "true");
  expect(screen.getByRole("button", { name: /UTC midnight-crossing Muninn batch/ })).toHaveAttribute("data-past", "true");
  expect(screen.getByRole("button", { name: /Future Sleipnir batch/ })).toHaveAttribute("data-past", "false");
});

test("shows at most three semantic chips and a grouped overflow action", () => {
  const onSelectDay = vi.fn();
  render(<CalendarMonthView milestones={CALENDAR_MILESTONE_FIXTURES} month={october} now={now} onSelectDay={onSelectDay} onSelectMilestone={vi.fn()} selectedDateKey="2026-10-02" timezone="eve" />);
  const cell = screen.getByRole("gridcell", { name: /Friday, October 2, 2026/ });
  expect(cell.querySelectorAll(".iw-calendar-chip")).toHaveLength(3);
  fireEvent.click(within(cell).getByRole("button", { name: "Show 2 more milestones for Friday, October 2, 2026" }));
  expect(onSelectDay).toHaveBeenCalledWith("2026-10-02", expect.arrayContaining([
    expect.objectContaining({ kind: "industry" }),
    expect.objectContaining({ kind: "skill" }),
  ]), "overflow");
});

test("selects an individual milestone and exposes its selected outline", () => {
  const onSelectMilestone = vi.fn();
  const selected = CALENDAR_MILESTONE_FIXTURES[1];
  const { rerender } = render(<CalendarMonthView milestones={CALENDAR_MILESTONE_FIXTURES} month={october} now={now} onSelectDay={vi.fn()} onSelectMilestone={onSelectMilestone} selectedDateKey="2026-10-02" timezone="eve" />);
  const chip = screen.getByRole("button", { name: new RegExp(selected.title) });
  fireEvent.click(chip);
  expect(onSelectMilestone).toHaveBeenCalledWith(selected, `milestone-${selected.id}`);
  rerender(<CalendarMonthView milestones={CALENDAR_MILESTONE_FIXTURES} month={october} now={now} onSelectDay={vi.fn()} onSelectMilestone={onSelectMilestone} selectedDateKey="2026-10-02" selectedMilestoneId={selected.id} timezone="eve" />);
  expect(screen.getByRole("button", { name: new RegExp(selected.title) })).toHaveAttribute("aria-pressed", "true");
});

test("distinguishes no synchronized milestones from a filtered-empty result", () => {
  const { rerender } = render(<CalendarMonthView hasCalendarData={false} milestones={[]} month={october} now={now} onSelectDay={vi.fn()} onSelectMilestone={vi.fn()} selectedDateKey="2026-10-02" timezone="eve" />);
  expect(screen.getByText("No calendar milestones yet")).toBeInTheDocument();
  expect(screen.getAllByRole("gridcell")).toHaveLength(35);
  rerender(<CalendarMonthView hasCalendarData milestones={[]} month={october} now={now} onSelectDay={vi.fn()} onSelectMilestone={vi.fn()} selectedDateKey="2026-10-02" timezone="eve" />);
  expect(screen.getByText("No milestones match these filters")).toBeInTheDocument();
});
