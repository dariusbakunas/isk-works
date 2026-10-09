import { AlertTriangle, Settings2 } from "lucide-react";
import type { ReactNode } from "react";

import type {
  FacilityProfile,
  MarketPricingPolicy,
  MarketScope,
  PriceSource,
} from "../../../../api/industry";
import { useMarketScopeLabel } from "../../../../hooks/use-market-scope-label";
import { pricingPolicyShortLabel } from "./planner-panels";

/**
 * Compact, mostly read-only summary of the Build's planning assumptions --
 * one label/value chip per assumption, wrapping naturally at narrow widths
 * (`flex-wrap`, content-sized cells) rather than a fixed equal-width grid
 * that would force long facility/station names into cramped columns. Editing happens in the `BuildSettingsPanel` right-side
 * inspector, opened via `onEdit`.
 */
export function BuildSettingsSummary({
  leading,
  manufacturingFacilities,
  manufacturingFacilityId,
  reactionFacilities,
  reactionFacilityId,
  rootFacilityRole,
  materialScope,
  materialPolicy,
  outputScope,
  outputPolicy,
  sourceId,
  sources,
  updating,
  onEdit,
  editDisabledReason,
}: {
  /** The Blueprint/Recipe cell -- rendered first, ahead of these settings
   * chips, but owned by the caller since Blueprint keeps its own dialog. */
  leading?: ReactNode;
  manufacturingFacilities: FacilityProfile[];
  manufacturingFacilityId: string;
  reactionFacilities: FacilityProfile[];
  reactionFacilityId: string;
  /** Which slot is the root job's own -- that's the only one an incomplete
   * (unselected) state is warned about, matching `PlanningToolbar`'s prior
   * behavior: an unselected non-root facility is normal, not a warning. */
  rootFacilityRole: "manufacturing" | "reaction";
  materialScope: MarketScope;
  materialPolicy: MarketPricingPolicy;
  outputScope: MarketScope;
  outputPolicy: MarketPricingPolicy;
  sourceId: string;
  sources: PriceSource[];
  updating: boolean;
  onEdit: () => void;
  /** Why Build settings can't be edited right now (e.g. an Epic is
   * selected); disables the button and explains on hover. */
  editDisabledReason?: string;
}) {
  const manufacturingFacility = manufacturingFacilities.find((facility) => facility.id === manufacturingFacilityId);
  const reactionFacility = reactionFacilities.find((facility) => facility.id === reactionFacilityId);
  const manufacturingIncomplete = rootFacilityRole === "manufacturing" && !manufacturingFacilityId;
  const reactionIncomplete = rootFacilityRole === "reaction" && !reactionFacilityId;
  const material = useMarketScopeLabel(materialScope);
  const output = useMarketScopeLabel(outputScope);
  const overrideSource = sourceId ? sources.find((item) => item.id === sourceId) : undefined;
  const overrideLabel = sourceId ? overrideSource?.name ?? "Unknown" : "None";

  return (
    <section aria-label="Planning assumptions" className="border-y border-border bg-panel px-2 py-1.5">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        {/* On a phone the chips collapse to the Edit button alone -- the
            persistent Build context must not push the Plan off-screen. */}
        <div className="contents max-sm:hidden">
          {leading}
          <SummaryCell incomplete={manufacturingIncomplete} label="Manufacturing" value={manufacturingFacility?.name ?? "Not selected"} />
          <SummaryCell incomplete={reactionIncomplete} label="Reactions" value={reactionFacility?.name ?? "Not selected"} />
          <SummaryCell label="Materials" value={`${material.regionName} · ${material.locationName} · ${pricingPolicyShortLabel(materialPolicy)}`} />
          <SummaryCell label="Output" value={`${output.regionName} · ${output.locationName} · ${pricingPolicyShortLabel(outputPolicy)}`} />
          <SummaryCell label="Overrides" value={overrideLabel} />
        </div>
        <button
          className="iw-button-secondary ml-auto shrink-0"
          data-build-settings-trigger
          disabled={editDisabledReason !== undefined}
          onClick={onEdit}
          title={editDisabledReason}
          type="button"
        >
          <Settings2 aria-hidden="true" className="h-3.5 w-3.5" />
          Edit build settings
          {manufacturingIncomplete || reactionIncomplete ? (
            // The chips (and their warning) are hidden on a phone; keep the
            // missing-facility warning visible on the button there.
            <AlertTriangle aria-hidden="true" className="h-3 w-3 text-warning sm:hidden" />
          ) : null}
        </button>
        {manufacturingIncomplete || reactionIncomplete ? (
          <span className="sr-only sm:hidden">Root facility not selected</span>
        ) : null}
      </div>
      <p className="sr-only" aria-live="polite">{updating ? "Updating candidate preview" : "Candidate preview up to date"}</p>
    </section>
  );
}

function SummaryCell({ incomplete = false, label, value }: { incomplete?: boolean; label: string; value: string }) {
  return (
    <div aria-label={label} className="min-w-0 max-w-xs" role="group">
      <span className={`iw-eyebrow flex items-center gap-1 ${incomplete ? "text-warning" : ""}`}>
        {label}
        {incomplete ? <AlertTriangle aria-hidden="true" className="h-3 w-3" /> : null}
      </span>
      <strong className={`mt-0.5 block truncate text-xs font-semibold ${incomplete ? "text-warning" : ""}`} title={value}>
        {value}
      </strong>
    </div>
  );
}
