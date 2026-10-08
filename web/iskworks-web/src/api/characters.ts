import { jsonRequest } from "./json-request";
import type { ConnectionStatus } from "./esi";

export type CharacterHealth = "healthy" | "stale" | "partialPerms" | "reconnectRequired" | "syncPending";

// Cached ESI skill queue entry, as returned by the API -- including
// entries whose finishDate has already passed. Deciding what's currently
// training is derived client-side (see deriveTrainingState), not baked
// into this shape.
export interface SkillQueueEntry {
  skillId: number;
  skillName: string | null;
  finishedLevel: number;
  queuePosition: number;
  startDate: string | null;
  finishDate: string | null;
  trainingStartSp: number | null;
  // SP threshold at the start of this queued level; with levelEndSp it
  // gives the level's full SP cost (see deriveSkillQueueSummary).
  levelStartSp: number | null;
  levelEndSp: number | null;
  // The character's current permanently-trained level in this skill (ESI
  // trained_skill_level from the synced skills array), 0 when synced but
  // untrained, null only when the skills array is absent. Drives the
  // Skills-tab level indicators' trained/queued/partial states.
  currentTrainedLevel: number | null;
}

export interface CharacterRosterEntry {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  corporationId: number | null;
  corporationName: string | null;
  securityStatus: string | null;
  solarSystemId: number | null;
  solarSystemName: string | null;
  walletBalance: string | null;
  totalSp: number | null;
  // Free SP not yet applied to any skill (ESI unallocated_sp).
  unallocatedSp: number | null;
  trainingQueue: SkillQueueEntry[];
  trainingObservedAt: string | null;
  // The connection has not granted esi-skills.read_skillqueue.v1: the queue
  // is empty for lack of authorization, not because it is actually empty.
  // The `skills` source still succeeds (total SP is a separate scope), so
  // this cannot be read off sources[].lastError like other missing scopes.
  trainingQueueScopeMissing: boolean;
  manufacturingActiveJobs: number | null;
  manufacturingMaxJobs: number | null;
  reactionActiveJobs: number | null;
  reactionMaxJobs: number | null;
  researchActiveJobs: number | null;
  researchMaxJobs: number | null;
  connectionStatus: ConnectionStatus;
  health: CharacterHealth;
  lastSyncedAt: string | null;
}

export type CharacterSourceKind =
  | "characterInfo"
  | "location"
  | "skills"
  | "wallet"
  | "industryJobs"
  | "assets"
  | "walletTransactions"
  | "planets";
export type SourceRefreshState = "missing" | "current" | "refreshing" | "failed";

export interface CharacterSourceDetail {
  sourceKind: CharacterSourceKind;
  refreshState: SourceRefreshState;
  observedAt: string | null;
  nextRefreshAt: string | null;
  lastError: string | null;
}

// Mirrors iskworks_core::IndustryActivity (serde camelCase). `other`
// covers ESI activity ids removed from EVE years ago -- it never appears in
// live data but keeps the union total.
export type IndustryActivity =
  | "manufacturing"
  | "timeEfficiencyResearch"
  | "materialEfficiencyResearch"
  | "copying"
  | "reverseEngineering"
  | "invention"
  | "reaction"
  | "other";

// One slot-occupying industry job for the inspector's Industry tab.
// startDate/endDate are ESI's own fixed timestamps; remaining time and
// progress are derived client-side (see industry-jobs.ts), never sent.
export interface IndustryJob {
  jobId: number;
  activity: IndustryActivity;
  activityId: number;
  status: string;
  blueprintTypeId: number;
  blueprintName: string | null;
  productTypeId: number | null;
  productName: string | null;
  runs: number;
  facilityId: number;
  facilityName: string | null;
  solarSystemName: string | null;
  startDate: string | null;
  endDate: string | null;
}

export interface CharacterDetail extends CharacterRosterEntry {
  sources: CharacterSourceDetail[];
  // Slot-occupying jobs, soonest-finishing first. Empty when industryJobs
  // has never synced, there are no running jobs, or the scope is missing --
  // callers tell these apart via `sources`.
  industryJobs: IndustryJob[];
}

export interface CharacterSourceSyncOutcome {
  sourceKind: CharacterSourceKind;
  outcome: "succeeded" | "failed" | "skipped";
}

export function listCharacters(): Promise<CharacterRosterEntry[]> {
  return request("/api/characters");
}

export function getCharacter(connectionId: string): Promise<CharacterDetail> {
  return request(`/api/characters/${connectionId}`);
}

export function syncCharacter(connectionId: string): Promise<CharacterSourceSyncOutcome[]> {
  return request(`/api/characters/${connectionId}/sync`, { method: "POST" });
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Characters request failed.",
});
