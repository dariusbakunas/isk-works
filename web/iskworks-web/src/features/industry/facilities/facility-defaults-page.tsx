import { Check } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import {
  assignFacilityDefaults,
  getFacilityDefaults,
  listFacilities,
  type FacilityDefaultAssignment,
  type FacilityDefaultStatus,
  type FacilityDefaultTarget,
  type FacilityDefaultsView,
  type FacilityProfile,
  type FacilityRole,
} from "../../../api/industry";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { Badge, EmptyState, InlineAlert, PageHeader, Panel, type Tone } from "../../../components/primitives";
import { apiMessage } from "../shared/api-error";
import { formatBonusPercent } from "./facility-bonus";
import { bonusFor, effectiveChoice, suggestionAssignments, targetKey } from "./facility-defaults-model";
import { FacilitiesSectionTabs } from "./facilities-section-tabs";

const COLUMNS: OperationalColumn[] = [
  { key: "target", label: "Target", width: "minmax(11rem, 1fr)" },
  { key: "facility", label: "Default facility", width: "minmax(16rem, 2fr)" },
  { key: "material", label: "ME", title: "Material bonus for this target", width: "5rem", align: "right", numeric: true },
  { key: "time", label: "TE", title: "Time bonus for this target", width: "5rem", align: "right", numeric: true },
  { key: "status", label: "Status", width: "11rem" },
];

const ROLE_GROUPS: Array<{ role: FacilityRole; label: string }> = [
  { role: "manufacturing", label: "Manufacturing" },
  { role: "reaction", label: "Reactions" },
];

const STATUS: Record<FacilityDefaultStatus, { label: string; tone: Tone; title: string }> = {
  set: { label: "Set", tone: "positive", title: "New Builds in this target use this facility." },
  coverageLost: {
    label: "Rigs no longer cover",
    tone: "warning",
    title: "This facility's rigs covered the target when it became the default, but no longer do.",
  },
  fallback: { label: "Uses fallback", tone: "muted", title: "No default; new Builds use “Everything else”." },
  none: { label: "No default", tone: "muted", title: "New Builds inherit their consumer's facility, if any." },
  unknownTarget: {
    label: "Unknown target",
    tone: "warning",
    title: "The active SDE no longer has this target. Clear it.",
  },
};

function percent(value: string): string {
  return formatBonusPercent(Number(value));
}

export function FacilityDefaultsPage() {
  const [view, setView] = useState<FacilityDefaultsView | null>(null);
  const [facilities, setFacilities] = useState<FacilityProfile[]>([]);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    Promise.all([getFacilityDefaults(), listFacilities()])
      .then(([defaults, profiles]) => {
        setView(defaults);
        setFacilities(profiles);
      })
      .catch((requestError) => setError(apiMessage(requestError)));
  }, []);

  async function assign(assignments: FacilityDefaultAssignment[]) {
    if (assignments.length === 0) return;
    setSaving(true);
    setError("");
    try {
      setView(await assignFacilityDefaults(assignments));
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setSaving(false);
    }
  }

  const pending = useMemo(() => (view ? suggestionAssignments(view) : []), [view]);
  const activeByRole = useMemo(() => {
    const active = facilities.filter((profile) => !profile.archivedAt);
    return {
      manufacturing: active.filter((profile) => profile.role === "manufacturing"),
      reaction: active.filter((profile) => profile.role === "reaction"),
    } satisfies Record<FacilityRole, FacilityProfile[]>;
  }, [facilities]);

  return (
    <>
      <PageHeader eyebrow="Industry assumptions" title="Facilities">
        Default facility per EVE rig target. New Builds take the most specific matching target's default, then “Everything else”.
      </PageHeader>
      <FacilitiesSectionTabs />
      {error ? <InlineAlert title="Facility defaults unavailable">{error}</InlineAlert> : null}
      {view === null ? (
        error ? null : (
          <p className="iw-muted mt-4 text-sm" role="status">Loading facility defaults…</p>
        )
      ) : view.targets.length === 0 ? (
        <Panel className="mt-4">
          <EmptyState title="No rig targets">Import the SDE to set defaults per rig target.</EmptyState>
        </Panel>
      ) : (
        <>
          <div className="mt-4 flex flex-wrap items-center justify-between gap-2">
            <p className="iw-muted text-xs">
              Defaults apply to new producer Builds and to new Builds created without a facility. Existing picks never change.
            </p>
            <button
              className="iw-button-secondary"
              disabled={saving || pending.length === 0}
              onClick={() => void assign(pending)}
              type="button"
            >
              <Check aria-hidden="true" className="mr-2 h-4 w-4" />
              Accept all suggestions ({pending.length})
            </button>
          </div>
          <div className="iw-panel mt-3">
            <OperationalTable ariaLabel="Facility defaults" columns={COLUMNS} onSelectRow={() => {}} selectedRowKey={null}>
              {ROLE_GROUPS.map(({ role, label }) => {
                const targets = view.targets.filter((target) => target.role === role);
                return (
                  <OperationalTableGroup groupKey={role} itemCount={targets.length} key={role} label={label}>
                    {targets.map((target) => (
                      <TargetRow
                        disabled={saving}
                        facilities={activeByRole[role]}
                        key={targetKey(role, target.filterId)}
                        onAssign={(facilityProfileId) =>
                          void assign([{ role, filterId: target.filterId, facilityProfileId }])
                        }
                        target={target}
                        view={view}
                      />
                    ))}
                  </OperationalTableGroup>
                );
              })}
            </OperationalTable>
          </div>
        </>
      )}
    </>
  );
}

function TargetRow({
  disabled,
  facilities,
  onAssign,
  target,
  view,
}: {
  disabled: boolean;
  facilities: FacilityProfile[];
  onAssign: (facilityProfileId: string | null) => void;
  target: FacilityDefaultTarget;
  view: FacilityDefaultsView;
}) {
  const effective = effectiveChoice(view, target);
  const status = STATUS[target.status];
  const isFallback = target.filterId === null;
  const label = isFallback ? `${target.name} (${target.role === "reaction" ? "reactions" : "manufacturing"})` : target.name;
  const muted = effective?.viaFallback ? "text-muted" : "";

  return (
    <OperationalTableRow
      accent={status.tone === "warning" ? "warning" : undefined}
      align="top"
      cells={{
        target: <span className={isFallback ? "italic" : undefined}>{target.name}</span>,
        facility: (
          <div className="min-w-0">
            <select
              aria-label={`Default facility for ${label}`}
              className="iw-input w-full"
              disabled={disabled}
              onChange={(event) => onAssign(event.target.value || null)}
              value={target.default?.facilityProfileId ?? ""}
            >
              <option value="">{isFallback ? "— No default" : "— Use fallback"}</option>
              {facilities.map((profile) => {
                const bonus = bonusFor(view, profile.id, target.role, target.filterId);
                return (
                  <option key={profile.id} value={profile.id}>
                    {bonus
                      ? `${profile.name} (${percent(bonus.materialBonusPercent)} / ${percent(bonus.timeBonusPercent)})`
                      : profile.name}
                  </option>
                );
              })}
            </select>
            {target.suggestion ? (
              <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-primary">
                <span>
                  Suggested: <span data-private="">{target.suggestion.facilityName}</span> (
                  {percent(target.suggestion.materialBonusPercent)} / {percent(target.suggestion.timeBonusPercent)})
                </span>
                <button
                  aria-label={`Accept suggestion for ${label}`}
                  className="iw-button-secondary h-6 px-2 text-xs"
                  disabled={disabled}
                  onClick={() => onAssign(target.suggestion!.facilityProfileId)}
                  type="button"
                >
                  Accept
                </button>
              </div>
            ) : null}
            {effective?.viaFallback ? (
              <p className="iw-muted mt-1 text-xs">
                Via fallback: <span data-private="">{effective.facilityName}</span>
              </p>
            ) : null}
          </div>
        ),
        material: <span className={muted}>{effective ? percent(effective.materialBonusPercent) : "—"}</span>,
        time: <span className={muted}>{effective ? percent(effective.timeBonusPercent) : "—"}</span>,
        status: (
          <span title={status.title}>
            <Badge tone={status.tone}>{status.label}</Badge>
          </span>
        ),
      }}
      rowKey={targetKey(target.role, target.filterId)}
      status={status.tone === "warning" ? "warning" : "neutral"}
    />
  );
}
