import type { CostWarning } from "../../../../api/industry";

// A `CostWarning` carries `opIndex` (and sometimes `typeId`/`traversalIndex`),
// never a durable row/occurrence identity -- so, unlike Graph's
// `graphNodeId`-attributed warnings, these cannot be pinned to a specific
// Stages row. Rendered as a single global banner instead of inventing a
// second per-row warning system. Labels mirror
// `crates/iskworks-core/src/build_cost.rs`'s own `cost_warning_message`
// wording.
export function costWarningLabel(warning: CostWarning): string {
  switch (warning.code) {
    case "missingFreshPrice":
      return `No fresh market/manual price is available for type ${warning.typeId}`;
    case "missingInventoryBasis":
      return `No historical inventory basis is available for type ${warning.typeId}`;
    case "missingAdjustedPrice":
      return `No adjusted price is available for ${warning.typeIds.length} base material${warning.typeIds.length === 1 ? "" : "s"}`;
    case "missingSystemCostIndex":
      return "A facility has no configured system cost index";
    case "noFacilitySelected":
      return "An operation has no facility selected";
    case "unresolvedBuild":
      return `Type ${warning.typeId} is set to Build but has no linked build yet`;
    case "childCostIncomplete":
      return "A production prerequisite's own cost is incomplete";
    case "staleFreshPrice":
      return `The price for type ${warning.typeId} is backed by stale market evidence`;
    case "arithmeticOverflow":
      return "A cost calculation exceeded the supported range";
    default:
      return "Unknown cost warning";
  }
}
