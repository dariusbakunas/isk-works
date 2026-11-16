import { X } from "lucide-react";
import { useEffect, useId, useState, type FormEvent } from "react";
import { apiMessage } from "../../api/api-error";
import { createCalendarEntry, updateCalendarEntry, type CalendarCustomMilestone } from "../../api/calendar";
import { InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import type { CalendarCharacterOption } from "./calendar-filters";
import { isValidTimeZone, timeZoneOptions, utcToWallClock, wallClockToUtc } from "./calendar-entry-time";

const TITLE_MAX = 120;
const DESCRIPTION_MAX = 4000;

/** The form's starting values for a new entry. */
export interface CalendarEntryDraft {
  date: string;
  time: string;
  timeZone: string;
}

/**
 * Adds a custom entry, or edits `entry`. The date and time are wall-clock
 * time in the chosen zone; the API gets the UTC instant.
 */
export function CalendarEntryDialog({ characters, draft, entry, onClose, onSaved }: {
  characters: CalendarCharacterOption[];
  draft: CalendarEntryDraft;
  entry?: CalendarCustomMilestone;
  onClose: () => void;
  onSaved: (entry: CalendarCustomMilestone) => void;
}) {
  const titleId = useId();
  const zoneListId = useId();
  const [timeZone, setTimeZone] = useState(entry?.timeZone ?? draft.timeZone);
  const initialClock = entry ? utcToWallClock(entry.occursAt, entry.timeZone) : { date: draft.date, time: draft.time };
  const [date, setDate] = useState(initialClock.date);
  const [time, setTime] = useState(initialClock.time);
  const [connectionId, setConnectionId] = useState(entry?.connectionId ?? "");
  const [title, setTitle] = useState(entry?.title ?? "");
  const [description, setDescription] = useState(entry?.description ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [zones] = useState(() => timeZoneOptions());

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const zoneValid = isValidTimeZone(timeZone);
  const occursAt = zoneValid ? wallClockToUtc({ date, time }, timeZone) : null;
  const titleValid = title.trim().length > 0 && title.trim().length <= TITLE_MAX;
  // A character the entry names but the workspace no longer lists still
  // shows, so saving does not silently drop it.
  const knownCharacter = !connectionId || characters.some((character) => character.connectionId === connectionId);

  async function save(event: FormEvent) {
    event.preventDefault();
    if (!occursAt || !titleValid) return;
    setSaving(true);
    setError(null);
    const input = {
      occursAt: occursAt.toISOString(),
      timeZone: timeZone.trim(),
      title: title.trim(),
      description: description.trim(),
      connectionId: connectionId || null,
    };
    try {
      onSaved(entry ? await updateCalendarEntry(entry.entryId, input) : await createCalendarEntry(input));
    } catch (reason) {
      setError(apiMessage(reason));
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-md p-4" role="dialog">
        <form className="iw-dialog-pane" onSubmit={(event) => void save(event)}>
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold" id={titleId}>{entry ? "Edit entry" : "New calendar entry"}</h2>
            <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
              <X aria-hidden="true" className="h-4 w-4" />
            </button>
          </div>
          <div className="iw-dialog-body mt-3 space-y-3">
            <label className="block space-y-1">
              <span className="iw-eyebrow block">Title</span>
              <input {...PRIVATE_ATTR} autoFocus className="iw-input" maxLength={TITLE_MAX} onChange={(event) => setTitle(event.target.value)} required value={title} />
            </label>
            <div className="grid grid-cols-2 gap-2">
              <label className="block space-y-1">
                <span className="iw-eyebrow block">Date</span>
                <input className="iw-input" onChange={(event) => setDate(event.target.value)} required type="date" value={date} />
              </label>
              <label className="block space-y-1">
                <span className="iw-eyebrow block">Time</span>
                <input className="iw-input" onChange={(event) => setTime(event.target.value)} required type="time" value={time} />
              </label>
            </div>
            <div className="space-y-1">
              <label className="block space-y-1">
                <span className="iw-eyebrow block">Time zone</span>
                <input
                  aria-invalid={!zoneValid}
                  className="iw-input"
                  list={zoneListId}
                  onChange={(event) => setTimeZone(event.target.value)}
                  placeholder="UTC"
                  required
                  value={timeZone}
                />
              </label>
              <datalist id={zoneListId}>
                {zones.map((zone) => <option key={zone} value={zone}>{zone === "UTC" ? "EVE time" : undefined}</option>)}
              </datalist>
              {!zoneValid && timeZone ? <p className="text-xs text-danger" role="alert">Pick a time zone from the list.</p> : null}
            </div>
            <label className="block space-y-1">
              <span className="iw-eyebrow block">Character</span>
              <select className="iw-input" onChange={(event) => setConnectionId(event.target.value)} value={connectionId}>
                <option value="">No character</option>
                {characters.map((character) => <option key={character.connectionId} value={character.connectionId}>{character.characterName}</option>)}
                {knownCharacter ? null : <option value={connectionId}>{entry?.characterName ?? "Unknown character"}</option>}
              </select>
            </label>
            <label className="block space-y-1">
              <span className="iw-eyebrow block">Description</span>
              <textarea
                {...PRIVATE_ATTR}
                className="iw-input min-h-24 resize-y"
                maxLength={DESCRIPTION_MAX}
                onChange={(event) => setDescription(event.target.value)}
                placeholder="Optional"
                value={description}
              />
            </label>
            {error ? <InlineAlert title="Couldn't save the entry">{error}</InlineAlert> : null}
          </div>
          <div className="iw-dialog-footer">
            <button className="iw-button-secondary" onClick={onClose} type="button">Cancel</button>
            <button className="iw-button-primary" disabled={!occursAt || !titleValid || saving} type="submit">
              {saving ? "Saving…" : entry ? "Save" : "Add entry"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
