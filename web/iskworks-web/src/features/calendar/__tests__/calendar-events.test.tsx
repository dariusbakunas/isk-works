import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, test, vi } from "vitest";
import { getCalendarMilestones } from "../../../api/calendar";
import { listCharacters } from "../../../api/characters";
import { beginAuthorization } from "../../../api/esi";
import { eventPlainText, formatEventDuration, milestoneConnectionIds } from "../calendar-events";
import { CalendarInspector } from "../calendar-inspector";
import { CalendarPage } from "../calendar-page";
import { presentCalendarMilestone } from "../calendar-presentation";
import { parseCalendarUrlState } from "../calendar-url-state";
import { CalendarWeekView } from "../calendar-week";
import { CalendarYearView } from "../calendar-year";
import { CALENDAR_MILESTONE_FIXTURES, calendarEvent } from "./fixtures";

vi.mock("../../../api/calendar", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../../api/calendar")>();
  return { ...original, getCalendarMilestones: vi.fn() };
});
vi.mock("../../../api/characters", () => ({ listCharacters: vi.fn() }));
vi.mock("../../../api/esi", () => ({ beginAuthorization: vi.fn() }));

const getCalendar = vi.mocked(getCalendarMilestones);
const getCharacters = vi.mocked(listCharacters);
const authorize = vi.mocked(beginAuthorization);
const ALIX = "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95";
const BRYN = "874c445a-67f4-4368-9028-b785770a2e52";
const CARA = "5d1c5b8e-2f43-4b6e-9a51-0c7e2f9a1b23";
const opEvent = calendarEvent(42, "2026-10-02T19:00:00Z", "Moon mining op");

beforeEach(() => {
  window.localStorage.clear();
  getCalendar.mockReset().mockResolvedValue([...CALENDAR_MILESTONE_FIXTURES, opEvent]);
  getCharacters.mockReset().mockResolvedValue([
    { connectionId: ALIX, eveCharacterId: 90000001, characterName: "Alix Morgan", calendarScopeMissing: false },
    { connectionId: BRYN, eveCharacterId: 90000002, characterName: "Bryn Vale", calendarScopeMissing: false },
    { connectionId: CARA, eveCharacterId: 90000003, characterName: "Cara Lind", calendarScopeMissing: true },
  ] as Awaited<ReturnType<typeof listCharacters>>);
  authorize.mockReset();
});

test("event text becomes plain text with line breaks and decoded entities", () => {
  expect(eventPlainText(opEvent.text)).toBe("Form up\nin Jita & bring ore");
  expect(eventPlainText("<script>alert(1)</script><img src=x onerror=alert(2)>Hi")).toBe("Hi");
  expect(eventPlainText("")).toBe("");
});

test("formats event durations and lists every viewer's connection", () => {
  expect(formatEventDuration(0)).toBeNull();
  expect(formatEventDuration(45)).toBe("45 min");
  expect(formatEventDuration(120)).toBe("2 h");
  expect(formatEventDuration(90)).toBe("1 h 30 min");
  expect(milestoneConnectionIds(opEvent)).toEqual([ALIX, BRYN]);
});

test("events have their own icon, label and tone, and a type filter", () => {
  expect(presentCalendarMilestone(opEvent)).toMatchObject({
    kindLabel: "Event",
    activityLabel: "In-game event",
    tone: "event",
    accessibleLabel: "Moon mining op, in-game event for Alix Morgan, Bryn Vale",
  });
  expect(parseCalendarUrlState(new URLSearchParams("type=event")).type).toBe("event");
});

test("the inspector shows owner, timing, each response and the text, never as HTML", () => {
  const hostile = calendarEvent(43, "2026-10-02T19:00:00Z", "Op", {
    text: "<b>Bold</b><img src=x onerror=\"alert(1)\"><script>window.pwned = true</script>",
  });
  render(<CalendarInspector onClose={vi.fn()} selection={{ kind: "milestone", milestone: hostile }} timezone="eve" />);
  expect(screen.getByText("Event")).toBeInTheDocument();
  expect(screen.getByText("Morgan Industries · Corporation")).toBeInTheDocument();
  expect(screen.getByText("7:00 PM UTC")).toBeInTheDocument();
  expect(screen.getByText("8:30 PM UTC")).toBeInTheDocument();
  expect(screen.getByText("1 h 30 min")).toBeInTheDocument();
  expect(screen.getByText("Important")).toBeInTheDocument();
  const responses = within(screen.getByRole("region", { name: "Character responses" }));
  expect(responses.getByText("Alix Morgan").closest("li")).toHaveTextContent("Accepted");
  expect(responses.getByText("Bryn Vale").closest("li")).toHaveTextContent("Tentative");
  const description = screen.getByRole("region", { name: "Event description" });
  expect(description).toHaveTextContent("Bold");
  expect(description.querySelector("b, img, script")).toBeNull();
  expect((window as { pwned?: boolean }).pwned).toBeUndefined();
});

test("the week view draws an event with a duration as a timed block", () => {
  const point = calendarEvent(44, "2026-10-03T10:00:00Z", "Corp meeting", { durationMinutes: 0 });
  render(<CalendarWeekView anchor="2026-10-02" milestones={[opEvent, point]} now={new Date("2026-10-01T00:00:00Z")} onSelectMilestone={vi.fn()} timezone="eve" />);
  const block = screen.getByRole("button", { name: /Moon mining op, In-game event for Alix Morgan \+1, 7:00 PM UTC – 8:30 PM UTC/ });
  expect(block).toHaveClass("iw-calendar-week-card-event", "iw-calendar-week-card-block");
  expect(block).toHaveAttribute("data-duration-minutes", "90");
  const pointCard = screen.getByRole("button", { name: /Corp meeting/ });
  expect(pointCard).not.toHaveClass("iw-calendar-week-card-block");
});

test("the year view counts and marks events", () => {
  render(<CalendarYearView anchor="2026-10-02" milestones={[opEvent]} now={new Date("2026-10-01T00:00:00Z")} onSelectMonth={vi.fn()} timezone="eve" />);
  expect(screen.getByRole("button", { name: "October 2026, 0 Industry, 0 Skill, 0 Planetary, 1 Event" })).toBeInTheDocument();
  expect(screen.getByLabelText("In-game events")).toHaveTextContent("E");
});

test("an event shared by two characters shows once and matches either character's filter", async () => {
  render(<MemoryRouter initialEntries={[`/calendar?view=month&date=2026-10-02&type=event&character=${BRYN}&tz=eve`]}><CalendarPage /></MemoryRouter>);
  expect(await screen.findAllByRole("button", { name: /Moon mining op, in-game event/ })).toHaveLength(1);
  expect(screen.queryByRole("button", { name: /Scimitar/ })).not.toBeInTheDocument();
});

test("characters without the calendar scope get a reconnect hint", async () => {
  authorize.mockResolvedValue({ authorizationUrl: "", fixtureMode: true, connection: null, requestedScopes: [] });
  const reload = vi.fn();
  vi.stubGlobal("location", { ...window.location, reload });
  try {
    render(<MemoryRouter initialEntries={["/calendar?view=month&date=2026-10-02&type=all&tz=eve"]}><CalendarPage /></MemoryRouter>);
    const hint = await screen.findByRole("note", { name: "Calendar access" });
    expect(hint).toHaveTextContent("Calendar events are missing for Cara Lind.");
    expect(hint).toHaveTextContent("log in as Cara Lind on EVE's page");
    await userEvent.click(within(hint).getByRole("button", { name: "Grant calendar access" }));
    await waitFor(() => expect(authorize).toHaveBeenCalledOnce());
    await userEvent.click(screen.getByRole("button", { name: /Filter by character/ }));
    expect(screen.getByRole("menuitemcheckbox", { name: "Cara Lind, no calendar access" })).toBeInTheDocument();
  } finally {
    vi.unstubAllGlobals();
  }
});

test("the reconnect hint stays out of other milestone types", async () => {
  render(<MemoryRouter initialEntries={["/calendar?view=month&date=2026-10-02&type=skill&tz=eve"]}><CalendarPage /></MemoryRouter>);
  await waitFor(() => expect(getCalendar).toHaveBeenCalledOnce());
  await screen.findAllByRole("button", { name: /Drones V/ });
  expect(screen.queryByRole("note")).not.toBeInTheDocument();
});
