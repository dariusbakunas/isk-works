import { ChevronLeft, ChevronRight, Plus, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { deleteCalendarEntry, getCalendarMilestones, type CalendarCustomMilestone, type CalendarMilestone } from "../../api/calendar";
import { listCharacters } from "../../api/characters";
import { beginAuthorization } from "../../api/esi";
import { ConfirmDialog, InlineAlert, LoadingState, PageHeader } from "../../components/primitives";
import { CalendarFilters, type CalendarCharacterOption } from "./calendar-filters";
import { dateKey, dateKeyForMonth, formatDateLabel, formatMonthLabel, formatWeekLabel, monthFromDateKey, monthRange, shiftCalendarDate, weekRange, yearRange } from "./calendar-dates";
import { CalendarEntryDialog, type CalendarEntryDraft } from "./calendar-entry-dialog";
import { browserTimeZone, EVE_TIME_ZONE, utcToWallClock } from "./calendar-entry-time";
import { CalendarInspector, type CalendarInspectorSelection } from "./calendar-inspector";
import { CalendarMonthView, milestonesForDay } from "./calendar-month";
import { matchesCharacterFilter, milestoneCharacterSummary } from "./calendar-events";
import { presentCalendarMilestone } from "./calendar-presentation";
import { CalendarScopeHint } from "./calendar-scope-hint";
import { readCalendarTimezone, writeCalendarTimezone } from "./calendar-timezone-preference";
import { parseCalendarUrlState, serializeCalendarUrlState, type CalendarUrlState } from "./calendar-url-state";
import { CalendarWeekView } from "./calendar-week";
import { CalendarYearView } from "./calendar-year";
import { apiMessage } from "../../api/api-error";

export function CalendarPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const state = useMemo(() => parseCalendarUrlState(searchParams, new Date(), readCalendarTimezone()), [searchParams]);
  const [milestones, setMilestones] = useState<CalendarMilestone[]>([]);
  const [characters, setCharacters] = useState<CalendarCharacterOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [selection, setSelection] = useState<CalendarInspectorSelection | null>(null);
  const [returnFocusSelector, setReturnFocusSelector] = useState<string>();
  const [entryDialog, setEntryDialog] = useState<{ draft: CalendarEntryDraft; entry?: CalendarCustomMilestone } | null>(null);
  const [deleting, setDeleting] = useState<CalendarCustomMilestone | null>(null);
  const requestSequence = useRef(0);
  const requestedRange = useRef("");
  const month = monthFromDateKey(state.date);
  const { from, to } = state.view === "week"
    ? weekRange(state.date, state.timezone)
    : state.view === "year"
      ? yearRange(state.date, state.timezone)
      : monthRange(month, state.timezone);

  useEffect(() => {
    let active = true;
    listCharacters().then((entries) => {
      if (active) {
        setCharacters(entries.map(({ connectionId, eveCharacterId, characterName, calendarScopeMissing }) => ({
          connectionId,
          eveCharacterId,
          characterName,
          calendarScopeMissing,
        })));
      }
    }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  // The range as timestamps: `from`/`to` are new Dates every render.
  const fromTime = from.getTime();
  const toTime = to.getTime();
  useEffect(() => {
    const from = new Date(fromTime);
    const to = new Date(toTime);
    const rangeKey = `${from.toISOString()}:${to.toISOString()}`;
    if (requestedRange.current === rangeKey) return;
    requestedRange.current = rangeKey;
    const sequence = ++requestSequence.current;
    setLoading(true);
    setError("");
    setMilestones([]);
    setSelection(null);
    setReturnFocusSelector(undefined);
    getCalendarMilestones(from, to)
      .then((rows) => {
        if (sequence !== requestSequence.current) return;
        setMilestones(rows);
        setLoading(false);
      })
      .catch((caught: unknown) => {
        if (sequence !== requestSequence.current) return;
        setError(apiMessage(caught));
        setLoading(false);
      });
  }, [fromTime, toTime]);

  const filtered = milestones.filter((milestone) =>
    (state.type === "all" || milestone.kind === state.type)
    // An in-game event belongs to every character that sees it; a custom
    // entry without a character to none, and no filter hides it.
    && matchesCharacterFilter(milestone, state.characters));
  const withoutCalendar = characters.filter((character) =>
    character.calendarScopeMissing
    && (state.characters.length === 0 || state.characters.includes(character.connectionId)));
  const showCalendarScopeHint = withoutCalendar.length > 0 && (state.type === "all" || state.type === "event");
  const selectedDayMilestones = milestonesForDay(filtered, state.date, state.timezone);

  function setState(next: CalendarUrlState, replace: boolean) {
    setSearchParams(serializeCalendarUrlState(next), { replace });
    setSelection(null);
    setReturnFocusSelector(undefined);
  }

  /** A new entry starts on the selected day, at the next full hour, in the
   * zone the calendar is showing. */
  function newEntryDraft(): CalendarEntryDraft {
    const timeZone = state.timezone === "eve" ? EVE_TIME_ZONE : browserTimeZone();
    const nextHour = new Date(Math.ceil(Date.now() / 3_600_000) * 3_600_000);
    return { date: state.date, time: utcToWallClock(nextHour, timeZone).time, timeZone };
  }

  /** Puts a saved entry into the loaded range without refetching it. */
  function entrySaved(entry: CalendarCustomMilestone) {
    setMilestones((current) => [...current.filter((milestone) => milestone.id !== entry.id), entry]
      .sort((left, right) => left.occursAt.localeCompare(right.occursAt) || left.id.localeCompare(right.id)));
    setEntryDialog(null);
    setSelection({ kind: "milestone", milestone: entry });
  }

  async function confirmDelete(entry: CalendarCustomMilestone) {
    setDeleting(null);
    try {
      await deleteCalendarEntry(entry.entryId);
      setMilestones((current) => current.filter((milestone) => milestone.id !== entry.id));
      setSelection(null);
    } catch (caught) {
      setError(apiMessage(caught));
    }
  }

  async function reconnectWithCalendar() {
    try {
      const started = await beginAuthorization();
      if (started.fixtureMode) window.location.reload();
      else window.location.assign(started.authorizationUrl);
    } catch (caught) {
      setError(apiMessage(caught));
    }
  }

  function selectDay(key: string, rows: CalendarMilestone[], source: "day" | "overflow") {
    setSearchParams(serializeCalendarUrlState({ ...state, date: key }), { replace: true });
    const isCompact = window.matchMedia?.("(max-width: 47.999rem)").matches === true;
    if (rows.length && (source === "overflow" || !isCompact)) {
      setReturnFocusSelector(`[data-calendar-trigger="${source}-${key}"]`);
      setSelection({ kind: "day", dateKey: key, milestones: rows });
    }
  }

  return (
    <section className="iw-calendar space-y-3">
      <PageHeader eyebrow="Operations" title="Calendar">Industry completions, queued skill finishes, planetary timers and in-game calendar events from your synchronized characters.</PageHeader>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1">
          <button aria-label={`Previous ${state.view}`} className="iw-icon-button" onClick={() => setState({ ...state, date: shiftCalendarDate(state.date, state.view, -1) }, false)} type="button"><ChevronLeft aria-hidden="true" className="h-4 w-4" /></button>
          <button className="iw-button-secondary" onClick={() => {
            setState({ ...state, date: dateKey(new Date(), state.timezone) }, false);
          }} type="button">Today</button>
          <button aria-label={`Next ${state.view}`} className="iw-icon-button" onClick={() => setState({ ...state, date: shiftCalendarDate(state.date, state.view, 1) }, false)} type="button"><ChevronRight aria-hidden="true" className="h-4 w-4" /></button>
          <h2 className="ml-2 whitespace-nowrap text-base font-semibold">{state.view === "week" ? formatWeekLabel(state.date) : state.view === "year" ? state.date.slice(0, 4) : formatMonthLabel(month)}</h2>
          <button className="iw-button-secondary ml-2 inline-flex items-center gap-1" onClick={() => setEntryDialog({ draft: newEntryDraft() })} type="button"><Plus aria-hidden="true" className="h-3.5 w-3.5" /> New entry</button>
          <div className="iw-calendar-view-switch" role="group" aria-label="Calendar view">
            <button aria-label="Week view" aria-pressed={state.view === "week"} onClick={() => setState({ ...state, view: "week" }, false)} type="button">Week</button>
            <button aria-label="Month view" aria-pressed={state.view === "month"} onClick={() => setState({ ...state, view: "month" }, false)} type="button">Month</button>
            <button aria-label="Year view" aria-pressed={state.view === "year"} onClick={() => setState({ ...state, view: "year" }, false)} type="button">Year</button>
          </div>
        </div>
        <CalendarFilters
          characters={characters}
          onCharactersChange={(value) => setState({ ...state, characters: value }, true)}
          onTimezoneChange={(value) => {
            writeCalendarTimezone(value);
            setState({ ...state, timezone: value }, true);
          }}
          onTypeChange={(value) => setState({ ...state, type: value }, true)}
          selectedCharacterIds={state.characters}
          timezone={state.timezone}
          type={state.type}
        />
      </div>
      {error ? <InlineAlert title="Calendar unavailable">{error}</InlineAlert> : null}
      {showCalendarScopeHint ? <CalendarScopeHint characters={withoutCalendar} onGrant={() => void reconnectWithCalendar()} /> : null}
      <div aria-busy={loading} className="relative min-h-80">
        {state.view === "month" ? (
          <CalendarMonthView
            hasCalendarData={milestones.length > 0}
            milestones={filtered}
            month={month}
            onSelectDay={selectDay}
            onSelectMilestone={(milestone, triggerId) => {
              setSearchParams(serializeCalendarUrlState({ ...state, date: dateKey(milestone.occursAt, state.timezone) }), { replace: true });
              setReturnFocusSelector(`[data-calendar-trigger="${triggerId}"]`);
              setSelection({ kind: "milestone", milestone });
            }}
            selectedDateKey={state.date}
            selectedMilestoneId={selection?.kind === "milestone" ? selection.milestone.id : undefined}
            showEmptyState={!loading && !error}
            timezone={state.timezone}
          />
        ) : state.view === "week" && !loading && !error ? (
          <CalendarWeekView
            anchor={state.date}
            hasCalendarData={milestones.length > 0}
            milestones={filtered}
            onSelectMilestone={(milestone, triggerId) => {
              setSearchParams(serializeCalendarUrlState({ ...state, date: dateKey(milestone.occursAt, state.timezone) }), { replace: true });
              setReturnFocusSelector(`[data-calendar-trigger="${triggerId}"]`);
              setSelection({ kind: "milestone", milestone });
            }}
            selectedMilestoneId={selection?.kind === "milestone" ? selection.milestone.id : undefined}
            showEmptyState={!loading && !error}
            timezone={state.timezone}
          />
        ) : state.view === "year" && !loading && !error ? (
          <CalendarYearView
            anchor={state.date}
            hasCalendarData={milestones.length > 0}
            milestones={filtered}
            onSelectMonth={(selectedMonth) => setState({ ...state, view: "month", date: dateKeyForMonth(state.date, selectedMonth) }, false)}
            showEmptyState={!loading && !error}
            timezone={state.timezone}
          />
        ) : null}
        {loading ? <div className="absolute inset-0 grid place-items-center bg-background/70"><LoadingState><RefreshCw aria-hidden="true" className="h-4 w-4 animate-spin" /> Loading calendar...</LoadingState></div> : null}
      </div>
      {state.view === "month" ? <section className="iw-calendar-agenda" aria-label="Selected day agenda">
        <h2>{formatDateLabel(state.date)}</h2>
        {selectedDayMilestones.length ? selectedDayMilestones.map((milestone) => {
          const presentation = presentCalendarMilestone(milestone);
          return <button aria-label={`Open ${milestone.title}`} data-calendar-trigger={`agenda-${milestone.id}`} key={milestone.id} onClick={() => {
            setReturnFocusSelector(`[data-calendar-trigger="agenda-${milestone.id}"]`);
            setSelection({ kind: "milestone", milestone });
          }} type="button"><span>{milestone.title}</span><small>{presentation.activityLabel} · {milestoneCharacterSummary(milestone)}</small></button>;
        }) : !loading && !error ? <p>No milestones this day.</p> : null}
      </section> : null}
      {selection ? (
        <CalendarInspector
          entryActions={{
            onDelete: (entry) => setDeleting(entry),
            onEdit: (entry) => setEntryDialog({ draft: newEntryDraft(), entry }),
          }}
          onClose={() => setSelection(null)}
          returnFocusSelector={returnFocusSelector}
          selection={selection}
          timezone={state.timezone}
        />
      ) : null}
      {entryDialog ? (
        <CalendarEntryDialog
          characters={characters}
          draft={entryDialog.draft}
          entry={entryDialog.entry}
          onClose={() => setEntryDialog(null)}
          onSaved={entrySaved}
        />
      ) : null}
      <ConfirmDialog
        confirmLabel="Delete entry"
        onCancel={() => setDeleting(null)}
        onConfirm={() => { if (deleting) void confirmDelete(deleting); }}
        open={deleting !== null}
        title="Delete calendar entry?"
      >
        {deleting ? `"${deleting.title}" will be removed from the calendar.` : ""}
      </ConfirmDialog>
    </section>
  );
}
