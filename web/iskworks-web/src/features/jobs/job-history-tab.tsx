import { useMemo } from "react";
import { useSearchParams } from "react-router";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import type { IndustryActivity } from "../../api/characters";
import type { JobHistory } from "../../api/jobs";
import { formatIskAbbreviated, formatIskSummary } from "../../components/money";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { EmptyState, InlineAlert, Panel } from "../../components/primitives";
import { KpiCard } from "../../components/kpi-card";
import { industryJobLocation } from "../characters/industry-jobs";
import { ActivityChip, CharacterChip, CostCell, JobTitle, StatusChip, TicketChip } from "./job-cells";
import { activityColor, activityLabel, characterById, daysBefore, JOB_ACTIVITIES, shortDate, utcDay } from "./jobs-format";
import { useJobHistory, useNow } from "./use-jobs";

const RANGES = [
  { id: "7d", label: "7d", days: 7 },
  { id: "30d", label: "30d", days: 30 },
  { id: "90d", label: "90d", days: 90 },
] as const;

type RangeId = (typeof RANGES)[number]["id"];

const COLUMNS: OperationalColumn[] = [
  { key: "completed", label: "Completed", width: "5.5rem", numeric: true },
  { key: "character", label: "Character", width: "minmax(7rem, 10rem)", hideBelow: "tablet" },
  { key: "activity", label: "Activity", width: "4.5rem" },
  { key: "job", label: "Blueprint → Product", width: "minmax(13rem, 1fr)", sticky: true },
  { key: "runs", label: "Runs", width: "5rem", align: "right", numeric: true },
  { key: "facility", label: "Facility", width: "minmax(9rem, 14rem)", hideBelow: "desktop" },
  { key: "cost", label: "Install cost", width: "6.5rem", align: "right", numeric: true },
  { key: "outcome", label: "Outcome", width: "6rem" },
  { key: "ticket", label: "Ticket", width: "6rem" },
];

const AXIS_TICK = { fill: "var(--color-muted)", fontSize: 10, fontFamily: "var(--font-mono)" };

type WeekRow = { label: string } & Partial<Record<IndustryActivity, number>>;

function WeeklyChart({ history }: { history: JobHistory }) {
  const rows: WeekRow[] = history.weekly.map((week) => {
    const row: WeekRow = { label: shortDate(`${week.weekStart}T00:00:00Z`) };
    for (const value of week.byActivity) row[value.activity] = value.count;
    return row;
  });
  const activities = JOB_ACTIVITIES.filter((activity) =>
    history.weekly.some((week) => week.byActivity.some((value) => value.activity === activity)),
  );
  return (
    <div>
      <div aria-label={`Jobs completed per week: ${history.kpis.jobsCompleted} in ${rows.length} weeks.`} className="h-48 w-full" role="img">
        <ResponsiveContainer height="100%" width="100%">
          <BarChart data={rows} margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
            <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
            <XAxis axisLine={{ stroke: "var(--color-border)" }} dataKey="label" tick={AXIS_TICK} tickLine={false} />
            <YAxis allowDecimals={false} axisLine={false} tick={AXIS_TICK} tickLine={false} width={32} />
            <Tooltip
              contentStyle={{ background: "var(--color-panel)", border: "1px solid var(--color-border)", fontSize: 11 }}
              cursor={{ fill: "var(--color-panel-strong)", opacity: 0.6 }}
              isAnimationActive={false}
            />
            {activities.map((activity) => (
              <Bar dataKey={activity} fill={activityColor(activity)} isAnimationActive={false} key={activity} maxBarSize={36} name={activityLabel(activity)} stackId="jobs" />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
      <ul aria-label="Activities" className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[0.625rem] text-muted">
        {activities.map((activity) => (
          <li className="flex items-center gap-1.5" key={activity}>
            <span aria-hidden="true" className="h-2 w-2 rounded-[1px]" style={{ background: activityColor(activity) }} />
            {activityLabel(activity)}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function JobHistoryTab() {
  const [params, setParams] = useSearchParams();
  const range: RangeId = RANGES.find((option) => option.id === params.get("range"))?.id ?? "30d";
  const nowMs = useNow(60_000);
  const to = utcDay(nowMs);
  const days = RANGES.find((option) => option.id === range)!.days;
  const { data, loading, error, retry } = useJobHistory(daysBefore(to, days - 1), to);
  const byId = useMemo(() => characterById(data?.characters ?? []), [data]);

  const kpis = data?.kpis;
  const inventionPercent =
    kpis && kpis.inventionAttemptedRuns > 0 ? Math.round((kpis.inventionSuccessfulRuns / kpis.inventionAttemptedRuns) * 100) : null;

  return (
    <div className="space-y-3">
      <div aria-label="Range" className="flex items-center justify-end gap-1" role="group">
        <span className="mr-1 text-[0.625rem] font-semibold uppercase tracking-wider text-muted">Range</span>
        {RANGES.map((option) => (
          <button
            aria-pressed={range === option.id}
            className={`rounded-[2px] border px-2 py-0.5 text-xs ${range === option.id ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted hover:text-foreground"}`}
            key={option.id}
            onClick={() => {
              const next = new URLSearchParams(params);
              if (option.id === "30d") next.delete("range");
              else next.set("range", option.id);
              setParams(next, { replace: true });
            }}
            type="button"
          >
            {option.label}
          </button>
        ))}
      </div>
      {error ? (
        <InlineAlert title="History unavailable">
          {error.message} <button className="underline" onClick={retry} type="button">Retry</button>
        </InlineAlert>
      ) : null}
      {loading && !data ? <div aria-busy="true" aria-label="Loading history" className="h-64 animate-pulse rounded-[2px] bg-panel-strong" role="status" /> : null}
      {data && kpis ? (
        <div aria-busy={loading} className={`space-y-3 ${loading ? "opacity-60" : ""}`}>
          <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
            <KpiCard detail={`last ${days} days`} label="Jobs completed" tone="primary" value={kpis.jobsCompleted.toLocaleString("en-US")} />
            <KpiCard detail="successful runs for invention" label="Runs delivered" tone="primary" value={kpis.runsDelivered.toLocaleString("en-US")} />
            <KpiCard detail="every finished job" label="Install cost" title={formatIskSummary(kpis.installCost)} tone="warning" value={formatIskAbbreviated(kpis.installCost, { currency: true })} />
            <KpiCard
              detail={`${kpis.inventionSuccessfulRuns}/${kpis.inventionAttemptedRuns} runs`}
              label="Invention success"
              tone={inventionPercent === null ? "muted" : "positive"}
              value={inventionPercent === null ? "—" : `${inventionPercent}%`}
            />
          </div>
          <section aria-label="Jobs completed per week" className="iw-panel p-3">
            <h2 className="mb-2 text-xs font-semibold">Jobs completed per week</h2>
            <WeeklyChart history={data} />
          </section>
          {data.jobs.length > 0 ? (
            <OperationalTable ariaLabel="Completed jobs" columns={COLUMNS} onSelectRow={() => undefined} selectedRowKey={null}>
              <tbody>
                {data.jobs.map((job) => (
                  <OperationalTableRow
                    cells={{
                      completed: <span className="text-xs">{shortDate(job.completedDate)}</span>,
                      character: <CharacterChip character={byId.get(job.connectionId)} />,
                      activity: <ActivityChip activity={job.activity} compact />,
                      job: <JobTitle job={job} />,
                      runs: job.activity === "invention" && job.successfulRuns !== null ? `${job.successfulRuns}/${job.runs}` : job.runs.toLocaleString("en-US"),
                      facility: <span className="truncate text-xs text-muted">{industryJobLocation(job)}</span>,
                      cost: <CostCell cost={job.cost} />,
                      outcome: <StatusChip status={job.status} />,
                      ticket: <TicketChip ticket={job.ticket} />,
                    }}
                    interactive={false}
                    key={job.jobId}
                    rowKey={String(job.jobId)}
                  />
                ))}
              </tbody>
            </OperationalTable>
          ) : (
            <Panel>
              <EmptyState title={data.earliestObservedAt ? "Nothing finished in this period" : "No history yet"}>
                {data.earliestObservedAt
                  ? "Widen the range to see earlier jobs."
                  : "History starts when ISK Works first syncs your jobs; the first sync brings in EVE's last 90 days."}
              </EmptyState>
            </Panel>
          )}
          <p className="text-[0.625rem] text-muted">
            EVE keeps finished jobs for 90 days; ISK Works keeps them from the day it first saw them
            {data.earliestObservedAt ? ` (${shortDate(data.earliestObservedAt)})` : ""}.
          </p>
        </div>
      ) : null}
    </div>
  );
}
