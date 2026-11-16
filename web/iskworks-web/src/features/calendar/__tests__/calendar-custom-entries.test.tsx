import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, test, vi } from "vitest";
import {
  createCalendarEntry,
  deleteCalendarEntry,
  getCalendarMilestones,
  updateCalendarEntry,
  type CalendarCustomMilestone,
} from "../../../api/calendar";
import { listCharacters } from "../../../api/characters";
import { CalendarPage } from "../calendar-page";
import { CALENDAR_MILESTONE_FIXTURES } from "./fixtures";

vi.mock("../../../api/calendar", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../../api/calendar")>();
  return {
    ...original,
    getCalendarMilestones: vi.fn(),
    createCalendarEntry: vi.fn(),
    updateCalendarEntry: vi.fn(),
    deleteCalendarEntry: vi.fn(),
  };
});
vi.mock("../../../api/characters", () => ({ listCharacters: vi.fn() }));

const ALIX = "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95";
const BRYN = "874c445a-67f4-4368-9028-b785770a2e52";

function custom(entryId: string, title: string, occursAt: string, character?: { id: string; eveId: number; name: string }): CalendarCustomMilestone {
  return {
    kind: "custom",
    id: `custom:${entryId}`,
    entryId,
    title,
    occursAt,
    timeZone: "UTC",
    description: "",
    connectionId: character?.id ?? null,
    eveCharacterId: character?.eveId ?? null,
    characterName: character?.name ?? null,
  };
}

const NOTE = custom("e1", "Workspace note", "2026-10-06T12:00:00Z");
const BRYN_OP = custom("e2", "Bryn op", "2026-10-07T12:00:00Z", { id: BRYN, eveId: 90000002, name: "Bryn Vale" });

beforeEach(() => {
  window.localStorage.clear();
  vi.mocked(getCalendarMilestones).mockReset().mockResolvedValue([...CALENDAR_MILESTONE_FIXTURES, NOTE, BRYN_OP]);
  vi.mocked(createCalendarEntry).mockReset();
  vi.mocked(updateCalendarEntry).mockReset();
  vi.mocked(deleteCalendarEntry).mockReset().mockResolvedValue(undefined);
  vi.mocked(listCharacters).mockReset().mockResolvedValue([
    { connectionId: ALIX, eveCharacterId: 90000001, characterName: "Alix Morgan" },
    { connectionId: BRYN, eveCharacterId: 90000002, characterName: "Bryn Vale" },
  ] as Awaited<ReturnType<typeof listCharacters>>);
});

function renderPage(url = "/calendar?view=month&date=2026-10-02&type=all&tz=eve") {
  return render(<MemoryRouter initialEntries={[url]}><CalendarPage /></MemoryRouter>);
}

test("adds an entry at a wall-clock time in the chosen zone, for a character", async () => {
  const user = userEvent.setup();
  const created = custom("e3", "Moon pull", "2026-10-09T22:00:00Z", { id: ALIX, eveId: 90000001, name: "Alix Morgan" });
  vi.mocked(createCalendarEntry).mockResolvedValue({ ...created, timeZone: "America/New_York", description: "Bring a Rorqual" });
  renderPage();
  await screen.findByRole("button", { name: /Workspace note, custom entry/ });

  await user.click(screen.getByRole("button", { name: "New entry" }));
  const dialog = screen.getByRole("dialog", { name: "New calendar entry" });
  // Starts on the selected day, in the zone the calendar shows.
  expect(within(dialog).getByLabelText("Date")).toHaveValue("2026-10-02");
  expect(within(dialog).getByLabelText("Time zone")).toHaveValue("UTC");
  expect(within(dialog).getByRole("button", { name: "Add entry" })).toBeDisabled();

  await user.type(within(dialog).getByLabelText("Title"), "  Moon pull ");
  await user.clear(within(dialog).getByLabelText("Date"));
  await user.type(within(dialog).getByLabelText("Date"), "2026-10-09");
  await user.clear(within(dialog).getByLabelText("Time"));
  await user.type(within(dialog).getByLabelText("Time"), "18:00");
  await user.clear(within(dialog).getByLabelText("Time zone"));
  await user.type(within(dialog).getByLabelText("Time zone"), "America/New_York");
  await user.selectOptions(within(dialog).getByLabelText("Character"), "Alix Morgan");
  await user.type(within(dialog).getByLabelText("Description"), "Bring a Rorqual");
  await user.click(within(dialog).getByRole("button", { name: "Add entry" }));

  expect(createCalendarEntry).toHaveBeenCalledWith({
    occursAt: "2026-10-09T22:00:00.000Z",
    timeZone: "America/New_York",
    title: "Moon pull",
    description: "Bring a Rorqual",
    connectionId: ALIX,
  });
  expect(screen.queryByRole("dialog", { name: "New calendar entry" })).not.toBeInTheDocument();
  // Shown at once, and opened in the inspector with its own zone's time.
  expect(screen.getAllByRole("button", { name: /Moon pull, custom entry for Alix Morgan/ }).length).toBeGreaterThan(0);
  expect(screen.getByRole("heading", { name: "Moon pull" })).toBeInTheDocument();
  expect(screen.getByText("America/New_York")).toBeInTheDocument();
  expect(screen.getByText("Friday, October 9, 2026, 18:00")).toBeInTheDocument();
  expect(screen.getByText("Bring a Rorqual")).toBeInTheDocument();
});

test("refuses an unknown time zone before saving", async () => {
  const user = userEvent.setup();
  renderPage();
  await screen.findByRole("button", { name: /Workspace note, custom entry/ });
  await user.click(screen.getByRole("button", { name: "New entry" }));
  const dialog = screen.getByRole("dialog", { name: "New calendar entry" });
  await user.type(within(dialog).getByLabelText("Title"), "Op");
  await user.clear(within(dialog).getByLabelText("Time zone"));
  await user.type(within(dialog).getByLabelText("Time zone"), "Mars/Olympus");
  expect(within(dialog).getByText("Pick a time zone from the list.")).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Add entry" })).toBeDisabled();
});

test("a character filter hides other characters' entries but never a workspace-wide one", async () => {
  renderPage(`/calendar?view=month&date=2026-10-02&type=all&character=${ALIX}&tz=eve`);
  expect(await screen.findByRole("button", { name: /Workspace note, custom entry$/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Bryn op/ })).not.toBeInTheDocument();
});

test("the Custom type filter shows only custom entries", async () => {
  renderPage("/calendar?view=month&date=2026-10-02&type=custom&tz=eve");
  expect(await screen.findByRole("button", { name: /Workspace note/ })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Bryn op/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Scimitar/ })).not.toBeInTheDocument();
});

test("edits and deletes an entry from the inspector", async () => {
  const user = userEvent.setup();
  vi.mocked(updateCalendarEntry).mockResolvedValue({ ...BRYN_OP, title: "Bryn op (moved)", occursAt: "2026-10-08T09:30:00Z", connectionId: null, eveCharacterId: null, characterName: null });
  renderPage();
  await user.click(await screen.findByRole("button", { name: /Bryn op, custom entry for Bryn Vale/ }));
  await user.click(screen.getByRole("button", { name: "Edit" }));

  const dialog = screen.getByRole("dialog", { name: "Edit entry" });
  expect(within(dialog).getByLabelText("Title")).toHaveValue("Bryn op");
  expect(within(dialog).getByLabelText("Date")).toHaveValue("2026-10-07");
  expect(within(dialog).getByLabelText("Time")).toHaveValue("12:00");
  expect(within(dialog).getByLabelText("Character")).toHaveValue(BRYN);
  await user.clear(within(dialog).getByLabelText("Title"));
  await user.type(within(dialog).getByLabelText("Title"), "Bryn op (moved)");
  await user.clear(within(dialog).getByLabelText("Date"));
  await user.type(within(dialog).getByLabelText("Date"), "2026-10-08");
  await user.clear(within(dialog).getByLabelText("Time"));
  await user.type(within(dialog).getByLabelText("Time"), "09:30");
  await user.selectOptions(within(dialog).getByLabelText("Character"), "No character");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  expect(updateCalendarEntry).toHaveBeenCalledWith("e2", {
    occursAt: "2026-10-08T09:30:00.000Z",
    timeZone: "UTC",
    title: "Bryn op (moved)",
    description: "",
    connectionId: null,
  });
  expect(screen.getAllByRole("button", { name: /Bryn op \(moved\), custom entry$/ }).length).toBeGreaterThan(0);

  await user.click(screen.getByRole("button", { name: "Delete" }));
  await user.click(screen.getByRole("button", { name: "Delete entry" }));
  await waitFor(() => expect(deleteCalendarEntry).toHaveBeenCalledWith("e2"));
  await waitFor(() => expect(screen.queryByRole("button", { name: /Bryn op/ })).not.toBeInTheDocument());
});
