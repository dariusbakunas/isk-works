import type { CalendarMilestone } from "../../api/calendar";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { CalendarEmptyState } from "./calendar-empty-state";
import { dateKey, formatDateLabel, formatTime, isToday, weekDateKeys, type CalendarTimezone } from "./calendar-dates";
import { presentCalendarMilestone } from "./calendar-presentation";

export function CalendarWeekView({
  anchor,
  hasCalendarData = true,
  milestones,
  now = new Date(),
  onSelectMilestone,
  selectedMilestoneId,
  showEmptyState = true,
  timezone,
}: {
  anchor: string;
  hasCalendarData?: boolean;
  milestones: CalendarMilestone[];
  now?: Date;
  onSelectMilestone: (milestone: CalendarMilestone, triggerId: string) => void;
  selectedMilestoneId?: string;
  showEmptyState?: boolean;
  timezone: CalendarTimezone;
}) {
  const dates = weekDateKeys(anchor);
  const today = dateKey(now, timezone);
  return (
    <div className="iw-calendar-week relative" role="group" aria-label="Calendar week">
      <div className="iw-calendar-week-grid iw-calendar-week-scroll-area">
        {dates.map((key) => {
          const rows = milestones
            .filter((milestone) => dateKey(milestone.occursAt, timezone) === key)
            .sort((left, right) => left.occursAt.localeCompare(right.occursAt) || left.id.localeCompare(right.id));
          return (
            <section aria-label={formatDateLabel(key)} className="iw-calendar-week-day" data-past={key < today} data-today={isToday(key, timezone, now)} key={key}>
              <header className="iw-calendar-week-day-header">
                <span className="iw-calendar-week-day-short-label">{formatDateLabel(key).split(",")[0]}</span>
                <span className="iw-calendar-week-day-full-label">{formatDateLabel(key)}</span>
                <strong>{Number(key.slice(-2))}</strong>
              </header>
              <div className="iw-calendar-week-events">
                {rows.length ? rows.map((milestone) => {
                  const presentation = presentCalendarMilestone(milestone);
                  const Icon = presentation.icon;
                  const triggerId = `week-milestone-${milestone.id}`;
                  return (
                    <button
                      aria-label={`${milestone.title}, ${presentation.activityLabel} for ${milestone.characterName}, ${formatTime(milestone.occursAt, timezone)}`}
                      aria-pressed={selectedMilestoneId === milestone.id}
                      className={`iw-calendar-week-card iw-calendar-week-card-${presentation.tone}`}
                      data-calendar-trigger={triggerId}
                      data-past={new Date(milestone.occursAt) < now}
                      key={milestone.id}
                      onClick={() => onSelectMilestone(milestone, triggerId)}
                      type="button"
                    >
                      <time>{formatTime(milestone.occursAt, timezone)}</time>
                      <span className="iw-calendar-week-card-title"><Icon aria-hidden="true" /> <span>{milestone.title}</span></span>
                      <span className="iw-calendar-week-card-character"><EveCharacterPortrait characterId={milestone.eveCharacterId} characterName={milestone.characterName} size={16} /><span>{milestone.characterName}</span></span>
                    </button>
                  );
                }) : <span aria-hidden="true" className="iw-calendar-week-empty-day">—</span>}
              </div>
            </section>
          );
        })}
      </div>
      {showEmptyState && milestones.length === 0 ? <CalendarEmptyState filtered={hasCalendarData} /> : null}
    </div>
  );
}
