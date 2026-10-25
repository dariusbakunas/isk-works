import { ArrowDown, ArrowUp, Check, Copy, GripVertical, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";

import {
  deleteSkillPlan,
  exportPlanText,
  getSkillPlan,
  updateSkillPlan,
  type CatalogueSkill,
  type PlanEntry,
  type PlanEntryProgress,
  type SkillCatalogue,
  type SkillPlanDetail,
} from "../../api/skills";
import { ConfirmDialog, InlineAlert, ProgressBar } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { formatRemainingMs, skillLevelNumeral } from "../characters/characters-formatters";
import { SkillLevelBlocks } from "../characters/skill-level-blocks";
import { skillLevelCells } from "../characters/skill-queue-summary";
import { dependentsOf, errorMessage, planProgressLabel } from "./skill-plans";

const entryKey = (entry: PlanEntry) => `${entry.skillTypeId}:${entry.level}`;
const label = (name: string, level: number) => `${name} ${skillLevelNumeral(level)}`;

function CompletionRing({ percent, sub }: { percent: number; sub: string }) {
  const radius = 42;
  const circumference = 2 * Math.PI * radius;
  const done = Math.min(100, Math.max(0, percent));
  return (
    <div className="relative mx-auto h-28 w-28" data-testid="plan-completion">
      <svg aria-hidden="true" className="h-full w-full -rotate-90" viewBox="0 0 100 100">
        <circle className="stroke-border" cx="50" cy="50" fill="none" r={radius} strokeWidth="7" />
        <circle
          className={done >= 100 ? "stroke-positive" : "stroke-primary"}
          cx="50"
          cy="50"
          fill="none"
          r={radius}
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - done / 100)}
          strokeLinecap="round"
          strokeWidth="7"
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span className="text-lg font-semibold">{done >= 100 ? "✓" : `${Math.floor(done)}%`}</span>
        <span className="font-mono text-[0.625rem] text-muted">{sub}</span>
      </div>
    </div>
  );
}

function entryTime(progress: PlanEntryProgress | undefined): string {
  if (!progress) return "";
  if (progress.status === "done") return "—";
  if (progress.status === "blocked" || progress.seconds === null) return "";
  const text = formatRemainingMs(progress.seconds * 1000);
  return progress.estimated ? `≈ ${text}` : text;
}

function AddSkill({
  catalogueSkills,
  onAdd,
  planned,
}: {
  catalogueSkills: CatalogueSkill[];
  onAdd: (entry: PlanEntry, name: string) => void;
  planned: PlanEntry[];
}) {
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<CatalogueSkill | null>(null);
  const [level, setLevel] = useState(1);
  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (needle === "" || picked) return [];
    return catalogueSkills.filter((skill) => skill.name.toLowerCase().includes(needle)).slice(0, 8);
  }, [catalogueSkills, query, picked]);

  function pick(skill: CatalogueSkill) {
    const highestPlanned = Math.max(0, ...planned.filter((entry) => entry.skillTypeId === skill.typeId).map((entry) => entry.level));
    const trained = skill.progress?.trainedLevel ?? 0;
    setPicked(skill);
    setQuery(skill.name);
    setLevel(Math.min(5, Math.max(highestPlanned, trained) + 1));
  }

  return (
    <div className="relative flex flex-wrap items-center gap-2 border-t border-border px-2 py-2">
      <input
        aria-autocomplete="list"
        aria-expanded={matches.length > 0}
        aria-label="Add a skill"
        className="iw-input min-w-0 flex-1"
        onChange={(event) => {
          setQuery(event.target.value);
          setPicked(null);
        }}
        placeholder="Search to add a skill…"
        role="combobox"
        value={query}
      />
      {picked ? (
        <>
          <label>
            <span className="sr-only">Level</span>
            <select className="iw-input" onChange={(event) => setLevel(Number(event.target.value))} value={level}>
              {[1, 2, 3, 4, 5].map((value) => <option key={value} value={value}>{skillLevelNumeral(value)}</option>)}
            </select>
          </label>
          <button
            className="iw-button-primary"
            onClick={() => {
              onAdd({ skillTypeId: picked.typeId, level }, picked.name);
              setQuery("");
              setPicked(null);
            }}
            type="button"
          >
            Add to plan
          </button>
        </>
      ) : null}
      {matches.length > 0 ? (
        <ul className="absolute bottom-full left-2 right-2 z-10 max-h-56 overflow-y-auto border border-border bg-panel shadow-xl" role="listbox">
          {matches.map((skill) => (
            <li aria-label={skill.name} aria-selected={false} key={skill.typeId} onClick={() => pick(skill)} role="option">
              <span className="flex cursor-pointer justify-between px-2 py-1 text-xs hover:bg-primary/10">
                <span>{skill.name}</span>
                <span className="text-muted">{skill.groupName}</span>
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function PlanEditor({ catalogue, characterId }: { catalogue: SkillCatalogue; characterId: string | null }) {
  const { planId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const [detail, setDetail] = useState<SkillPlanDetail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [hideTrained, setHideTrained] = useState(false);
  const [pendingRemoval, setPendingRemoval] = useState<{ index: number; dependents: number[] } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);
  // The plan text, shown to copy by hand when the browser blocks the
  // clipboard (permissions, embedded frames).
  const [copyFallback, setCopyFallback] = useState<string | null>(null);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const listSearch = characterId ? `?${new URLSearchParams({ character: characterId })}` : "";

  useEffect(() => {
    const controller = new AbortController();
    setLoadError(null);
    getSkillPlan(planId, controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) setDetail(result);
      })
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setLoadError(errorMessage(reason, "Could not load the plan."));
      });
    return () => controller.abort();
  }, [planId]);

  const trainedBySkill = useMemo(
    () => new Map(catalogue.skills.map((skill) => [skill.typeId, skill.progress?.trainedLevel ?? 0])),
    [catalogue.skills],
  );

  if (loadError) {
    return (
      <InlineAlert title="Skill plan unavailable">
        {loadError} <Link className="underline" to={{ pathname: "/skills/plans", search: listSearch }}>Back to plans</Link>
      </InlineAlert>
    );
  }
  if (!detail) {
    return <div aria-busy="true" aria-label="Loading skill plan" className="h-64 animate-pulse rounded-[2px] bg-panel-strong" role="status" />;
  }

  const plan = detail.plan;
  const entries: PlanEntry[] = detail.entries.map(({ skillTypeId, level }) => ({ skillTypeId, level }));
  const evaluation = detail.characters.find((character) => character.connectionId === characterId)?.evaluation ?? null;
  const progressByKey = new Map((evaluation?.entries ?? []).map((entry) => [entryKey(entry), entry]));
  const nameOf = (entry: PlanEntry) => detail.entries.find((view) => entryKey(view) === entryKey(entry))?.name ?? `Skill ${entry.skillTypeId}`;
  const closestId = detail.characters.find(
    (character) => character.evaluation && !character.evaluation.blocked && character.evaluation.percentComplete < 100,
  )?.connectionId;

  async function save(next: PlanEntry[], added?: string) {
    setSaving(true);
    setSaveError(null);
    setNotice(null);
    try {
      const result = await updateSkillPlan(plan.id, { name: plan.name, description: plan.description, entries: next });
      setDetail(result);
      if (added) {
        const count = result.inserted.length;
        setNotice(count > 0 ? `Added ${added} and ${count} prerequisite${count === 1 ? "" : "s"}` : `Added ${added}`);
      }
    } catch (reason) {
      setSaveError(errorMessage(reason, "Could not save the plan."));
    } finally {
      setSaving(false);
    }
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= entries.length || from === to) return;
    const next = [...entries];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    void save(next);
  }

  function remove(index: number) {
    const dependents = dependentsOf(entries, index, catalogue.skills);
    if (dependents.length > 0) {
      setPendingRemoval({ index, dependents });
      return;
    }
    void save(entries.filter((_, position) => position !== index));
  }

  async function copy() {
    let text: string;
    try {
      text = await exportPlanText(plan.id);
    } catch (reason) {
      setSaveError(errorMessage(reason, "Could not export the plan."));
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopyFallback(null);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2_000);
    } catch {
      setCopyFallback(text);
    }
  }

  async function removePlan() {
    await deleteSkillPlan(plan.id);
    navigate({ pathname: "/skills/plans", search: listSearch });
  }

  function selectCharacter(connectionId: string) {
    const next = new URLSearchParams(params);
    next.set("character", connectionId);
    setParams(next, { replace: true });
  }

  const visible = detail.entries
    .map((view, index) => ({ view, index, progress: progressByKey.get(entryKey(view)) }))
    .filter(({ progress }) => !(hideTrained && progress?.status === "done"));
  const totalSp = detail.entries.reduce((sum, entry) => sum + entry.sp, 0);

  return (
    <div className="grid gap-3 xl:grid-cols-[15rem_minmax(0,1fr)_16rem]">
      <aside aria-label="Plan summary" className="iw-panel space-y-3 p-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold">{plan.name}</h2>
            {plan.description ? <p className="mt-0.5 text-xs text-muted">{plan.description}</p> : null}
          </div>
          <Link aria-label="Back to plans" className="text-muted hover:text-foreground" to={{ pathname: "/skills/plans", search: listSearch }}>
            <X aria-hidden="true" className="h-4 w-4" />
          </Link>
        </div>
        {evaluation ? (
          <CompletionRing percent={evaluation.percentComplete} sub={planProgressLabel(evaluation)} />
        ) : (
          <p className="text-xs text-muted">No skills data for this character yet.</p>
        )}
        {evaluation && evaluation.missingSkillbooks.length > 0 ? (
          <div className="rounded-[2px] border border-warning/50 bg-warning/10 px-2 py-1.5 text-xs text-warning">
            Missing skillbooks ({evaluation.missingSkillbooks.length})
          </div>
        ) : null}
        <button className="iw-button-secondary flex w-full items-center justify-center gap-1" onClick={() => void copy()} type="button">
          {copied ? <Check aria-hidden="true" className="h-3.5 w-3.5" /> : <Copy aria-hidden="true" className="h-3.5 w-3.5" />}
          {copied ? "Copied" : "Copy for in-game import"}
        </button>
        {copyFallback !== null ? (
          <div className="space-y-1">
            <p className="text-xs text-warning">Couldn't copy automatically. Select the text and copy it.</p>
            <textarea
              aria-label="Plan text to copy"
              className="iw-input h-32 w-full font-mono text-[0.625rem]"
              onFocus={(event) => event.currentTarget.select()}
              readOnly
              value={copyFallback}
            />
          </div>
        ) : null}
        <button className="iw-button-danger flex w-full items-center justify-center gap-1" onClick={() => setConfirmDelete(true)} type="button">
          <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />
          Delete plan
        </button>
      </aside>

      <section aria-label="Plan content" className="iw-panel flex min-w-0 flex-col">
        <div className="flex items-center gap-3 border-b border-border px-2 py-1.5">
          <label className="flex items-center gap-1.5 text-xs">
            <input checked={hideTrained} onChange={(event) => setHideTrained(event.target.checked)} type="checkbox" />
            Hide trained
          </label>
          {saving ? <span className="text-xs text-muted">Saving…</span> : null}
          {notice ? <span className="text-xs text-positive" role="status">{notice}</span> : null}
        </div>
        {saveError ? <div className="px-2 pt-2"><InlineAlert title="Not saved">{saveError}</InlineAlert></div> : null}
        {detail.entries.length === 0 ? (
          <p className="px-3 py-6 text-center text-xs text-muted">This plan is empty. Search below to add a skill.</p>
        ) : null}
        <ol aria-label="Plan entries" className="divide-y divide-border">
          {visible.map(({ view, index, progress }) => {
            const name = label(view.name, view.level);
            const done = progress?.status === "done";
            return (
              <li
                className={`group flex items-center gap-2 px-2 py-1.5 text-xs ${done ? "opacity-50" : ""} ${dragIndex === index ? "bg-primary/10" : ""}`}
                data-entry={name}
                data-status={progress?.status ?? "unknown"}
                draggable
                key={entryKey(view)}
                onDragOver={(event) => event.preventDefault()}
                onDragStart={() => setDragIndex(index)}
                onDrop={() => {
                  if (dragIndex !== null) move(dragIndex, index);
                  setDragIndex(null);
                }}
              >
                <GripVertical aria-hidden="true" className="h-3.5 w-3.5 shrink-0 cursor-grab text-muted" />
                <SkillLevelBlocks
                  cells={skillLevelCells({
                    targetLevel: view.level,
                    currentTrainedLevel: trainedBySkill.get(view.skillTypeId) ?? 0,
                    trainingLevel: null,
                    hasPriorPartialSp: progress?.status === "partial",
                  })}
                  target={view.level}
                />
                <span className="min-w-0 flex-1 truncate">
                  {name}
                  {done ? <Check aria-label="trained" className="ml-1 inline h-3 w-3 text-positive" /> : null}
                </span>
                {progress?.status === "blocked" ? (
                  <span className="rounded-[2px] border border-[#8a6cc4]/50 px-1 text-[0.625rem] text-[#a58ad8]">Omega only</span>
                ) : null}
                <span className="w-20 text-right font-mono">{entryTime(progress)}</span>
                <span className="flex shrink-0 gap-0.5 opacity-60 group-hover:opacity-100">
                  <button aria-label={`Move ${name} up`} className="p-0.5 hover:text-primary disabled:opacity-30" disabled={saving || index === 0} onClick={() => move(index, index - 1)} type="button">
                    <ArrowUp aria-hidden="true" className="h-3 w-3" />
                  </button>
                  <button aria-label={`Move ${name} down`} className="p-0.5 hover:text-primary disabled:opacity-30" disabled={saving || index === entries.length - 1} onClick={() => move(index, index + 1)} type="button">
                    <ArrowDown aria-hidden="true" className="h-3 w-3" />
                  </button>
                  <button aria-label={`Remove ${name}`} className="p-0.5 hover:text-danger" disabled={saving} onClick={() => remove(index)} type="button">
                    <Trash2 aria-hidden="true" className="h-3 w-3" />
                  </button>
                </span>
              </li>
            );
          })}
        </ol>
        <AddSkill catalogueSkills={catalogue.skills} onAdd={(entry, name) => void save([...entries, entry], label(name, entry.level))} planned={entries} />
        <div className="mt-auto flex justify-between border-t border-border px-2 py-1.5 font-mono text-xs text-muted" data-testid="plan-footer">
          <span>{detail.entries.length} skills · {totalSp.toLocaleString("en-US")} SP</span>
          {evaluation ? <span className="text-foreground">{planProgressLabel(evaluation)}</span> : null}
        </div>
      </section>

      <aside className="iw-panel p-2">
        <h3 className="iw-eyebrow mb-1.5 px-1">Character progress</h3>
        <ul aria-label="Character progress" className="space-y-1">
          {detail.characters.map((character) => {
            const result = character.evaluation;
            const selected = character.connectionId === characterId;
            return (
              <li key={character.connectionId}>
                <button
                  aria-pressed={selected}
                  className={`w-full rounded-[2px] border px-2 py-1.5 text-left text-xs ${selected ? "border-primary/60 bg-primary/10" : "border-transparent hover:border-border"}`}
                  onClick={() => selectCharacter(character.connectionId)}
                  type="button"
                >
                  <span className="flex items-center gap-1.5">
                    <CharacterName className="truncate font-medium" name={character.name} />
                    {character.connectionId === closestId ? (
                      <span className="rounded-[2px] border border-positive/50 px-1 text-[0.625rem] text-positive">Closest</span>
                    ) : null}
                  </span>
                  {result ? (
                    <>
                      <span className="mt-1 block"><ProgressBar percent={result.percentComplete} /></span>
                      <span className="mt-0.5 flex flex-wrap gap-x-2 font-mono text-[0.625rem] text-muted">
                        <span>{planProgressLabel(result)}</span>
                        {result.missingSkillbooks.length > 0 ? (
                          <span className="text-warning">{result.missingSkillbooks.length} books</span>
                        ) : null}
                        {result.blocked ? <span className="text-[#a58ad8]">Blocked: Omega only</span> : null}
                      </span>
                    </>
                  ) : (
                    <span className="mt-0.5 block text-[0.625rem] text-muted">No skills data</span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      <ConfirmDialog
        confirmLabel={pendingRemoval ? `Remove ${pendingRemoval.dependents.length + 1} entries` : "Remove"}
        onCancel={() => setPendingRemoval(null)}
        onConfirm={() => {
          if (!pendingRemoval) return;
          const dropped = new Set([pendingRemoval.index, ...pendingRemoval.dependents]);
          setPendingRemoval(null);
          void save(entries.filter((_, position) => !dropped.has(position)));
        }}
        open={pendingRemoval !== null}
        title={pendingRemoval ? `Remove ${nameOf(entries[pendingRemoval.index])} ${skillLevelNumeral(entries[pendingRemoval.index].level)}?` : ""}
      >
        {pendingRemoval
          ? `${pendingRemoval.dependents.map((index) => label(nameOf(entries[index]), entries[index].level)).join(", ")} ${
              pendingRemoval.dependents.length === 1 ? "needs" : "need"
            } it, so ${pendingRemoval.dependents.length === 1 ? "it is" : "they are"} removed too.`
          : null}
      </ConfirmDialog>
      <ConfirmDialog
        confirmLabel="Delete plan"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          void removePlan();
        }}
        open={confirmDelete}
        title={`Delete ${plan.name}?`}
      >
        The plan is removed for every character in the workspace. Training in the game is not affected.
      </ConfirmDialog>
    </div>
  );
}
