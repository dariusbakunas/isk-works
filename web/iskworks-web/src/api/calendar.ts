import type { IndustryActivity } from "./characters";
import { ApiError, requestJson } from "./workspace";

interface CalendarMilestoneCommon {
  id: string;
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  title: string;
  occursAt: string;
}

export interface CalendarIndustryMilestone extends CalendarMilestoneCommon {
  kind: "industry";
  jobId: number;
  activity: IndustryActivity;
  activityId: number;
  status: string;
  typeId: number;
  typeName: string | null;
  blueprintTypeId: number;
  blueprintName: string | null;
  productTypeId: number | null;
  productName: string | null;
  runs: number;
  facilityId: number;
  facilityName: string | null;
  solarSystemName: string | null;
}

export interface CalendarSkillMilestone extends CalendarMilestoneCommon {
  kind: "skill";
  skillTypeId: number;
  skillName: string | null;
  targetLevel: number;
  queuePosition: number;
  nextSkillTypeId: number | null;
  nextSkillName: string | null;
}

interface CalendarPlanetaryCommon extends CalendarMilestoneCommon {
  kind: "planetary";
  planetId: number;
  planetName: string;
  planetType: string;
  solarSystemName: string | null;
  /** Projected from the in-game snapshot rather than reported by ESI. */
  estimated: boolean;
}

export interface CalendarExtractorMilestone extends CalendarPlanetaryCommon {
  event: "extractorExpiry";
  extractorCount: number;
  products: { typeId: number; name: string }[];
}

export interface CalendarImportMilestone extends CalendarPlanetaryCommon {
  event: "importDepleted";
  typeId: number;
  typeName: string;
  qtyPerHour: string;
}

export type CalendarPlanetaryMilestone = CalendarExtractorMilestone | CalendarImportMilestone;

export type CalendarEventResponse = "accepted" | "declined" | "tentative" | "not_responded";

export interface CalendarEventCharacter {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  response: CalendarEventResponse;
}

/**
 * An in-game EVE calendar event, once however many characters see it.
 * `text` is raw player-authored markup: render it as text, never as HTML.
 */
export interface CalendarEventMilestone extends CalendarMilestoneCommon {
  kind: "event";
  eventId: number;
  durationMinutes: number;
  importance: number;
  ownerId: number | null;
  ownerName: string | null;
  ownerType: string | null;
  text: string;
  characters: CalendarEventCharacter[];
}

/**
 * An entry the player added. Its character is optional (null for a
 * workspace-wide note), so it does not share the other kinds' fields.
 * `timeZone` is the IANA zone it was entered in; `description` is plain
 * text, empty when there is none.
 */
export interface CalendarCustomMilestone {
  kind: "custom";
  id: string;
  entryId: string;
  title: string;
  occursAt: string;
  timeZone: string;
  description: string;
  connectionId: string | null;
  eveCharacterId: number | null;
  characterName: string | null;
}

export type CalendarMilestone =
  | CalendarIndustryMilestone
  | CalendarSkillMilestone
  | CalendarPlanetaryMilestone
  | CalendarEventMilestone
  | CalendarCustomMilestone;

/** A milestone that always belongs to one character. */
export type CalendarCharacterMilestone = Exclude<CalendarMilestone, CalendarCustomMilestone>;

export interface CalendarEntryInput {
  /** The UTC instant, computed from the wall-clock time in `timeZone`. */
  occursAt: string;
  timeZone: string;
  title: string;
  description: string;
  connectionId: string | null;
}

const JSON_HEADERS = { "Content-Type": "application/json" };

export function createCalendarEntry(input: CalendarEntryInput): Promise<CalendarCustomMilestone> {
  return requestJson<CalendarCustomMilestone>("/api/calendar/entries", {
    method: "POST",
    headers: JSON_HEADERS,
    body: JSON.stringify(input),
  });
}

export function updateCalendarEntry(entryId: string, input: CalendarEntryInput): Promise<CalendarCustomMilestone> {
  return requestJson<CalendarCustomMilestone>(`/api/calendar/entries/${encodeURIComponent(entryId)}`, {
    method: "PUT",
    headers: JSON_HEADERS,
    body: JSON.stringify(input),
  });
}

export function deleteCalendarEntry(entryId: string): Promise<void> {
  return requestJson<void>(`/api/calendar/entries/${encodeURIComponent(entryId)}`, { method: "DELETE" });
}

export async function getCalendarMilestones(from: Date, to: Date): Promise<CalendarMilestone[]> {
  const query = new URLSearchParams({
    from: from.toISOString(),
    to: to.toISOString(),
  });
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  let response: Response;

  try {
    response = await fetch(`${baseUrl}/api/calendar?${query}`, {
      headers: { Accept: "application/json" },
      credentials: "include",
    });
  } catch {
    throw new ApiError(0, {
      code: "api_unavailable",
      message: "ISK Works API is unavailable.",
    });
  }

  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new ApiError(
      response.status,
      payload?.error ?? {
        code: "api_error",
        message: "Calendar request failed.",
      },
    );
  }

  return response.json() as Promise<CalendarMilestone[]>;
}
