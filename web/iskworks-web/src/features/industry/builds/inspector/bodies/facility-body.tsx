import { FacilityEivControl } from "../../components/planner-panels";
import type {
  FacilitySlice,
  InspectorActions,
} from "../inspector-model";
import { InspectorRow } from "../inspector-section";

export function FacilityBody({
  slice,
  actions,
}: {
  slice: FacilitySlice;
  actions: InspectorActions["facility"];
}) {
  if (!slice.editable || !actions) {
    return (
      <>
        <InspectorRow
          label="Facility"
          value={slice.name ?? <span className="text-muted">{slice.summary}</span>}
        />
        {slice.location ? <InspectorRow label="System" value={slice.location} /> : null}
        {slice.bonuses ? <InspectorRow label="Bonuses" value={slice.bonuses} /> : null}
        {slice.rigCount > 0 ? <InspectorRow label="Rigs" value={`${slice.rigCount} installed`} /> : null}
      </>
    );
  }
  const isRoot = slice.eiv !== undefined;
  const facilityLabel = isRoot
    ? slice.role === "reaction"
      ? "Reaction Facility"
      : "Manufacturing Facility"
    : "Facility";
  return (
    <div className="space-y-1.5">
      {slice.state !== "set" && !isRoot ? (
        <InspectorRow label="Resolved facility" value={<span className="text-muted">{slice.summary}</span>} />
      ) : null}
      <label className={`block text-xs font-semibold ${slice.state !== "set" && isRoot ? "text-warning" : ""}`}>
        {facilityLabel}
        <select
          aria-label={facilityLabel}
          className="iw-input mt-1"
          disabled={actions.pending}
          onChange={(event) => actions.onSelect?.(event.target.value || null)}
          value={slice.selectedFacilityId ?? ""}
        >
          <option value="">{isRoot ? "Not selected" : "Use build facility"}</option>
          {actions.options
            .filter((profile) => !profile.archivedAt && profile.role === slice.role)
            .map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
        </select>
      </label>
      {slice.location ? <InspectorRow label="System" value={slice.location} /> : null}
      {slice.bonuses ? <InspectorRow label="Bonuses" value={slice.bonuses} /> : null}
      {slice.rigCount > 0 ? <InspectorRow label="Rigs" value={`${slice.rigCount} installed`} /> : null}
      {slice.eiv ? (
        <FacilityEivControl
          automaticEiv={slice.eiv.automaticEiv}
          error={slice.eiv.error}
          loading={slice.eiv.loading}
          manual={slice.eiv.manual}
          onClearValue={() => actions.onEivClear?.()}
          onCommitValue={(canonical) => actions.onEivCommit?.(canonical)}
          onManual={(manual) => actions.onEivManual?.(manual)}
          value={slice.eiv.value}
        />
      ) : null}
      {actions.error ? <p className="text-[11px] text-danger">{actions.error}</p> : null}
    </div>
  );
}
