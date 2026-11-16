import type { CalendarEventMilestone, CalendarEventResponse, CalendarMilestone } from "../../api/calendar";

/**
 * Player-authored event text as plain text. EVE stores it as HTML-ish
 * markup (`<font>`, `<br>`, `<a href="showinfo:…">`); line breaks are kept,
 * every tag is dropped and entities are decoded. The result must still be
 * rendered as text (React escapes it), never as HTML.
 */
export function eventPlainText(text: string): string {
  const withBreaks = text
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|li)>/gi, "\n");
  // DOMParser builds an inert document: no scripts run, nothing loads.
  const document = new DOMParser().parseFromString(withBreaks, "text/html");
  return (document.body.textContent ?? "")
    .replace(/ /g, " ")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function formatEventDuration(minutes: number): string | null {
  if (minutes <= 0) return null;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest} min`;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

export function eventEndsAt(milestone: CalendarEventMilestone): Date | null {
  if (milestone.durationMinutes <= 0) return null;
  return new Date(new Date(milestone.occursAt).getTime() + milestone.durationMinutes * 60_000);
}

export const responseLabel: Record<CalendarEventResponse, string> = {
  accepted: "Accepted",
  declined: "Declined",
  tentative: "Tentative",
  not_responded: "Not responded",
};

const ownerTypeLabels: Record<string, string> = {
  eve_server: "CCP",
  corporation: "Corporation",
  alliance: "Alliance",
  faction: "Faction",
  character: "Character",
};

export function ownerTypeLabel(ownerType: string | null): string | null {
  if (!ownerType) return null;
  return ownerTypeLabels[ownerType] ?? ownerType;
}

/** Every connection a milestone belongs to: all viewers of an event; none
 * for a custom entry without a character. */
export function milestoneConnectionIds(milestone: CalendarMilestone): string[] {
  if (milestone.kind === "event") return milestone.characters.map((character) => character.connectionId);
  if (milestone.kind === "custom") return milestone.connectionId ? [milestone.connectionId] : [];
  return [milestone.connectionId];
}

/** Whether the character filter shows the milestone. A custom entry without
 * a character is a workspace-wide note: no filter hides it. */
export function matchesCharacterFilter(milestone: CalendarMilestone, selected: readonly string[]): boolean {
  if (selected.length === 0) return true;
  const ids = milestoneConnectionIds(milestone);
  if (milestone.kind === "custom" && ids.length === 0) return true;
  return ids.some((id) => selected.includes(id));
}

/** "Aeva" or "Aeva +2" for an event several characters see; "No character"
 * for a workspace-wide custom entry. */
export function milestoneCharacterSummary(milestone: CalendarMilestone): string {
  if (milestone.kind === "custom") return milestone.characterName ?? "No character";
  if (milestone.kind !== "event" || milestone.characters.length <= 1) return milestone.characterName;
  return `${milestone.characterName} +${milestone.characters.length - 1}`;
}
