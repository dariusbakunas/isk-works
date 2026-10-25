import { AlertTriangle, Lock, Search, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";

import { beginAuthorization } from "../../api/esi";
import type { CatalogueGroup, CatalogueSkill, SkillCatalogue } from "../../api/skills";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../components/operational-table";
import { EmptyState, Panel, ProgressBar } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { AttributeTags, CatalogueLevelBlocks, SkillStateChip } from "./skill-cells";
import { SkillInspector } from "./skill-inspector";
import { filterSkills, requiredForIndex, SKILL_FILTERS, timeLabel, type SkillFilter } from "./skills-catalogue";

const COLUMNS: OperationalColumn[] = [
  { key: "level", label: "Level", width: "4.5rem" },
  { key: "skill", label: "Skill", width: "minmax(12rem, 1fr)", sticky: true },
  { key: "rank", label: "Rank", width: "3rem", align: "right", numeric: true },
  { key: "attributes", label: "Attrs", width: "5.5rem", hideBelow: "tablet" },
  { key: "time", label: "Time", width: "6.5rem", align: "right", numeric: true },
  { key: "state", label: "State", width: "7.5rem" },
];

// Collapsed, the grid shows four rows: 12 tiles from the small breakpoint
// up, 6 on a phone (two columns), so the skill list stays in view.
const COLLAPSED_GROUPS = 12;
const COLLAPSED_GROUPS_PHONE = 6;

async function reauthorize() {
  const started = await beginAuthorization();
  if (!started.fixtureMode) window.location.assign(started.authorizationUrl);
}

function GroupTile({
  className = "",
  group,
  onSelect,
  selected,
}: {
  className?: string;
  group: CatalogueGroup;
  onSelect: () => void;
  selected: boolean;
}) {
  const trainedSp = group.trainedSp ?? 0;
  const percent = group.maxSp > 0 ? (trainedSp / group.maxSp) * 100 : 0;
  return (
    <button
      aria-pressed={selected}
      className={`flex w-full items-center gap-2 rounded-[2px] border px-2 py-1.5 text-left ${
        selected ? "border-primary bg-primary/10" : "border-border bg-panel-strong hover:border-primary/50"
      } ${className}`}
      onClick={onSelect}
      title={`${trainedSp.toLocaleString("en-US")} of ${group.maxSp.toLocaleString("en-US")} SP`}
      type="button"
    >
      <span
        aria-hidden="true"
        className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-[2px] text-[0.625rem] font-bold ${
          selected ? "bg-primary/30 text-primary" : "bg-border text-muted"
        }`}
      >
        {group.name[0]}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs font-medium">{group.name}</span>
        <span className="mt-0.5 flex items-center gap-1.5">
          <span className="font-mono text-[0.625rem] text-muted">
            {group.injectedCount ?? "–"}/{group.skillCount}
          </span>
          <span className="flex-1">
            <ProgressBar percent={percent} tone={selected ? "primary" : "muted"} />
          </span>
        </span>
      </span>
    </button>
  );
}

function GroupGrid({
  groups,
  onSelect,
  selectedId,
}: {
  groups: CatalogueGroup[];
  onSelect: (groupId: number | null) => void;
  selectedId: number | null;
}) {
  const [expanded, setExpanded] = useState(false);
  // Collapsed, the grid keeps the selected group visible even when it sorts
  // past the first rows.
  const visible = expanded
    ? groups
    : groups.filter((group, index) => index < COLLAPSED_GROUPS || group.groupId === selectedId);
  return (
    <section aria-label="Skill groups" className="iw-panel p-2">
      <div className="mb-1.5 flex items-center gap-2">
        <span className="iw-eyebrow">Skill groups</span>
        <span className="flex-1" />
        {selectedId !== null ? (
          <button className="flex items-center gap-1 text-xs text-primary hover:underline" onClick={() => onSelect(null)} type="button">
            <X aria-hidden="true" className="h-3 w-3" />
            All groups
          </button>
        ) : null}
        {groups.length > COLLAPSED_GROUPS_PHONE ? (
          <button
            aria-expanded={expanded}
            className="text-xs text-muted hover:text-foreground"
            onClick={() => setExpanded((open) => !open)}
            type="button"
          >
            {expanded ? "Show less" : `Show all ${groups.length}`}
          </button>
        ) : null}
      </div>
      <div className="grid grid-cols-2 gap-1 xl:grid-cols-3">
        {visible.map((group, index) => (
          <GroupTile
            className={
              !expanded && index >= COLLAPSED_GROUPS_PHONE && group.groupId !== selectedId ? "max-sm:hidden" : ""
            }
            group={group}
            key={group.groupId}
            onSelect={() => onSelect(selectedId === group.groupId ? null : group.groupId)}
            selected={selectedId === group.groupId}
          />
        ))}
      </div>
    </section>
  );
}

function SkillName({ skill }: { skill: CatalogueSkill }) {
  const state = skill.progress?.state;
  const dim = state === "locked" || state === "omegaOnly";
  return (
    <span className={`flex min-w-0 items-center gap-1.5 ${dim ? "opacity-50" : ""}`}>
      {state === "locked" ? <Lock aria-label="Locked" className="h-3 w-3 shrink-0 text-muted" /> : null}
      <span className="truncate">{skill.name}</span>
    </span>
  );
}

export function CatalogueTab({ catalogue }: { catalogue: SkillCatalogue }) {
  const [params, setParams] = useSearchParams();
  const [search, setSearch] = useState("");
  const filter = (SKILL_FILTERS.find((option) => option.id === params.get("filter"))?.id ?? "all") as SkillFilter;
  const groupParam = params.get("group");
  const groupId = groupParam === null ? null : Number(groupParam);
  const selectedSkillId = params.has("skill") ? Number(params.get("skill")) : null;
  const character = catalogue.character;
  const hasOverlay = catalogue.skills.some((skill) => skill.progress !== null);

  function update(change: (next: URLSearchParams) => void) {
    const next = new URLSearchParams(params);
    change(next);
    setParams(next, { replace: true });
  }

  const visible = useMemo(
    () => filterSkills(catalogue.skills, { filter, groupId, search }),
    [catalogue.skills, filter, groupId, search],
  );
  const groups = useMemo(() => {
    const byGroup = new Map<number, CatalogueSkill[]>();
    for (const skill of visible) byGroup.set(skill.groupId, [...(byGroup.get(skill.groupId) ?? []), skill]);
    return catalogue.groups
      .filter((group) => byGroup.has(group.groupId))
      .map((group) => ({ group, skills: byGroup.get(group.groupId)! }));
  }, [catalogue.groups, visible]);
  const requiredFor = useMemo(() => requiredForIndex(catalogue.skills), [catalogue.skills]);
  const selected = catalogue.skills.find((skill) => skill.typeId === selectedSkillId) ?? null;

  if (!catalogue.skillDataAvailable) {
    return (
      <Panel>
        <EmptyState title="No skill data available">
          The server's static data has no skill definitions yet. An operator can re-import the SDE to load them.
        </EmptyState>
      </Panel>
    );
  }

  return (
    <div className="space-y-3">
      {character?.skillsScopeMissing ? (
        <div className="flex items-center justify-between gap-2 rounded-md border border-warning/60 bg-warning/10 px-3 py-2 text-sm" role="status">
          <span className="flex items-center gap-2 text-warning">
            <AlertTriangle aria-hidden="true" className="h-4 w-4 shrink-0" />
            <span>
              <CharacterName className="font-semibold" name={character.name} /> is missing the skills permission, so
              trained levels aren't shown. The catalogue is still browsable.
            </span>
          </span>
          <button className="iw-button-secondary" onClick={() => void reauthorize()} type="button">Reconnect</button>
        </div>
      ) : null}

      <GroupGrid
        groups={catalogue.groups}
        onSelect={(id) => update((next) => (id === null ? next.delete("group") : next.set("group", String(id))))}
        selectedId={groupId}
      />

      <div className="flex flex-wrap items-center gap-2">
        <div aria-label="Skill filter" className="flex flex-wrap" role="group">
          {SKILL_FILTERS.map((option) => (
            <button
              aria-pressed={filter === option.id}
              className={`border px-2 py-1 text-xs first:rounded-l-[2px] last:rounded-r-[2px] ${
                filter === option.id ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-muted hover:text-foreground"
              }`}
              disabled={option.id !== "all" && !hasOverlay}
              key={option.id}
              onClick={() => update((next) => (option.id === "all" ? next.delete("filter") : next.set("filter", option.id)))}
              type="button"
            >
              {option.label}
            </button>
          ))}
        </div>
        <label className="relative">
          <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
          <span className="sr-only">Filter skills</span>
          <input className="iw-input w-52 pl-8" onChange={(event) => setSearch(event.target.value)} placeholder="Filter skills…" type="search" value={search} />
        </label>
        <span className="ml-auto text-xs text-muted">{visible.length} skills</span>
      </div>

      {visible.length === 0 ? (
        <Panel>
          <EmptyState title="No matching skills">No skills match the current filter and search.</EmptyState>
        </Panel>
      ) : (
        <OperationalTable
          ariaLabel="Skills"
          columns={COLUMNS}
          onSelectRow={(rowKey) => update((next) => next.set("skill", rowKey))}
          selectedRowKey={selectedSkillId === null ? null : String(selectedSkillId)}
        >
          {groups.map(({ group, skills }) => (
            <OperationalTableGroup
              groupKey={String(group.groupId)}
              itemCount={skills.length}
              key={group.groupId}
              label={group.name}
              summary={
                group.injectedCount === null ? undefined : (
                  <span className="font-mono text-[0.625rem]">{group.injectedCount} / {group.skillCount}</span>
                )
              }
            >
              {skills.map((skill) => (
                <OperationalTableRow
                  cells={{
                    level: <CatalogueLevelBlocks skill={skill} />,
                    skill: <SkillName skill={skill} />,
                    rank: <span className="text-muted">×{skill.rank}</span>,
                    attributes: <AttributeTags primary={skill.primaryAttribute} secondary={skill.secondaryAttribute} />,
                    time: skill.progress ? (
                      <span className={`font-mono text-xs ${skill.progress.state === "max" ? "text-positive" : ""}`}>
                        {timeLabel(skill.progress)}
                      </span>
                    ) : null,
                    state: skill.progress ? <SkillStateChip progress={skill.progress} /> : null,
                  }}
                  key={skill.typeId}
                  rowKey={String(skill.typeId)}
                />
              ))}
            </OperationalTableGroup>
          ))}
        </OperationalTable>
      )}

      {selected ? (
        <SkillInspector
          onClose={() => update((next) => next.delete("skill"))}
          onSelect={(typeId) => update((next) => next.set("skill", String(typeId)))}
          requiredFor={requiredFor.get(selected.typeId) ?? []}
          skill={selected}
          skills={catalogue.skills}
        />
      ) : null}
    </div>
  );
}
