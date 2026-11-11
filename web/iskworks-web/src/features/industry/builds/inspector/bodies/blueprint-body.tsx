import { useEffect, useState } from "react";

import { BlueprintSelectionFields } from "../../planner/blueprint-selection-section";
import type {
  BlueprintSlice,
  InspectorActions,
} from "../inspector-model";
import { InspectorRow } from "../inspector-section";

export function BlueprintBody({
  slice,
  actions,
}: {
  slice: BlueprintSlice;
  actions: InspectorActions["blueprint"];
}) {
  const canEdit = slice.editable && Boolean(actions);
  const hasObservations = slice.observations.length > 0;
  const startExisting =
    slice.mode === "existing" ? true : slice.mode === "manual" ? false : hasObservations;
  const [mode, setMode] = useState<"manual" | "observedAsset">(
    startExisting ? "observedAsset" : "manual",
  );
  const [kind, setKind] = useState<"original" | "copy">(slice.origin === "BPC" ? "copy" : "original");
  const [me, setMe] = useState(String(slice.me ?? 0));
  const [te, setTe] = useState(String(slice.te ?? 0));
  const [runs, setRuns] = useState(slice.licensedRuns != null ? String(slice.licensedRuns) : "");
  const [notes, setNotes] = useState(slice.notes);

  // Re-seed only when the underlying selection identity changes -- not on
  // every preview-derived me/te tick, so an in-progress edit isn't clobbered.
  useEffect(() => {
    setMode(startExisting ? "observedAsset" : "manual");
    setKind(slice.origin === "BPC" ? "copy" : "original");
    setMe(String(slice.me ?? 0));
    setTe(String(slice.te ?? 0));
    setRuns(slice.licensedRuns != null ? String(slice.licensedRuns) : "");
    setNotes(slice.notes);
  }, [slice.blueprintTypeId, slice.mode, slice.selectedObservationId]);

  function commitManual(next: Partial<{ kind: "original" | "copy"; me: string; te: string; runs: string; notes: string }> = {}) {
    const k = next.kind ?? kind;
    const meValue = Number(next.me ?? me);
    const teValue = Number(next.te ?? te);
    const runsValue = next.runs ?? runs;
    if (!Number.isInteger(meValue) || meValue < 0 || meValue > 10) return;
    if (!Number.isInteger(teValue) || teValue < 0 || teValue > 20) return;
    const licensedRuns = k === "copy" ? (runsValue ? Number(runsValue) : null) : null;
    if (k === "copy" && (licensedRuns == null || !Number.isInteger(licensedRuns) || licensedRuns < 1)) return;
    actions?.onModelManually?.({
      kind: k,
      materialEfficiency: meValue,
      timeEfficiency: teValue,
      licensedRuns,
      notes: next.notes ?? notes,
    });
  }

  // The observation currently backing an `observedAsset` selection -- its own
  // ME/TE/origin/runs are the authoritative read-only values.
  const selectedObservation = slice.selectedObservationId
    ? slice.observations.find((o) => o.id === slice.selectedObservationId) ?? null
    : null;
  const derivedOrigin =
    selectedObservation != null
      ? selectedObservation.kind === "copy"
        ? "BPC"
        : "BPO"
      : slice.origin;
  const derivedMe = selectedObservation?.materialEfficiency ?? slice.me;
  const derivedTe = selectedObservation?.timeEfficiency ?? slice.te;
  const derivedRuns =
    selectedObservation != null
      ? selectedObservation.kind === "copy"
        ? selectedObservation.licensedRuns
        : null
      : slice.licensedRuns;

  if (!canEdit) {
    return (
      <>
        <InspectorRow
          label="Blueprint"
          value={
            slice.name != null ? (
              <span>
                {slice.name}
                {derivedOrigin ? <span className="ml-1.5 text-muted">· {derivedOrigin}</span> : null}
              </span>
            ) : (
              "—"
            )
          }
        />
        {derivedMe != null && derivedTe != null ? (
          <InspectorRow label="ME / TE" value={`ME ${derivedMe} · TE ${derivedTe}`} />
        ) : slice.computing ? (
          <InspectorRow label="ME / TE" value={<span className="text-muted">Computing…</span>} />
        ) : null}
        {derivedRuns != null ? (
          <InspectorRow label="Runs remaining" value={derivedRuns.toLocaleString()} />
        ) : null}
      </>
    );
  }

  // Editable: the same "Enter manually" / "Use available blueprint" control
  // as the standalone Choose Blueprint dialog, so both offer one selection UX.
  return (
    <div className="space-y-2">
      <InspectorRow label="Blueprint" value={slice.name ?? "—"} />
      <BlueprintSelectionFields
        kind={kind}
        licensedRuns={runs}
        me={me}
        mode={mode}
        notes={notes}
        observations={slice.observations}
        onCommit={() => commitManual()}
        onKind={(next) => {
          setKind(next);
          commitManual({ kind: next });
        }}
        onLicensedRuns={setRuns}
        onMe={setMe}
        onMode={setMode}
        onNotes={setNotes}
        onObservation={(id) => actions?.onSelectObservation?.(id)}
        onTe={setTe}
        requiredRuns={slice.requiredRuns}
        selectedObservationId={slice.selectedObservationId ?? ""}
        te={te}
      />
      {mode === "observedAsset" && slice.selectedObservationId && selectedObservation == null ? (
        // The chosen blueprint isn't in the loaded list -- still state the
        // ME/TE the plan resolved from it.
        <>
          {derivedMe != null && derivedTe != null ? (
            <InspectorRow label="ME / TE" value={`ME ${derivedMe} · TE ${derivedTe}`} />
          ) : null}
          {derivedRuns != null ? (
            <InspectorRow label="Runs remaining" value={derivedRuns.toLocaleString()} />
          ) : null}
        </>
      ) : null}
      {actions?.error ? <p className="text-[11px] text-danger">{actions.error}</p> : null}
    </div>
  );
}
