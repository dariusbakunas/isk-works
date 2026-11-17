import { Link } from "react-router";

import type { CharacterRosterEntry } from "../../api/characters";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { IskCompact } from "../../components/money";
import { dotToneClasses } from "../../components/primitives";
import { formatRemainingMs, formatTrainingSkillLabel, healthMeta } from "../../lib/characters-formatters";
import { CharacterName } from "../../observability/private";
import { deriveTrainingState } from "../characters/training-state";
import { useNow } from "../jobs/use-jobs";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";

interface TrainingLine {
  text: string;
  detail: string;
  className: string;
  /** Progress through the current level, 0-1, while training. */
  fraction: number | null;
}

function trainingLine(character: CharacterRosterEntry, nowMs: number): TrainingLine {
  if (character.trainingQueueScopeMissing) {
    return { text: "No queue scope", detail: "skill queue permission not granted", className: "text-muted", fraction: null };
  }
  const state = deriveTrainingState(character.trainingQueue, nowMs);
  if (state.kind === "active") {
    const ends = Math.max(...character.trainingQueue.map((entry) => (entry.finishDate ? Date.parse(entry.finishDate) : 0)));
    return {
      text: formatTrainingSkillLabel(state.entry.skillName, state.entry.finishedLevel) ?? `Skill ${state.entry.skillId}`,
      detail: `queue ends in ${formatRemainingMs(ends - nowMs)}`,
      className: "text-foreground",
      fraction: state.fraction,
    };
  }
  if (state.kind === "paused") {
    return { text: "Training paused", detail: "the queue isn't running", className: "text-muted", fraction: null };
  }
  return { text: "Queue empty", detail: "add a skill to resume training", className: "text-danger", fraction: null };
}

function slot(label: string, used: number | null, max: number | null): string {
  return `${label} ${used ?? 0}/${max ?? "?"}`;
}

function CharacterCard({ character, nowMs }: { character: CharacterRosterEntry; nowMs: number }) {
  const training = trainingLine(character, nowMs);
  const health = healthMeta[character.health];
  return (
    <Link
      className="w-[13.5rem] shrink-0 rounded-[2px] border border-border bg-background/40 p-2.5 transition hover:border-primary/50"
      to="/characters"
    >
      <div className="flex items-center gap-2">
        <EveCharacterPortrait characterId={character.eveCharacterId} characterName={character.characterName} size={32} />
        <div className="min-w-0">
          <CharacterName className="block truncate text-xs font-semibold text-foreground" name={character.characterName} />
          <span className="block truncate text-[0.6875rem] text-muted">{character.solarSystemName ?? "Unknown location"}</span>
        </div>
        <span
          aria-label={health.label}
          className={`ml-auto h-2 w-2 shrink-0 rounded-full ${dotToneClasses[health.tone]}`}
          role="img"
          title={health.label}
        />
      </div>
      <div className="mt-2 text-sm">
        {character.walletBalance !== null ? <IskCompact className="text-foreground" value={character.walletBalance} /> : <span className="text-muted">—</span>}
      </div>
      <div className={`mt-1.5 truncate text-xs ${training.className}`}>{training.text}</div>
      <div className="truncate text-[0.625rem] text-muted">{training.detail}</div>
      <div
        aria-label="Current skill progress"
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={training.fraction === null ? undefined : Math.round(training.fraction * 100)}
        className="mt-1.5 h-1 overflow-hidden rounded-full bg-border"
        role={training.fraction === null ? undefined : "progressbar"}
      >
        {training.fraction !== null ? <i className="block h-full bg-primary" style={{ width: `${training.fraction * 100}%` }} /> : null}
      </div>
      <div className="mt-2 flex gap-2 font-mono text-[0.625rem] text-muted">
        <span>{slot("M", character.manufacturingActiveJobs, character.manufacturingMaxJobs)}</span>
        <span>{slot("R", character.reactionActiveJobs, character.reactionMaxJobs)}</span>
        <span>{slot("S", character.researchActiveJobs, character.researchMaxJobs)}</span>
      </div>
    </Link>
  );
}

/** One compact card per character: where they are, wallet, training and
 * slots (spec §3.8, §9.8). */
export function CharactersStrip({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const characters = sources.characters.data ?? [];

  return (
    <PanelFrame
      className={className}
      link={{ to: "/characters" }}
      stamp={`${characters.length} connected`}
      state={panelState(sources.characters, (data) => data.length === 0, "No characters connected.")}
      title="Characters"
    >
      <div className="flex gap-2 overflow-x-auto p-2">
        {characters.map((character) => (
          <CharacterCard key={character.connectionId + character.characterName} character={character} nowMs={nowMs} />
        ))}
      </div>
    </PanelFrame>
  );
}
