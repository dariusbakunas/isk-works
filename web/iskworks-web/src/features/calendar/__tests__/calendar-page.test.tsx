import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { beforeEach, expect, test, vi } from "vitest";
import { getCalendarMilestones } from "../../../api/calendar";
import { listCharacters } from "../../../api/characters";
import { CalendarPage } from "../calendar-page";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";

vi.mock("../../../api/calendar", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../../api/calendar")>();
  return { ...original, getCalendarMilestones: vi.fn() };
});
vi.mock("../../../api/characters", () => ({ listCharacters: vi.fn() }));

const getCalendar = vi.mocked(getCalendarMilestones);
const getCharacters = vi.mocked(listCharacters);

beforeEach(() => {
  getCalendar.mockReset().mockResolvedValue(CALENDAR_MILESTONE_FIXTURES);
  getCharacters.mockReset().mockResolvedValue([
    { connectionId: "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95", eveCharacterId: 90000001, characterName: "Alix Morgan" },
    { connectionId: "874c445a-67f4-4368-9028-b785770a2e52", eveCharacterId: 90000002, characterName: "Bryn Vale" },
  ] as Awaited<ReturnType<typeof listCharacters>>);
});

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}{location.search}</output>;
}

function renderPage(url = "/calendar?view=month&date=2026-10-02&type=all&tz=eve") {
  return render(<MemoryRouter initialEntries={[url]}><CalendarPage /><LocationProbe /></MemoryRouter>);
}

test("deduplicates the same annual range when Strict Mode replays effects", async () => {
  render(
    <StrictMode>
      <MemoryRouter initialEntries={["/calendar?view=year&date=2026-10-02&type=all&tz=eve"]}>
        <CalendarPage />
      </MemoryRouter>
    </StrictMode>,
  );
  await screen.findByRole("group", { name: "Calendar year 2026" });
  expect(getCalendar).toHaveBeenCalledOnce();
});

test("loads the restored absolute EVE month range and applies URL filters", async () => {
  renderPage("/calendar?view=month&date=2026-10-02&type=skill&character=874c445a-67f4-4368-9028-b785770a2e52&tz=eve");
  await waitFor(() => expect(getCalendar).toHaveBeenCalledOnce());
  expect(getCalendar).toHaveBeenCalledWith(new Date("2026-10-01T00:00:00Z"), new Date("2026-11-01T00:00:00Z"));
  expect(await screen.findByRole("button", { name: /Drones V, skill completion/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Future Sleipnir batch/ })).not.toBeInTheDocument();
});

test("pushes month navigation and replaces type, character, and timezone state", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Scimitar, Manufacturing/ });
  await user.click(screen.getByRole("button", { name: "Next month" }));
  expect(screen.getByTestId("location")).toHaveTextContent("date=2026-11-02");
  await user.click(screen.getByRole("button", { name: "Previous month" }));
  expect(screen.getByTestId("location")).toHaveTextContent("date=2026-10-02");
  await user.click(screen.getByRole("button", { name: "Skill milestones" }));
  expect(screen.getByTestId("location")).toHaveTextContent("type=skill");
  await user.click(screen.getByRole("button", { name: /Filter by character/ }));
  await user.click(screen.getByRole("menuitemcheckbox", { name: "Alix Morgan" }));
  expect(screen.getByTestId("location")).toHaveTextContent("character=0aa719e8-f94f-4ef1-8f47-cf649d3b2d95");
  await user.click(screen.getByRole("button", { name: /Filter by character/ }));
  await user.click(screen.getByRole("menuitemcheckbox", { name: "Bryn Vale" }));
  expect(screen.getByRole("button", { name: "Filter by character: 2 characters" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Local time" }));
  expect(screen.getByTestId("location")).toHaveTextContent("tz=local");
  await user.click(screen.getByRole("button", { name: "Today" }));
  const today = new Date();
  expect(screen.getByTestId("location")).toHaveTextContent(`date=${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`);
  await waitFor(() => expect(getCalendar.mock.calls.length).toBeGreaterThanOrEqual(3));
});

test("distinguishes loading and API failure", async () => {
  let reject!: (reason: Error) => void;
  getCalendar.mockReturnValue(new Promise((_, rejectPromise) => { reject = rejectPromise; }));
  renderPage("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  expect(screen.getByText("Loading calendar...")).toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Calendar week" })).not.toBeInTheDocument();
  reject(new Error("Calendar backend unavailable"));
  expect(await screen.findByText("Calendar backend unavailable")).toBeInTheDocument();
  expect(screen.queryByText("Loading calendar...")).not.toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Calendar week" })).not.toBeInTheDocument();
});

test("ignores a stale response after the visible month changes", async () => {
  let resolveOctober!: (rows: typeof CALENDAR_MILESTONE_FIXTURES) => void;
  getCalendar.mockReturnValueOnce(new Promise((resolve) => { resolveOctober = resolve; })).mockResolvedValueOnce([]);
  renderPage();
  fireEvent.click(screen.getByRole("button", { name: "Next month" }));
  await waitFor(() => expect(getCalendar).toHaveBeenCalledTimes(2));
  resolveOctober(CALENDAR_MILESTONE_FIXTURES);
  await waitFor(() => expect(screen.queryByRole("button", { name: /Future Sleipnir batch/ })).not.toBeInTheDocument());
});

test("ignores a stale Month response after switching to Week", async () => {
  let resolveMonth!: (rows: typeof CALENDAR_MILESTONE_FIXTURES) => void;
  getCalendar.mockReturnValueOnce(new Promise((resolve) => { resolveMonth = resolve; })).mockResolvedValueOnce([]);
  const user = userEvent.setup();
  renderPage();
  await user.click(screen.getByRole("button", { name: "Week view" }));
  await waitFor(() => expect(getCalendar).toHaveBeenCalledTimes(2));
  resolveMonth(CALENDAR_MILESTONE_FIXTURES);
  await waitFor(() => expect(screen.queryByRole("button", { name: /Scimitar, Manufacturing/ })).not.toBeInTheDocument());
});

test("clears the previous range while loading and does not present it or an empty conclusion after failure", async () => {
  const user = userEvent.setup();
  getCalendar.mockResolvedValueOnce(CALENDAR_MILESTONE_FIXTURES).mockRejectedValueOnce(new Error("November failed"));
  renderPage();
  await screen.findByRole("button", { name: /Scimitar, Manufacturing/ });
  await user.click(screen.getByRole("button", { name: "Next month" }));
  expect(await screen.findByText("November failed")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Scimitar/ })).not.toBeInTheDocument();
  expect(screen.queryByText("No calendar milestones yet")).not.toBeInTheDocument();
  expect(screen.queryByText("No milestones this day.")).not.toBeInTheDocument();
});

test("desktop date selection opens the day inspector and close restores the originating control", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Mining Director III/ });
  const dateButton = screen.getByRole("button", { name: "Show milestones for Thursday, October 15, 2026" });
  await user.click(dateButton);
  expect(screen.getByRole("button", { name: "Close calendar inspector" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Close calendar inspector" }));
  await waitFor(() => expect(dateButton).toHaveFocus());
});

test("month navigation reconciles the selected agenda day", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Scimitar, Manufacturing/ });
  await user.click(screen.getByRole("button", { name: "Next month" }));
  expect(screen.getByRole("region", { name: "Selected day agenda" })).toHaveTextContent("Monday, November 2, 2026");
});

test("provides mobile dot identity and a real day agenda that opens the same inspector", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Scimitar, Manufacturing/ });
  const cell = screen.getByRole("gridcell", { name: /Friday, October 2, 2026/ });
  expect(cell.querySelector(".iw-calendar-chip")).toHaveAccessibleName(/Scimitar/);
  await user.click(screen.getByRole("button", { name: "Show milestones for Friday, October 2, 2026" }));
  const agenda = screen.getByRole("region", { name: "Selected day agenda" });
  expect(agenda).toHaveTextContent("Scimitar");
  await user.click(within(agenda).getByRole("button", { name: "Open Scimitar" }));
  expect(screen.getByRole("heading", { name: "Scimitar" })).toBeInTheDocument();
});

test("requests a Monday-first week and preserves its date when switching views", async () => {
  const user = userEvent.setup();
  renderPage("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  await waitFor(() => expect(getCalendar).toHaveBeenCalledWith(new Date("2026-09-28T00:00:00Z"), new Date("2026-10-05T00:00:00Z")));
  expect(screen.getByRole("button", { name: "Week view" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.queryByRole("region", { name: "Selected day agenda" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Month view" }));
  expect(screen.getByTestId("location")).toHaveTextContent("view=month&date=2026-10-02");
  expect(screen.getByTestId("location")).not.toHaveTextContent("month=");
});

test("month date selection becomes the week anchor", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Mining Director III/ });
  await user.click(screen.getByRole("button", { name: "Show milestones for Thursday, October 15, 2026" }));
  expect(screen.getByTestId("location")).toHaveTextContent("date=2026-10-15");
  await user.click(screen.getByRole("button", { name: "Close calendar inspector" }));
  await user.click(screen.getByRole("button", { name: "Week view" }));
  expect(screen.getByRole("region", { name: "Monday, October 12, 2026" })).toBeInTheDocument();
});

test("week navigation moves seven days and milestone focus returns after close", async () => {
  const user = userEvent.setup();
  renderPage("/calendar?view=week&date=2026-10-02&type=all&tz=eve");
  const milestone = await screen.findByRole("button", { name: /Scimitar, Manufacturing/ });
  await user.click(milestone);
  await user.click(screen.getByRole("button", { name: "Close calendar inspector" }));
  await waitFor(() => expect(milestone).toHaveFocus());
  await user.click(screen.getByRole("button", { name: "Next week" }));
  expect(screen.getByTestId("location")).toHaveTextContent("date=2026-10-09");
});

test("requests a full EVE year and exposes canonical Year navigation", async () => {
  const user = userEvent.setup();
  renderPage("/calendar?view=year&date=2028-02-29&type=all&tz=eve");
  await waitFor(() => expect(getCalendar).toHaveBeenCalledWith(new Date("2028-01-01T00:00:00Z"), new Date("2029-01-01T00:00:00Z")));
  expect(screen.getByRole("button", { name: "Year view" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("heading", { name: "2028" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Close calendar inspector" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Previous year" }));
  expect(screen.getByTestId("location")).toHaveTextContent("view=year&date=2027-02-28");
  await user.click(screen.getByRole("button", { name: "Next year" }));
  expect(screen.getByTestId("location")).toHaveTextContent("view=year&date=2028-02-28");
});

test("Year month selection preserves and clamps the anchor day", async () => {
  const user = userEvent.setup();
  renderPage("/calendar?view=year&date=2027-01-31&type=all&tz=eve");
  await screen.findByRole("button", { name: /^February 2027/ });
  await user.click(screen.getByRole("button", { name: /^February 2027/ }));
  expect(screen.getByTestId("location")).toHaveTextContent("view=month&date=2027-02-28");
});

test("Year filters update summaries without refetching", async () => {
  const user = userEvent.setup();
  renderPage("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  await screen.findByRole("button", { name: /^October 2026, 5 Industry, 3 Skill/ });
  expect(getCalendar).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "Skill milestones" }));
  expect(screen.getByRole("button", { name: /^October 2026, 0 Industry, 3 Skill/ })).toBeInTheDocument();
  expect(getCalendar).toHaveBeenCalledOnce();
});

test("Year stays hidden during loading and failure", async () => {
  let reject!: (reason: Error) => void;
  getCalendar.mockReturnValue(new Promise((_, rejectPromise) => { reject = rejectPromise; }));
  renderPage("/calendar?view=year&date=2026-10-02&type=all&tz=eve");
  expect(screen.queryByRole("group", { name: "Calendar year 2026" })).not.toBeInTheDocument();
  reject(new Error("Year unavailable"));
  expect(await screen.findByText("Year unavailable")).toBeInTheDocument();
  expect(screen.queryByRole("group", { name: "Calendar year 2026" })).not.toBeInTheDocument();
});

test("ignores a stale Month response after switching to Year", async () => {
  let resolveMonth!: (rows: typeof CALENDAR_MILESTONE_FIXTURES) => void;
  getCalendar.mockReturnValueOnce(new Promise((resolve) => { resolveMonth = resolve; })).mockResolvedValueOnce([]);
  const user = userEvent.setup();
  renderPage();
  await user.click(screen.getByRole("button", { name: "Year view" }));
  await waitFor(() => expect(getCalendar).toHaveBeenCalledTimes(2));
  resolveMonth(CALENDAR_MILESTONE_FIXTURES);
  await waitFor(() => expect(screen.getByRole("button", { name: /^October 2026, 0 Industry, 0 Skill/ })).toBeInTheDocument());
});
