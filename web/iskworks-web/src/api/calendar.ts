import type { IndustryActivity } from "./characters";
import { ApiError } from "./workspace";

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

export type CalendarMilestone = CalendarIndustryMilestone | CalendarSkillMilestone | CalendarPlanetaryMilestone;

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
