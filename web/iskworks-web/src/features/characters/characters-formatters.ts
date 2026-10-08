import type { Tone } from "../../components/primitives";
import type {
  CharacterHealth,
  CharacterRosterEntry,
  CharacterSourceKind,
  SourceRefreshState,
} from "../../api/characters";

// Mirrors opportunity-formatters.ts's formatEvidenceAge shape -- kept local
// rather than shared, matching this codebase's per-feature formatter
// convention (see market/shared.tsx's own comment on the same choice).
export function formatRelativeAge(iso: string | null): string {
  if (iso === null) return "never";
  const ageSeconds = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (ageSeconds < 60) return "just now";
  const minutes = Math.floor(ageSeconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (hours < 24) return remainingMinutes > 0 ? `${hours}h ${remainingMinutes}m ago` : `${hours}h ago`;
  const days = Math.floor(hours / 24);
  const remainingHours = hours % 24;
  return remainingHours > 0 ? `${days}d ${remainingHours}h ago` : `${days}d ago`;
}

// Remaining-time formatting for a *live* remainingMs (from
// deriveTrainingState/useTrainingCountdown) lives in live-training-progress.tsx
// -- not here as render-time-only helpers, which would compute once from a
// static snapshot and never re-evaluate (leaving a permanent "finishing"
// label). There's no state called "finishing": an expired entry is simply
// skipped by deriveTrainingState.

// EVE-style compact remaining/duration text: "3d 1h", "1h 20m", "17m 6s",
// "45s". Two units at most, largest first, seconds only under an hour.
// Shared by the Overview tab's live current-skill countdown
// (live-training-progress.tsx) and the Skills tab's per-row + footer
// durations, so the two never drift.
export function formatRemainingMs(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(remainingMs / 1000));
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  if (days > 0) return `${days}d ${hours}h`;
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  const seconds = totalSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

// Compact skill-point text in EVE's idiom: "94.3m", "26.45m", "512k",
// "1.2b". Trailing zeros trimmed. The exact grouped value belongs in a
// `title`, mirroring how money.tsx pairs compact cells with an exact
// tooltip.
export function formatSpCompact(sp: number): string {
  const abs = Math.abs(sp);
  const sign = sp < 0 ? "-" : "";
  const scale = (value: number, digits: number, suffix: string): string =>
    `${sign}${Number(value.toFixed(digits))}${suffix}`;
  if (abs >= 1_000_000_000) return scale(abs / 1_000_000_000, 2, "b");
  if (abs >= 1_000_000) return scale(abs / 1_000_000, 2, "m");
  if (abs >= 1_000) return scale(abs / 1_000, 1, "k");
  return `${sign}${abs}`;
}

const SKILL_LEVEL_NUMERALS = ["I", "II", "III", "IV", "V"];

export function skillLevelNumeral(level: number): string {
  return SKILL_LEVEL_NUMERALS[level - 1] ?? String(level);
}

export function formatTrainingSkillLabel(
  skillName: string | null,
  finishedLevel: number | null,
): string | null {
  if (skillName === null) return null;
  return finishedLevel !== null ? `${skillName} ${skillLevelNumeral(finishedLevel)}` : skillName;
}

export function securityStatusTone(value: number): Tone {
  if (value >= 5.0) return "primary";
  if (value >= 0.0) return "positive";
  if (value >= -5.0) return "warning";
  return "danger";
}

export const healthMeta: Record<CharacterHealth, { label: string; tone: Tone }> = {
  healthy: { label: "Healthy", tone: "positive" },
  stale: { label: "Stale", tone: "warning" },
  partialPerms: { label: "Partial perms", tone: "warning" },
  reconnectRequired: { label: "Reconnect required", tone: "danger" },
  syncPending: { label: "Sync pending", tone: "muted" },
};

// Each roster card is bordered in its sync-health color
// (subtle for healthy, tinted otherwise) with a thicker left edge as the
// primary accent -- CharacterCard pairs this with a plain `border-{tone}/40`
// class rather than a full color token, since only the accent border needs
// tinting, not a new global color.
export const healthBorderClass: Record<CharacterHealth, string> = {
  healthy: "border-border",
  stale: "border-warning/40",
  partialPerms: "border-warning/40",
  reconnectRequired: "border-danger/40",
  syncPending: "border-primary/40",
};

export interface IndustryCategoryStat {
  key: "manufacturing" | "reaction" | "research";
  label: string;
  shortLabel: string;
  tone: Tone;
  active: number | null;
  max: number | null;
}

// The three job categories, in a fixed order and with a fixed
// per-category color (amber/purple/blue -- warning, reaction, primary
// here), shared by the roster card and the inspector's Industry Jobs
// panel.
export function industryCategoryStats(entry: CharacterRosterEntry): IndustryCategoryStat[] {
  return [
    {
      key: "manufacturing",
      label: "Manufacturing",
      shortLabel: "MFG",
      tone: "warning",
      active: entry.manufacturingActiveJobs,
      max: entry.manufacturingMaxJobs,
    },
    {
      key: "reaction",
      label: "Reactions",
      shortLabel: "RXN",
      tone: "reaction",
      active: entry.reactionActiveJobs,
      max: entry.reactionMaxJobs,
    },
    {
      key: "research",
      label: "Research",
      shortLabel: "RES",
      tone: "primary",
      active: entry.researchActiveJobs,
      max: entry.researchMaxJobs,
    },
  ];
}

export type RosterFilter = "all" | "training" | "industry" | "needsAttention";

export function matchesFilter(entry: CharacterRosterEntry, filter: RosterFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "training":
      return entry.trainingQueue.length > 0;
    case "industry":
      return (
        (entry.manufacturingActiveJobs ?? 0) > 0 ||
        (entry.reactionActiveJobs ?? 0) > 0 ||
        (entry.researchActiveJobs ?? 0) > 0
      );
    case "needsAttention":
      return entry.health !== "healthy";
  }
}

export type RosterSort = "name" | "walletBalance" | "totalSp";

export function sortEntries(entries: CharacterRosterEntry[], sort: RosterSort): CharacterRosterEntry[] {
  const sorted = [...entries];
  sorted.sort((a, b) => {
    switch (sort) {
      case "name":
        return a.characterName.localeCompare(b.characterName);
      case "walletBalance":
        return Number(b.walletBalance ?? 0) - Number(a.walletBalance ?? 0);
      case "totalSp":
        return (b.totalSp ?? 0) - (a.totalSp ?? 0);
    }
  });
  return sorted;
}

export const sourceKindLabel: Record<CharacterSourceKind, string> = {
  characterInfo: "Character Info",
  location: "Location",
  skills: "Skills",
  wallet: "Wallet",
  industryJobs: "Industry Jobs",
  assets: "Assets",
  walletTransactions: "Wallet Transactions",
  planets: "Planetary Colonies",
};

export const sourceStateMeta: Record<SourceRefreshState, { label: string; tone: Tone }> = {
  missing: { label: "Not synced", tone: "muted" },
  current: { label: "Synced", tone: "positive" },
  refreshing: { label: "Syncing", tone: "primary" },
  failed: { label: "Failed", tone: "danger" },
};

const MISSING_SCOPE_PREFIX = "missing scope: ";

// CharacterSyncService (apps/iskworks-api/src/character_sync.rs) writes this
// exact prefix when a source is skipped for lacking the scope it needs --
// reusing that string instead of a second API call for the connection's
// granted scopes.
export function missingScopeFrom(lastError: string | null): string | null {
  if (lastError === null || !lastError.startsWith(MISSING_SCOPE_PREFIX)) return null;
  return lastError.slice(MISSING_SCOPE_PREFIX.length);
}

export function matchesSearch(entry: CharacterRosterEntry, query: string): boolean {
  if (query.trim() === "") return true;
  const needle = query.trim().toLowerCase();
  return (
    entry.characterName.toLowerCase().includes(needle) ||
    (entry.corporationName?.toLowerCase().includes(needle) ?? false)
  );
}
