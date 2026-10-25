import { Link, NavLink, Route, Routes, useMatch, useSearchParams } from "react-router";

import type { SkillCatalogueCharacter } from "../../api/skills";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { CharacterSkillQueueTab } from "../characters/skill-queue-tab";
import { useNow } from "../jobs/use-jobs";
import { CatalogueTab } from "./catalogue-tab";
import { PlanEditor } from "./plan-editor";
import { PlansTab } from "./plans-tab";
import { useCharacterDetail, useCharacters, useSkillCatalogue } from "./use-skills";

function CharacterHeader({
  character,
  characters,
  onSelect,
  selectedId,
}: {
  character: SkillCatalogueCharacter | null;
  characters: Array<{ connectionId: string; characterName: string }>;
  onSelect: (connectionId: string) => void;
  selectedId: string;
}) {
  return (
    <div className="iw-panel flex flex-wrap items-center gap-x-4 gap-y-1 px-3 py-2" data-testid="skills-character-header">
      <label className="flex items-center gap-2">
        <span className="sr-only">Character</span>
        <select
          {...PRIVATE_ATTR}
          className="iw-input font-semibold"
          onChange={(event) => onSelect(event.target.value)}
          value={selectedId}
        >
          {characters.map((entry) => (
            <option key={entry.connectionId} value={entry.connectionId}>{entry.characterName}</option>
          ))}
        </select>
      </label>
      {character?.alphaDetected ? (
        <span className="rounded-[2px] border border-warning/50 px-1.5 py-px text-[0.625rem] text-warning" title="Skills are capped below their trained levels, which only happens on an Alpha clone">
          Alpha (detected)
        </span>
      ) : null}
      {character?.totalSp !== null && character?.totalSp !== undefined ? (
        <span className="font-mono text-xs text-muted">
          <span className="text-foreground">{character.totalSp.toLocaleString("en-US")} SP</span>
          {character.unallocatedSp ? (
            <span className="text-warning"> · {character.unallocatedSp.toLocaleString("en-US")} unallocated</span>
          ) : null}
        </span>
      ) : null}
      {character?.rate?.accelerated ? (
        <span className="text-xs text-muted" title="The skill in training is going faster than the character's attributes explain, so times are upper bounds">
          Training faster than attributes explain (≈ times)
        </span>
      ) : null}
    </div>
  );
}

function QueueRail({ characterId }: { characterId: string }) {
  const { data } = useCharacterDetail(characterId);
  const nowMs = useNow(1_000);
  return (
    <aside aria-label="Training queue" className="iw-panel p-3 lg:sticky lg:top-3 lg:max-h-[calc(100vh-1.5rem)] lg:overflow-y-auto">
      {data ? <CharacterSkillQueueTab detail={data} nowMs={nowMs} /> : <p className="text-xs text-muted">Loading queue…</p>}
    </aside>
  );
}

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Skills: for one character (the header picker), the full skill catalogue
// the way the client's Skill Catalogue shows it, and the workspace's skill
// plans; the training queue alongside.
export function SkillsPage() {
  const [params, setParams] = useSearchParams();
  const characters = useCharacters();
  const roster = characters.data ?? [];
  const requested = params.get("character");
  const characterId = characters.data
    ? (roster.find((entry) => entry.connectionId === requested) ?? roster[0])?.connectionId ?? null
    : undefined;
  const catalogue = useSkillCatalogue(characterId);
  // The plan editor has its own character column, so it takes the width
  // the training queue would.
  const editing = useMatch("/skills/plans/:planId") !== null;
  const tabSearch = characterId ? `?${new URLSearchParams({ character: characterId })}` : "";

  function selectCharacter(connectionId: string) {
    const next = new URLSearchParams(params);
    next.set("character", connectionId);
    next.delete("skill");
    setParams(next, { replace: true });
  }

  const error = characters.error ?? catalogue.error;
  return (
    <>
      <PageHeader eyebrow="Characters" title="Skills">
        Every skill by group, with your character's levels, prerequisites and time to train.
      </PageHeader>
      {error && !catalogue.data ? (
        <InlineAlert title="Skills unavailable">
          {error.message}{" "}
          <button className="underline" onClick={characters.error ? characters.retry : catalogue.retry} type="button">Retry</button>
        </InlineAlert>
      ) : !catalogue.data ? (
        <div aria-busy="true" aria-label="Loading skills" className="space-y-1" role="status">
          {Array.from({ length: 10 }, (_, index) => (
            <div className="h-7 w-full animate-pulse rounded-[2px] bg-panel-strong" key={index} />
          ))}
        </div>
      ) : (
        <div className={`grid gap-3 ${editing ? "" : "lg:grid-cols-[minmax(0,1fr)_20rem]"}`}>
          <div className="min-w-0 space-y-3">
            {characterId ? (
              <CharacterHeader
                character={catalogue.data.character}
                characters={roster}
                onSelect={selectCharacter}
                selectedId={characterId}
              />
            ) : (
              <Panel>
                <EmptyState action={<Link className="iw-button-primary" to="/characters">Connect a character</Link>} title="No characters connected">
                  The catalogue below is the full skill list. Connect a character to see its levels and training times.
                </EmptyState>
              </Panel>
            )}
            <nav aria-label="Skills views" className="flex items-stretch border-b border-border">
              <NavLink className={tabClass} end to={{ pathname: "/skills", search: tabSearch }}>Catalogue</NavLink>
              <NavLink className={tabClass} to={{ pathname: "/skills/plans", search: tabSearch }}>Plans</NavLink>
            </nav>
            <Routes>
              <Route element={<CatalogueTab catalogue={catalogue.data} />} index />
              <Route element={<PlansTab characterId={characterId ?? null} />} path="plans" />
              <Route
                element={<PlanEditor catalogue={catalogue.data} characterId={characterId ?? null} />}
                path="plans/:planId"
              />
            </Routes>
          </div>
          {characterId && !editing ? <QueueRail characterId={characterId} /> : null}
        </div>
      )}
    </>
  );
}
