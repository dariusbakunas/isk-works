import { CalendarDays, FilterX } from "lucide-react";

export function CalendarEmptyState({ filtered }: { filtered: boolean }) {
  const Icon = filtered ? FilterX : CalendarDays;
  return (
    <div className="iw-calendar-empty" role="status">
      <Icon aria-hidden="true" className="h-5 w-5" />
      <div>
        <p className="font-semibold text-foreground">{filtered ? "No milestones match these filters" : "No calendar milestones yet"}</p>
        <p>{filtered ? "Try showing another milestone type or character." : "Industry completions, queued skill finishes and planetary timers will appear here after synchronization."}</p>
      </div>
    </div>
  );
}
