// Stages inspector editor for a producer's descendant facility/blueprint configuration.

import { useEffect, useState } from "react";

import type {
  BlueprintObservation,
  BlueprintSelection,
  DescendantConfigurationMember,
  DescendantProductionConfigurationRequest,
  ExecutionNode,
  ExecutionOccurrence,
  PreviewBuildPlanCommand,
} from "../../../../api/industry";
import { listBlueprintObservations, updateDescendantProductionConfiguration } from "../../../../api/industry";
import type { FacilityProfile } from "../../../../api/industry/facilities";
import { apiMessage } from "../../shared/api-error";
import { BlueprintSelectionFields } from "../planner/blueprint-selection-section";
import { InspectorRow } from "../../inspector/inspector-section";



import { ConfigurationWarning, MissingFacilityWarning } from "./stages-inspector-parts";

/** Descendant production configuration -- the producer's own
 * configuration (blueprint + ME/TE/kind/licensed runs or an observed
 * blueprint asset for manufacturing; the formula, read-only, for a
 * reaction; the facility for both), written through the existing
 * `PATCH descendant-production-configuration` path. Applies the identical
 * patch to every Build in `members` atomically, then hands control back to
 * the host to re-project the plan -- this component never computes or
 * locally patches a quantity, run count, or cost. Missing configuration
 * (no facility, no resolvable blueprint/formula) is flagged next to the
 * field it concerns. */
export function DescendantConfigurationEditor({
  rootBuildId,
  command,
  members,
  activity,
  currentFacilityId,
  currentFacilityName,
  blueprintOrFormulaName,
  blueprintSelection,
  effectiveMe = null,
  effectiveTe = null,
  requiredRuns,
  facilities,
  onSaved,
}: {
  rootBuildId: string;
  command: PreviewBuildPlanCommand;
  members: ExecutionOccurrence[];
  activity: ExecutionNode["activity"];
  currentFacilityId: string | null;
  currentFacilityName: string | null;
  blueprintOrFormulaName: string | null;
  blueprintSelection: BlueprintSelection | null;
  effectiveMe?: number | null;
  effectiveTe?: number | null;
  requiredRuns: number;
  facilities: FacilityProfile[];
  onSaved: () => void;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [observations, setObservations] = useState<BlueprintObservation[]>([]);

  const isManufacturing = activity === "manufacturing";
  const blueprintTypeId = members[0]?.blueprintOrFormulaTypeId ?? null;

  // Owned blueprint instances for this Manufacturing operation's own
  // blueprint, so "Use available blueprint" can offer them -- mirrors
  // Graph's own fetch exactly (same endpoint, same trigger: the selected
  // node's blueprint type id).
  useEffect(() => {
    if (!isManufacturing || blueprintTypeId == null) {
      setObservations([]);
      return;
    }
    let cancelled = false;
    listBlueprintObservations(blueprintTypeId)
      .then((rows) => {
        if (!cancelled) setObservations(rows);
      })
      .catch(() => {
        if (!cancelled) setObservations([]);
      });
    return () => {
      cancelled = true;
    };
  }, [isManufacturing, blueprintTypeId]);

  const manual = blueprintSelection?.mode === "manual" ? blueprintSelection : null;
  const observedAsset = blueprintSelection?.mode === "observedAsset" ? blueprintSelection : null;
  const observedKind: "original" | "copy" =
    observedAsset?.kind && observedAsset.kind !== "unknown" ? observedAsset.kind : "original";

  const [mode, setMode] = useState<"manual" | "observedAsset">(
    observedAsset ? "observedAsset" : "manual",
  );
  const [kind, setKind] = useState<"original" | "copy">(manual?.kind ?? observedKind);
  const [me, setMe] = useState(String(manual?.materialEfficiency ?? 0));
  const [te, setTe] = useState(String(manual?.timeEfficiency ?? 0));
  const [licensedRuns, setLicensedRuns] = useState(
    manual?.licensedRuns != null ? String(manual.licensedRuns) : "",
  );
  const [notes, setNotes] = useState(manual?.notes ?? "");
  const [selectedObservationId, setSelectedObservationId] = useState(
    observedAsset?.observationId ?? "",
  );

  // Re-seed only when the underlying selection identity changes -- not on
  // every render -- so an in-progress edit isn't clobbered by the same
  // plan re-rendering. Mirrors `BlueprintBody`'s own re-seed effect.
  useEffect(() => {
    setMode(observedAsset ? "observedAsset" : "manual");
    setKind(manual?.kind ?? observedKind);
    setMe(String(manual?.materialEfficiency ?? 0));
    setTe(String(manual?.timeEfficiency ?? 0));
    setLicensedRuns(manual?.licensedRuns != null ? String(manual.licensedRuns) : "");
    setNotes(manual?.notes ?? "");
    setSelectedObservationId(observedAsset?.observationId ?? "");
  }, [blueprintTypeId, blueprintSelection?.mode, observedAsset?.observationId]);

  if (members.length === 0) {
    return <InspectorRow label="Facility" value={currentFacilityName ?? "No facility"} />;
  }

  async function submit(request: DescendantProductionConfigurationRequest) {
    setSaving(true);
    setError(null);
    try {
      await updateDescendantProductionConfiguration(rootBuildId, {
        command,
        members: members.map(
          (member): DescendantConfigurationMember => ({
            buildId: member.buildId,
            expectedRevision: member.revision,
          }),
        ),
        request,
      });
      onSaved();
    } catch (caught) {
      setError(apiMessage(caught));
    } finally {
      // The inspector stays open on the same producer after a save (the
      // host re-projects the plan underneath it), so always re-enable.
      setSaving(false);
    }
  }

  async function handleFacilityChange(facilityProfileId: string) {
    await submit({ kind: "facility", facilityProfileId: facilityProfileId || null });
  }

  // Same manual ME/TE/kind/licensed-runs validation `BlueprintBody` already
  // uses -- domain rules live here once, never reimplemented in React.
  function commitManual(
    next: Partial<{ kind: "original" | "copy"; me: string; te: string; runs: string; notes: string }> = {},
  ) {
    const k = next.kind ?? kind;
    const meValue = Number(next.me ?? me);
    const teValue = Number(next.te ?? te);
    const runsValue = next.runs ?? licensedRuns;
    if (!Number.isInteger(meValue) || meValue < 0 || meValue > 10) return;
    if (!Number.isInteger(teValue) || teValue < 0 || teValue > 20) return;
    const licensedRunsValue = k === "copy" ? (runsValue ? Number(runsValue) : null) : null;
    if (k === "copy" && (licensedRunsValue == null || !Number.isInteger(licensedRunsValue) || licensedRunsValue < 1)) {
      return;
    }
    void submit({
      kind: "blueprintSelection",
      blueprintSelection: {
        mode: "manual",
        kind: k,
        materialEfficiency: meValue,
        timeEfficiency: teValue,
        licensedRuns: licensedRunsValue,
        notes: next.notes ?? notes,
      },
    });
  }

  return (
    <div className="space-y-2">
      {isManufacturing ? (
        <div className="space-y-1.5">
          <InspectorRow label="Blueprint" value={blueprintOrFormulaName || "No blueprint"} />
          {!blueprintOrFormulaName ? (
            <ConfigurationWarning>
              No published blueprint resolved for this operation -- quantities and cost are incomplete.
            </ConfigurationWarning>
          ) : null}
          <div className="grid grid-cols-2 gap-x-3">
            <InspectorRow label="ME" value={effectiveMe ?? "—"} />
            <InspectorRow label="TE" value={effectiveTe ?? "—"} />
          </div>
          <fieldset className="space-y-1.5" disabled={saving}>
            <BlueprintSelectionFields
              kind={kind}
              licensedRuns={licensedRuns}
              me={me}
              mode={mode}
              notes={notes}
              observations={observations}
              onCommit={() => commitManual()}
              onKind={(next) => {
                setKind(next);
                commitManual({ kind: next });
              }}
              onLicensedRuns={setLicensedRuns}
              onMe={setMe}
              onMode={setMode}
              onNotes={setNotes}
              onObservation={(id) => {
                setSelectedObservationId(id);
                void submit({
                  kind: "blueprintSelection",
                  blueprintSelection: { mode: "observedAsset", observationId: id },
                });
              }}
              onTe={setTe}
              requiredRuns={requiredRuns}
              selectedObservationId={selectedObservationId}
              te={te}
            />
          </fieldset>
        </div>
      ) : (
        <>
          {/* A reaction has exactly one formula per output in the SDE, so it
              is shown, not chosen. */}
          <InspectorRow label="Reaction formula" value={blueprintOrFormulaName || "No formula"} />
          {!blueprintOrFormulaName ? (
            <ConfigurationWarning>
              No reaction formula resolved for this operation -- quantities and cost are incomplete.
            </ConfigurationWarning>
          ) : null}
        </>
      )}

      <div className="space-y-1.5 border-t border-border pt-2">
        <label className="block text-xs font-semibold">
          Facility
          <select
            aria-label="Facility"
            className="iw-input mt-1"
            disabled={saving}
            onChange={(event) => void handleFacilityChange(event.target.value)}
            value={currentFacilityId ?? ""}
          >
            <option value="">No facility</option>
            {facilities
              .filter((profile) => !profile.archivedAt && profile.role === activity)
              .map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
          </select>
        </label>
        {!currentFacilityId ? <MissingFacilityWarning /> : null}
      </div>

      {saving ? <p className="text-[11px] text-muted">Saving...</p> : null}
      {error ? <p className="text-[11px] text-danger">{error}</p> : null}
    </div>
  );
}
