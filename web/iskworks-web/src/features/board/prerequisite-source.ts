// SOURCE-column presentation for one frozen `TicketPrerequisite` (the
// Required Materials table in the ticket detail drawer).
//
// A prerequisite carries BOTH a sourcing `kind` (Buy / Build / React --
// "how this tier resolves it") AND a frozen Model-B inventory-reuse split
// (`reusedQuantity` / `freshQuantity`). The two are orthogonal: a
// `kind: "buy"` row can be fully covered by planned inventory reuse
// (`freshQuantity === 0`), in which case the user buys nothing. Rendering
// the raw kind label ("BUY") for such a row is misleading.
//
// This folds the coverage split into the label using the SAME shared
// `coverageSplit` + `sourcingSummary` the Build Worksheet and Graph use, so
// the vocabulary matches those surfaces exactly:
//   fully covered  -> "Use Inventory"
//   partial        -> "<Kind> · Missing (N)"
//   uncovered      -> "<Kind>"
//
// PRESENTATION ONLY. It reads the frozen prerequisite snapshot and never
// consults live inventory.

import type { RequirementKind } from "../../api/industry";
import type { Tone } from "../../components/primitives";
import { coverageSplit, sourcingSummary } from "../industry/inspector/sourcing-coverage";
import { requirementKindMeta } from "./order-meta";

// The base word `sourcingSummary` accepts (its union spells reactions
// "Reaction")...
const SUMMARY_BASE: Record<RequirementKind, "Buy" | "Build" | "Reaction"> = {
  buy: "Buy",
  build: "Build",
  react: "Reaction",
};

// ...versus the shorter word this table shows. `sourcingSummary` output is
// adapted to it below rather than widening the shared union with a synonym.
const UI_BASE: Record<RequirementKind, string> = {
  buy: "Buy",
  build: "Build",
  react: "React",
};

export interface PrerequisiteSourcePresentation {
  /** e.g. "Use Inventory", "Buy · Missing (60)", "Build". */
  label: string;
  tone: Tone;
}

/** The minimal frozen-prerequisite shape this helper reads -- structurally
 * satisfied by `TicketPrerequisite`. */
export interface PrerequisiteSourceInput {
  kind: RequirementKind;
  requiredQuantity: number;
  freshQuantity: number;
  fulfillmentScope: "missing" | "full";
}

/**
 * SOURCE label + tone for one frozen ticket prerequisite.
 *
 * `remaining` (the amount still to source after planned inventory reuse) is
 * `freshQuantity`, except for an explicitly `Full`-scoped row: that was
 * frozen with no reuse, so it always presents as fresh `<Kind>` even if live
 * stock now exists -- the same defensive guard the Worksheet/Graph adapter
 * applies (`inspector/adapters/linked-build.ts`).
 */
export function prerequisiteSourcePresentation(
  prerequisite: PrerequisiteSourceInput,
): PrerequisiteSourcePresentation {
  const required = prerequisite.requiredQuantity;
  const remaining =
    prerequisite.fulfillmentScope === "full" ? required : prerequisite.freshQuantity;
  const coverage = coverageSplit(required, remaining);

  // `sourcingSummary` owns the state machine (full -> "Use Inventory",
  // partial -> "<base> · Missing (N)", fresh -> "<base>"); we only swap the
  // base word to this table's vocabulary. The replace is a no-op for
  // buy/build and for the "Use Inventory" string.
  const label = sourcingSummary(coverage, { base: SUMMARY_BASE[prerequisite.kind] }).replace(
    SUMMARY_BASE[prerequisite.kind],
    UI_BASE[prerequisite.kind],
  );

  const tone: Tone = coverage.fullyCovered
    ? "positive"
    : coverage.partiallyCovered
      ? "warning"
      : requirementKindMeta[prerequisite.kind].tone;

  return { label, tone };
}
