import { Link } from "react-router";

import type { CalendarMilestone } from "../../api/calendar";
import { dateKey, type CalendarTimezone } from "../calendar/calendar-dates";
import { presentCalendarMilestone, type CalendarMilestonePresentation } from "../calendar/calendar-presentation";
import { readCalendarTimezone } from "../calendar/calendar-timezone-preference";
import { CharacterName } from "../../observability/private";
import { eveDateTime } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";

const ROW_LIMIT = 8;
/** Below `sm` only the first few rows show (spec §9.11). */
const MOBILE_ROWS = 3;
const CALENDAR_WEEK = "/calendar?view=week";

const TONE_COLORS: Record<CalendarMilestonePresentation["tone"], string> = {
  industry: "var(--color-primary)",
  skill: "var(--iw-calendar-skill)",
  planetary: "var(--iw-calendar-planetary)",
  event: "var(--iw-calendar-event)",
  custom: "var(--iw-calendar-custom)",
};

/** "in 42m", "in 2h 14m", "in 31h" (minutes dropped from a day out). */
export function inLabel(atMs: number, nowMs: number): string {
  const minutes = Math.floor((atMs - nowMs) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `in ${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return hours >= 24 || rest === 0 ? `in ${hours}h` : `in ${hours}h ${rest}m`;
}

export interface AgendaGroup {
  key: string;
  label: string;
  rows: CalendarMilestone[];
}

const DAY_LABEL = new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric", month: "short", timeZone: "UTC" });

function dayLabel(key: string, todayKey: string, tomorrowKey: string): string {
  if (key === todayKey) return "Today";
  if (key === tomorrowKey) return "Tomorrow";
  return DAY_LABEL.format(new Date(`${key}T00:00:00Z`)).replace(",", "");
}

/** Upcoming milestones by day (in the Calendar's timezone), soonest first,
 * capped at `limit` rows; `hidden` is how many the cap left out. */
export function agendaGroups(
  milestones: CalendarMilestone[],
  nowMs: number,
  timezone: CalendarTimezone,
  limit: number,
): { groups: AgendaGroup[]; hidden: number } {
  const upcoming = milestones
    .filter((milestone) => Date.parse(milestone.occursAt) >= nowMs)
    .sort((a, b) => Date.parse(a.occursAt) - Date.parse(b.occursAt));
  const todayKey = dateKey(new Date(nowMs), timezone);
  const tomorrowKey = dateKey(new Date(nowMs + 86_400_000), timezone);
  const groups: AgendaGroup[] = [];
  for (const milestone of upcoming.slice(0, limit)) {
    const key = dateKey(milestone.occursAt, timezone);
    const last = groups.at(-1);
    if (last?.key === key) last.rows.push(milestone);
    else groups.push({ key, label: dayLabel(key, todayKey, tomorrowKey), rows: [milestone] });
  }
  return { groups, hidden: Math.max(0, upcoming.length - limit) };
}

function characterLabel(milestone: CalendarMilestone): string {
  return milestone.characterName ?? "Workspace";
}

/** The next 48 hours from the Calendar: jobs finishing, skills, PI, events
 * and the player's own entries (spec §3.2, §9.3). */
export function AgendaPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const timezone = readCalendarTimezone();
  const calendar = sources.calendar;
  const { groups, hidden } = agendaGroups(calendar.data ?? [], nowMs, timezone, ROW_LIMIT);
  const state = panelState(calendar, () => groups.length === 0, "Nothing scheduled in the next 48 hours.");

  return (
    <PanelFrame className={className} link={{ to: CALENDAR_WEEK }} state={state} title="Next 48 h">
      <div>
        {groups.map((group, groupIndex) => {
          const firstIndex = groups.slice(0, groupIndex).reduce((count, previous) => count + previous.rows.length, 0);
          return (
          <div key={group.key} className={firstIndex >= MOBILE_ROWS ? "hidden sm:block" : undefined}>
            <div className="flex h-6 items-center bg-panel-strong/60 px-3 text-[0.6875rem] font-semibold tracking-wider text-muted">
              {group.label}
            </div>
            {group.rows.map((milestone, rowIndex) => {
              const presentation = presentCalendarMilestone(milestone);
              const Icon = presentation.icon;
              return (
                <Link
                  key={milestone.id}
                  aria-label={presentation.accessibleLabel}
                  className={`${firstIndex + rowIndex >= MOBILE_ROWS ? "hidden sm:grid" : "grid"} min-h-10 grid-cols-[4.5rem_1.25rem_minmax(0,1fr)] items-center gap-1 border-t border-border/60 px-3 py-1 hover:bg-panel-strong/50`}
                  to={`/calendar?view=week&date=${group.key}&tz=${timezone}`}
                >
                  <span className="font-mono text-[0.6875rem] text-muted" title={`${eveDateTime(milestone.occursAt)} EVE`}>
                    {inLabel(Date.parse(milestone.occursAt), nowMs)}
                  </span>
                  <Icon aria-hidden="true" className="h-3.5 w-3.5" style={{ color: TONE_COLORS[presentation.tone] }} />
                  <span className="min-w-0">
                    <span className="block truncate text-xs text-foreground" title={milestone.title}>{milestone.title}</span>
                    <span className="block truncate text-[0.6875rem] text-muted">
                      {milestone.characterName ? <CharacterName name={milestone.characterName} /> : characterLabel(milestone)}
                    </span>
                  </span>
                </Link>
              );
            })}
          </div>
          );
        })}
        {hidden > 0 ? (
          <Link className="block h-8 border-t border-border text-center text-xs leading-8 text-primary hover:underline" to={CALENDAR_WEEK}>
            +{hidden} more
          </Link>
        ) : null}
      </div>
    </PanelFrame>
  );
}
