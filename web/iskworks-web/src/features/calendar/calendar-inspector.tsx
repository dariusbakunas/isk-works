import { useEffect, useState } from "react";
import { Link } from "react-router";
import type { CalendarMilestone } from "../../api/calendar";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { EveTypeImage } from "../../components/eve-type-image";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { formatDateLabel, formatTime, type CalendarTimezone } from "./calendar-dates";
import { presentCalendarMilestone } from "./calendar-presentation";

export type CalendarInspectorSelection =
  | { kind: "milestone"; milestone: CalendarMilestone }
  | { kind: "day"; dateKey: string; milestones: CalendarMilestone[] };

export function CalendarInspector({ onClose, returnFocusSelector, selection, timezone }: {
  onClose: () => void;
  returnFocusSelector?: string;
  selection: CalendarInspectorSelection;
  timezone: CalendarTimezone;
}) {
  const [drilledMilestone, setDrilledMilestone] = useState<CalendarMilestone | null>(null);
  useEffect(() => setDrilledMilestone(null), [selection]);
  const grouped = selection.kind === "day" ? selection : null;
  const milestone = selection.kind === "milestone" ? selection.milestone : drilledMilestone;
  const groupTitle = grouped ? formatDateLabel(grouped.dateKey) : null;

  return (
    <PlannerInspectorShell
      backLabel={groupTitle ? `Back to ${groupTitle}` : undefined}
      closeLabel="Close calendar inspector"
      dismissLabel="Dismiss calendar inspector"
      eyebrow={milestone ? presentCalendarMilestone(milestone).kindLabel : "Day milestones"}
      onBack={milestone && grouped ? () => setDrilledMilestone(null) : undefined}
      onClose={onClose}
      open
      returnFocusSelector={returnFocusSelector}
      title={milestone?.title ?? groupTitle ?? "Calendar"}
    >
      {milestone ? <MilestoneBody milestone={milestone} timezone={timezone} /> : grouped ? (
        <GroupedDayBody dateKey={grouped.dateKey} milestones={grouped.milestones} onSelect={setDrilledMilestone} timezone={timezone} />
      ) : null}
    </PlannerInspectorShell>
  );
}

function CharacterHeader({ milestone, timezone }: { milestone: CalendarMilestone; timezone: CalendarTimezone }) {
  return (
    <div className="flex items-center gap-2 border-b border-border px-3 pb-3">
      <EveCharacterPortrait characterId={milestone.eveCharacterId} characterName={milestone.characterName} size={32} />
      <div className="min-w-0"><p className="truncate text-xs font-semibold">{milestone.characterName}</p><p className="text-[0.625rem] text-muted">{formatTime(milestone.occursAt, timezone)}</p></div>
    </div>
  );
}

function MilestoneBody({ milestone, timezone }: { milestone: CalendarMilestone; timezone: CalendarTimezone }) {
  return (
    <div className="space-y-3 pb-4">
      <CharacterHeader milestone={milestone} timezone={timezone} />
      {milestone.kind === "industry" ? (
        <IndustryBody milestone={milestone} timezone={timezone} />
      ) : milestone.kind === "skill" ? (
        <SkillBody milestone={milestone} timezone={timezone} />
      ) : (
        <PlanetaryBody milestone={milestone} timezone={timezone} />
      )}
    </div>
  );
}

function IndustryBody({ milestone, timezone }: { milestone: Extract<CalendarMilestone, { kind: "industry" }>; timezone: CalendarTimezone }) {
  const imageTypeId = milestone.productTypeId ?? milestone.blueprintTypeId;
  const imageName = milestone.typeName ?? milestone.title;
  return (
    <div className="space-y-3 px-3">
      <div className="flex items-center gap-3">
        <EveTypeImage size={48} typeId={imageTypeId} typeName={imageName} />
        <div><p className="text-sm font-semibold">{milestone.title}</p><p className="text-xs text-muted">{presentCalendarMilestone(milestone).activityLabel} · {milestone.status}</p></div>
      </div>
      <dl className="grid grid-cols-2 gap-x-3 gap-y-2 text-xs">
        <Fact label="Completes" value={formatTime(milestone.occursAt, timezone)} />
        <Fact label="Runs" value={`${milestone.runs} runs`} />
        {milestone.facilityName ? <Fact label="Facility" value={milestone.facilityName} /> : null}
        {milestone.solarSystemName ? <Fact label="System" value={milestone.solarSystemName} /> : null}
        {milestone.blueprintName ? <Fact label="Blueprint" value={milestone.blueprintName} /> : null}
        {milestone.productName ? <Fact label="Product" value={milestone.productName} /> : null}
      </dl>
    </div>
  );
}

function SkillBody({ milestone, timezone }: { milestone: Extract<CalendarMilestone, { kind: "skill" }>; timezone: CalendarTimezone }) {
  return (
    <div className="space-y-3 px-3 text-xs">
      <p className="rounded border border-border bg-panel-strong p-2 font-semibold text-muted">Current queue projection</p>
      <dl className="grid grid-cols-2 gap-x-3 gap-y-2">
        <Fact label="Projected finish" value={formatTime(milestone.occursAt, timezone)} />
        <Fact label="Target" value={`Level ${["0", "I", "II", "III", "IV", "V"][milestone.targetLevel]}`} />
        <Fact label="Queue" value={`Queue position ${milestone.queuePosition + 1}`} />
        {milestone.nextSkillName ? <Fact label="Next skill" value={milestone.nextSkillName} /> : null}
      </dl>
    </div>
  );
}

function PlanetaryBody({ milestone, timezone }: { milestone: Extract<CalendarMilestone, { kind: "planetary" }>; timezone: CalendarTimezone }) {
  const imageTypeId = milestone.event === "extractorExpiry" ? milestone.products[0]?.typeId : milestone.typeId;
  return (
    <div className="space-y-3 px-3 text-xs">
      <div className="flex items-center gap-3">
        {imageTypeId ? <EveTypeImage size={48} typeId={imageTypeId} typeName={milestone.title} /> : null}
        <div>
          <p className="text-sm font-semibold">{milestone.planetName}</p>
          <p className="text-muted">{presentCalendarMilestone(milestone).activityLabel}</p>
        </div>
      </div>
      {milestone.estimated ? (
        <p className="rounded border border-border bg-panel-strong p-2 font-semibold text-muted">
          Projected from the colony as last seen in game; opening it in EVE refreshes the estimate.
        </p>
      ) : null}
      <dl className="grid grid-cols-2 gap-x-3 gap-y-2">
        <Fact label={milestone.event === "extractorExpiry" ? "Expires" : "Runs out"} value={formatTime(milestone.occursAt, timezone)} />
        {milestone.solarSystemName ? <Fact label="System" value={milestone.solarSystemName} /> : null}
        {milestone.event === "extractorExpiry" ? (
          <>
            <Fact label="Extractors" value={String(milestone.extractorCount)} />
            <Fact label="Extracting" value={milestone.products.map((product) => product.name).join(", ") || "—"} />
          </>
        ) : (
          <>
            <Fact label="Input" value={milestone.typeName} />
            <Fact label="Consumption" value={`${Number(milestone.qtyPerHour).toLocaleString("en-US")}/h`} />
          </>
        )}
      </dl>
      <Link className="iw-button-secondary inline-flex" to="/planetary">Open Planetary</Link>
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-[0.625rem] font-semibold uppercase text-muted">{label}</dt><dd className="mt-0.5 text-foreground">{value}</dd></div>;
}

function GroupedDayBody({ dateKey, milestones, onSelect, timezone }: {
  dateKey: string;
  milestones: CalendarMilestone[];
  onSelect: (milestone: CalendarMilestone) => void;
  timezone: CalendarTimezone;
}) {
  const industry = milestones.filter((milestone) => milestone.kind === "industry").length;
  const skill = milestones.filter((milestone) => milestone.kind === "skill").length;
  const planetary = milestones.filter((milestone) => milestone.kind === "planetary").length;
  return (
    <div className="space-y-2 px-3 pb-4">
      <p className="text-xs font-semibold text-muted">{industry} Industry · {skill} Skill · {planetary} Planetary</p>
      <div className="divide-y divide-border rounded border border-border">
        {milestones.map((milestone) => (
          <button aria-label={`Open ${milestone.title}`} className="flex w-full items-center gap-2 p-2 text-left hover:bg-panel-strong" key={milestone.id} onClick={() => onSelect(milestone)} type="button">
            <EveCharacterPortrait characterId={milestone.eveCharacterId} characterName={milestone.characterName} size={32} />
            <span className="min-w-0 flex-1"><span className="block truncate text-xs font-semibold">{milestone.title}</span><span className="text-[0.625rem] text-muted">{formatTime(milestone.occursAt, timezone)} · {milestone.characterName}</span></span>
          </button>
        ))}
      </div>
      <span className="sr-only">{formatDateLabel(dateKey)}</span>
    </div>
  );
}
