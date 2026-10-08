import type { CalendarMilestone } from "../../api/calendar";
import {
  dateKey,
  formatDateLabel,
  groupByDate,
  isToday,
  monthGrid,
  type CalendarMonth,
  type CalendarTimezone,
} from "./calendar-dates";
import { CalendarEmptyState } from "./calendar-empty-state";
import { CalendarMilestoneLabel, presentCalendarMilestone } from "./calendar-presentation";

const weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function CalendarMonthView({
  hasCalendarData = true,
  milestones,
  month,
  now = new Date(),
  onSelectDay,
  onSelectMilestone,
  selectedDateKey,
  selectedMilestoneId,
  showEmptyState = true,
  timezone,
}: {
  hasCalendarData?: boolean;
  milestones: CalendarMilestone[];
  month: CalendarMonth;
  now?: Date;
  onSelectDay: (key: string, milestones: CalendarMilestone[], source: "day" | "overflow") => void;
  onSelectMilestone: (milestone: CalendarMilestone, triggerId: string) => void;
  selectedDateKey: string;
  selectedMilestoneId?: string;
  showEmptyState?: boolean;
  timezone: CalendarTimezone;
}) {
  const days = monthGrid(month);
  const grouped = groupByDate(milestones, timezone);

  return (
    <div className="iw-calendar-month relative">
      <div className="iw-calendar-weekdays" role="row">
        {weekdays.map((weekday) => <div key={weekday} role="columnheader">{weekday}</div>)}
      </div>
      <div aria-label="Calendar month" className="iw-calendar-month-grid" role="grid">
        {days.map((day) => {
          const dayMilestones = grouped.get(day.key) ?? [];
          const label = formatDateLabel(day.key);
          const today = isToday(day.key, timezone, now);
          return (
            <div
              aria-label={label}
              className="iw-calendar-day"
              data-in-month={day.inMonth}
              data-selected={day.key === selectedDateKey}
              data-today={today}
              key={day.key}
              role="gridcell"
            >
              <button aria-label={`Show milestones for ${label}`} className="iw-calendar-day-number" data-calendar-trigger={`day-${day.key}`} onClick={() => onSelectDay(day.key, dayMilestones, "day")} type="button">{day.dayNumber}</button>
              <div className="iw-calendar-day-events">
                {dayMilestones.slice(0, 3).map((milestone) => {
                  const presentation = presentCalendarMilestone(milestone);
                  return (
                    <button
                      aria-label={presentation.accessibleLabel}
                      aria-pressed={milestone.id === selectedMilestoneId}
                      className={`iw-calendar-chip iw-calendar-chip-${presentation.tone}`}
                      data-past={new Date(milestone.occursAt) < now}
                      data-calendar-trigger={`milestone-${milestone.id}`}
                      key={milestone.id}
                      onClick={() => onSelectMilestone(milestone, `milestone-${milestone.id}`)}
                      type="button"
                    >
                      <CalendarMilestoneLabel milestone={milestone} />
                    </button>
                  );
                })}
                {dayMilestones.length > 3 ? (
                  <button aria-label={`Show ${dayMilestones.length - 3} more milestones for ${label}`} className="iw-calendar-more" data-calendar-trigger={`overflow-${day.key}`} onClick={() => onSelectDay(day.key, dayMilestones, "overflow")} type="button">+{dayMilestones.length - 3} more</button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
      {showEmptyState && milestones.length === 0 ? <CalendarEmptyState filtered={hasCalendarData} /> : null}
    </div>
  );
}

export function milestonesForDay(milestones: CalendarMilestone[], key: string, timezone: CalendarTimezone) {
  return milestones.filter((milestone) => dateKey(milestone.occursAt, timezone) === key);
}
