import { useMemo } from "react";
import type { CalendarMilestone } from "../../api/calendar";
import { CalendarEmptyState } from "./calendar-empty-state";
import { dateKey, formatMonthLabel, isToday, monthGrid, yearMonths, type CalendarMonth, type CalendarTimezone } from "./calendar-dates";

const weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
type KindTotals = Record<CalendarMilestone["kind"], number>;
const KINDS = ["industry", "skill", "planetary", "event"] as const satisfies readonly CalendarMilestone["kind"][];
const MARKERS: Record<CalendarMilestone["kind"], { letter: string; label: string }> = {
  industry: { letter: "I", label: "Industry milestones" },
  skill: { letter: "S", label: "Skill milestones" },
  planetary: { letter: "P", label: "Planetary timers" },
  event: { letter: "E", label: "In-game events" },
};

function emptyTotals(): KindTotals {
  return { industry: 0, skill: 0, planetary: 0, event: 0 };
}

export function CalendarYearView({
  anchor,
  hasCalendarData = true,
  milestones,
  now = new Date(),
  onSelectMonth,
  showEmptyState = true,
  timezone,
}: {
  anchor: string;
  hasCalendarData?: boolean;
  milestones: CalendarMilestone[];
  now?: Date;
  onSelectMonth: (month: CalendarMonth) => void;
  showEmptyState?: boolean;
  timezone: CalendarTimezone;
}) {
  const months = yearMonths(anchor);
  const today = dateKey(now, timezone);
  const { byDate, totals } = useMemo(() => {
    const nextByDate = new Map<string, Set<CalendarMilestone["kind"]>>();
    const nextTotals = new Map<string, KindTotals>();
    for (const milestone of milestones) {
      const key = dateKey(milestone.occursAt, timezone);
      const monthKey = key.slice(0, 7);
      const kinds = nextByDate.get(key) ?? new Set<CalendarMilestone["kind"]>();
      kinds.add(milestone.kind);
      nextByDate.set(key, kinds);
      const total = nextTotals.get(monthKey) ?? emptyTotals();
      total[milestone.kind] += 1;
      nextTotals.set(monthKey, total);
    }
    return { byDate: nextByDate, totals: nextTotals };
  }, [milestones, timezone]);

  return (
    <div className="iw-calendar-year relative" role="group" aria-label={`Calendar year ${anchor.slice(0, 4)}`}>
      <div className="iw-calendar-year-grid">
        {months.map((month) => {
          const monthKey = `${month.year}-${String(month.month).padStart(2, "0")}`;
          const label = formatMonthLabel(month);
          const total = totals.get(monthKey) ?? emptyTotals();
          return (
            <button
              aria-label={`${label}, ${total.industry} Industry, ${total.skill} Skill, ${total.planetary} Planetary, ${total.event} Event`}
              className="iw-calendar-year-month"
              data-calendar-year-month={monthKey}
              key={monthKey}
              onClick={() => onSelectMonth(month)}
              type="button"
            >
              <span className="iw-calendar-year-month-header" aria-hidden="true">
                <strong>{label.split(" ")[0]}</strong>
                <span>{KINDS.map((kind) => <span data-kind={kind} key={kind}>{MARKERS[kind].letter} {total[kind]}</span>)}</span>
              </span>
              <span className="iw-calendar-year-weekdays" aria-hidden="true">
                {weekdays.map((weekday) => <span key={weekday}>{weekday}</span>)}
              </span>
              <span className="iw-calendar-year-days">
                {monthGrid(month).map((day) => {
                  if (!day.inMonth) return <span aria-hidden="true" className="iw-calendar-year-day-placeholder" key={day.key} />;
                  const kinds = byDate.get(day.key);
                  return (
                    <span
                      className="iw-calendar-year-day"
                      data-calendar-year-day=""
                      data-milestone-kinds={KINDS.filter((kind) => kinds?.has(kind)).join(" ")}
                      data-past={day.key < today}
                      data-testid={`year-day-${day.key}`}
                      data-today={isToday(day.key, timezone, now)}
                      key={day.key}
                    >
                      <span aria-hidden="true">{day.dayNumber}</span>
                      <span className="iw-calendar-year-markers">
                        {KINDS.filter((kind) => kinds?.has(kind)).map((kind) => (
                          <span aria-label={MARKERS[kind].label} data-kind={kind} key={kind}>{MARKERS[kind].letter}</span>
                        ))}
                      </span>
                    </span>
                  );
                })}
              </span>
            </button>
          );
        })}
      </div>
      {showEmptyState && milestones.length === 0 ? <CalendarEmptyState filtered={hasCalendarData} /> : null}
    </div>
  );
}
