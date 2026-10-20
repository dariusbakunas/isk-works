import { AlertTriangle, RefreshCw, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";

import type { IndustryActivity } from "../../api/characters";
import { beginAuthorization } from "../../api/esi";
import type { JobCharacter, JobSlots, JobView, SlotUsage } from "../../api/jobs";
import { OperationalTable, OperationalTableGroup, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { EmptyState, InlineAlert, Panel, ProgressBar } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { industryJobLocation, industryJobProgress } from "../characters/industry-jobs";
import { KpiCard } from "../../components/kpi-card";
import { ActivityChip, CostCell, JobTitle, StatusChip, TicketChip } from "./job-cells";
import { JobInspector } from "./job-inspector";
import { activityLabel, ago, characterById, JOB_ACTIVITIES } from "./jobs-format";
import { useActiveJobs, useNow } from "./use-jobs";

type GroupBy = "character" | "activity" | "facility" | "none";

const GROUP_OPTIONS: Array<{ id: GroupBy; label: string }> = [
  { id: "character", label: "Character" },
  { id: "activity", label: "Activity" },
  { id: "facility", label: "Facility" },
  { id: "none", label: "None" },
];

const COLUMNS: OperationalColumn[] = [
  { key: "status", label: "Status", width: "5rem" },
  { key: "activity", label: "Activity", width: "8rem", hideBelow: "tablet" },
  { key: "job", label: "Blueprint → Product", width: "minmax(12rem, 1fr)", sticky: true },
  { key: "runs", label: "Runs", width: "3.5rem", align: "right", numeric: true },
  { key: "facility", label: "Facility", width: "minmax(8rem, 14rem)", hideBelow: "desktop" },
  { key: "ends", label: "Ends", width: "7.5rem" },
  { key: "cost", label: "Install cost", width: "6rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "ticket", label: "Ticket", width: "5.5rem" },
];

function slotText(slots: JobSlots): string {
  const part = (usage: SlotUsage, label: string) => `${usage.used}/${usage.max ?? "?"} ${label}`;
  return [part(slots.manufacturing, "mfg"), part(slots.reaction, "rxn"), part(slots.research, "rsc")].join(" · ");
}

function SlotBar({ label, usage }: { label: string; usage: SlotUsage }) {
  const percent = usage.max ? (usage.used / usage.max) * 100 : 0;
  return (
    <div className="min-w-0 flex-1" data-testid="slot-bar">
      <div className="flex items-baseline justify-between gap-2 text-[0.625rem] uppercase tracking-wider text-muted">
        <span>{label}</span>
        <span className="font-mono text-xs text-foreground">{usage.used}/{usage.max ?? "?"}</span>
      </div>
      <div className="mt-1"><ProgressBar percent={percent} tone={percent >= 90 ? "warning" : "primary"} /></div>
    </div>
  );
}

function EndsCell({ job, nowMs }: { job: JobView; nowMs: number }) {
  const progress = industryJobProgress(job, nowMs);
  if (progress.state === "ready") return <span className="text-xs font-semibold text-positive">Ready</span>;
  return (
    <span className="flex w-full flex-col gap-1">
      <span className={`font-mono text-xs ${progress.state === "paused" ? "text-warning" : ""}`}>{progress.remainingLabel}</span>
      <ProgressBar percent={progress.percent} tone={progress.state === "paused" ? "warning" : "primary"} />
    </span>
  );
}

interface Group {
  key: string;
  label: string;
  labelCase: "upper" | "normal";
  labelPrivate: boolean;
  summary?: string;
  jobs: JobView[];
}

function groupJobs(jobs: JobView[], characters: JobCharacter[], groupBy: GroupBy): Group[] {
  if (groupBy === "none") return [{ key: "all", label: "All jobs", labelCase: "upper", labelPrivate: false, jobs }];
  if (groupBy === "character") {
    return characters
      .map((character) => ({
        key: character.connectionId,
        label: character.characterName,
        labelCase: "normal" as const,
        labelPrivate: true,
        summary: slotText(character.slots),
        jobs: jobs.filter((job) => job.connectionId === character.connectionId),
      }))
      .filter((group) => group.jobs.length > 0);
  }
  if (groupBy === "activity") {
    return JOB_ACTIVITIES.map((activity) => ({
      key: activity,
      label: activityLabel(activity),
      labelCase: "upper" as const,
      labelPrivate: false,
      jobs: jobs.filter((job) => job.activity === activity),
    })).filter((group) => group.jobs.length > 0);
  }
  const byFacility = new Map<string, JobView[]>();
  for (const job of jobs) {
    const label = industryJobLocation(job);
    byFacility.set(label, [...(byFacility.get(label) ?? []), job]);
  }
  return [...byFacility.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([label, groupJobs]) => ({ key: label, label, labelCase: "normal" as const, labelPrivate: false, jobs: groupJobs }));
}

function matchesSearch(job: JobView, search: string): boolean {
  const needle = search.trim().toLowerCase();
  if (!needle) return true;
  return [job.blueprintName, job.productName].some((name) => name?.toLowerCase().includes(needle));
}

function ActiveSkeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading jobs" className="space-y-3" role="status">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => <div className={`${block} h-16`} key={index} />)}
      </div>
      <div className="space-y-1">
        {Array.from({ length: 10 }, (_, index) => <div className={`${block} h-7 w-full`} key={index} />)}
      </div>
    </div>
  );
}

async function reauthorize() {
  const started = await beginAuthorization();
  if (!started.fixtureMode) window.location.assign(started.authorizationUrl);
}

export function ActiveJobsTab() {
  const { data, loading, error, retry } = useActiveJobs();
  const nowMs = useNow();
  const [params, setParams] = useSearchParams();
  const [search, setSearch] = useState("");
  const groupBy = (GROUP_OPTIONS.find((option) => option.id === params.get("group"))?.id ?? "character") as GroupBy;
  const activity = (params.get("activity") ?? "") as IndustryActivity | "";
  const hiddenCharacters = useMemo(() => new Set(params.getAll("hide")), [params]);
  const selectedJobId = params.has("job") ? Number(params.get("job")) : null;

  function update(change: (next: URLSearchParams) => void) {
    const next = new URLSearchParams(params);
    change(next);
    setParams(next, { replace: true });
  }

  const characters = data?.characters ?? [];
  const byId = useMemo(() => characterById(characters), [characters]);
  const visible = useMemo(
    () =>
      (data?.jobs ?? []).filter(
        (job) =>
          !hiddenCharacters.has(job.connectionId) &&
          (activity === "" || job.activity === activity) &&
          matchesSearch(job, search),
      ),
    [data, hiddenCharacters, activity, search],
  );
  const groups = useMemo(() => groupJobs(visible, characters, groupBy), [visible, characters, groupBy]);
  const selected = data?.jobs.find((job) => job.jobId === selectedJobId) ?? null;
  const activitiesPresent = JOB_ACTIVITIES.filter((value) => data?.jobs.some((job) => job.activity === value));

  if (loading && !data) return <ActiveSkeleton />;
  if (error && !data) {
    return (
      <InlineAlert title="Jobs unavailable">
        {error.message} <button className="underline" onClick={retry} type="button">Retry</button>
      </InlineAlert>
    );
  }
  if (!data) return null;
  if (characters.length === 0) {
    return (
      <Panel>
        <EmptyState action={<Link className="iw-button-primary" to="/characters">Connect a character</Link>} title="No characters connected">
          Connect an EVE character with the industry jobs permission to see its jobs here.
        </EmptyState>
      </Panel>
    );
  }

  const stale = characters.filter((character) => character.syncState === "stale" || character.syncState === "failed");
  const missingScope = characters.filter((character) => character.syncState === "missingScope");

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-end gap-2 text-xs text-muted">
        <span className={`h-1.5 w-1.5 rounded-full ${stale.length > 0 ? "bg-warning" : "bg-positive"}`} />
        <span>Synced {ago(data.syncedAt, nowMs)}</span>
        <button aria-label="Refresh jobs" className="hover:text-foreground" disabled={loading} onClick={retry} type="button">
          <RefreshCw aria-hidden="true" className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
        </button>
      </div>
      {stale.length > 0 ? (
        <InlineAlert title={`${stale.length} character${stale.length === 1 ? " hasn't" : "s haven't"} synced in over an hour`} tone="warning">
          Their jobs may be out of date. ISK Works retries automatically.
        </InlineAlert>
      ) : null}
      {missingScope.map((character) => (
        <div className="flex items-center justify-between gap-2 rounded-md border border-warning/60 bg-warning/10 px-3 py-2 text-sm" key={character.connectionId} role="status">
          <span className="flex items-center gap-2 text-warning">
            <AlertTriangle aria-hidden="true" className="h-4 w-4" />
            <span><CharacterName className="font-semibold" name={character.characterName} /> is missing the industry jobs permission.</span>
          </span>
          <button className="iw-button-secondary" onClick={() => void reauthorize()} type="button">Re-authorize</button>
        </div>
      ))}

      <div className="grid grid-cols-2 gap-2 lg:grid-cols-[repeat(3,minmax(0,10rem))_1fr]">
        <KpiCard detail="across all characters" label="Running" tone="primary" value={String(data.summary.running)} />
        <KpiCard detail="deliver in game" label="Ready to deliver" tone={data.summary.ready > 0 ? "positive" : "muted"} value={String(data.summary.ready)} />
        <KpiCard detail="active jobs" label="Ending in 24h" tone="warning" value={String(data.summary.endingWithinDay)} />
        <div className="iw-panel col-span-2 flex flex-col gap-2 px-3 py-2 sm:flex-row sm:items-center sm:gap-4 lg:col-span-1">
          <SlotBar label="Manufacturing" usage={data.slots.manufacturing} />
          <SlotBar label="Reactions" usage={data.slots.reaction} />
          <SlotBar label="Research" usage={data.slots.research} />
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <label className="relative">
            <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
            <span className="sr-only">Search jobs</span>
            <input className="iw-input w-52 pl-8" onChange={(event) => setSearch(event.target.value)} placeholder="Blueprint or product" type="search" value={search} />
          </label>
          <div aria-label="Characters" className="flex flex-wrap gap-1" role="group">
            {characters.map((character) => {
              const shown = !hiddenCharacters.has(character.connectionId);
              return (
                <button
                  aria-pressed={shown}
                  className={`rounded-[2px] border px-2 py-1 text-xs ${shown ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-muted"}`}
                  key={character.connectionId}
                  onClick={() =>
                    update((next) => {
                      const hidden = new Set(next.getAll("hide"));
                      if (hidden.has(character.connectionId)) hidden.delete(character.connectionId);
                      else hidden.add(character.connectionId);
                      next.delete("hide");
                      for (const id of hidden) next.append("hide", id);
                    })
                  }
                  type="button"
                >
                  <CharacterName name={character.characterName} />
                </button>
              );
            })}
          </div>
          <label>
            <span className="sr-only">Activity</span>
            <select className="iw-input" onChange={(event) => update((next) => (event.target.value ? next.set("activity", event.target.value) : next.delete("activity")))} value={activity}>
              <option value="">All activities</option>
              {activitiesPresent.map((value) => <option key={value} value={value}>{activityLabel(value)}</option>)}
            </select>
          </label>
        </div>
        <div aria-label="Group by" className="flex items-center gap-1" role="group">
          <span className="mr-1 text-[0.625rem] font-semibold uppercase tracking-wider text-muted">Group</span>
          {GROUP_OPTIONS.map((option) => (
            <button
              aria-pressed={groupBy === option.id}
              className={`rounded-[2px] border px-2 py-0.5 text-xs ${groupBy === option.id ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted hover:text-foreground"}`}
              key={option.id}
              onClick={() => update((next) => (option.id === "character" ? next.delete("group") : next.set("group", option.id)))}
              type="button"
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      {data.jobs.length === 0 ? (
        <Panel>
          <EmptyState title="All slots idle">
            {characters.map((character) => (
              <span className="block" key={character.connectionId}>
                <CharacterName name={character.characterName} />: {slotText(character.slots)}
              </span>
            ))}
          </EmptyState>
        </Panel>
      ) : visible.length === 0 ? (
        <Panel>
          <EmptyState title="No matching jobs">No jobs match the current search and filters.</EmptyState>
        </Panel>
      ) : (
        <OperationalTable
          ariaLabel="Active jobs"
          columns={COLUMNS}
          onSelectRow={(rowKey) => update((next) => next.set("job", rowKey))}
          selectedRowKey={selectedJobId === null ? null : String(selectedJobId)}
        >
          {groups.map((group) => (
            <OperationalTableGroup
              groupKey={group.key}
              itemCount={group.jobs.length}
              key={group.key}
              label={group.label}
              labelCase={group.labelCase}
              labelPrivate={group.labelPrivate}
              summary={group.summary ? <span className="font-mono text-[0.625rem]">{group.summary}</span> : undefined}
            >
              {group.jobs.map((job) => (
                <OperationalTableRow
                  cells={{
                    status: <StatusChip status={job.status} />,
                    activity: <ActivityChip activity={job.activity} />,
                    job: <JobTitle job={job} />,
                    runs: job.runs.toLocaleString("en-US"),
                    facility: <span className="truncate text-xs text-muted">{industryJobLocation(job)}</span>,
                    ends: <EndsCell job={job} nowMs={nowMs} />,
                    cost: <CostCell cost={job.cost} />,
                    ticket: <TicketChip ticket={job.ticket} />,
                  }}
                  key={job.jobId}
                  rowKey={String(job.jobId)}
                />
              ))}
            </OperationalTableGroup>
          ))}
        </OperationalTable>
      )}
      {selected ? (
        <JobInspector
          character={byId.get(selected.connectionId)}
          job={selected}
          nowMs={nowMs}
          onClose={() => update((next) => next.delete("job"))}
        />
      ) : null}
    </div>
  );
}

