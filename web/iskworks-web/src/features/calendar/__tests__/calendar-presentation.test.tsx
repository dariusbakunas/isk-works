import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";
import { CalendarMilestoneLabel, presentCalendarMilestone } from "../calendar-presentation";

test("classifies Industry activity and exposes complete accessible copy", () => {
  const milestone = CALENDAR_MILESTONE_FIXTURES.find((item) => item.kind === "industry")!;
  const presented = presentCalendarMilestone(milestone);
  expect(presented).toMatchObject({ kindLabel: "Industry", activityLabel: "Manufacturing", tone: "industry" });
  expect(presented.accessibleLabel).toContain("Past Hurricane batch");
  expect(presented.accessibleLabel).toContain("Alix Morgan");
});

test("classifies Skill milestones separately", () => {
  const milestone = CALENDAR_MILESTONE_FIXTURES.find((item) => item.kind === "skill")!;
  expect(presentCalendarMilestone(milestone)).toMatchObject({
    kindLabel: "Skill",
    activityLabel: "Skill completion",
    tone: "skill",
  });
});

test("truncates long visible labels while retaining the full title", () => {
  const milestone = CALENDAR_MILESTONE_FIXTURES.find((item) => item.title.startsWith("An extremely"))!;
  render(<CalendarMilestoneLabel milestone={milestone} />);
  const label = screen.getByText(milestone.title);
  expect(label).toHaveClass("truncate");
  expect(label).toHaveAttribute("title", milestone.title);
  expect(label.parentElement).toHaveAttribute("aria-label", expect.stringContaining(milestone.characterName));
});
