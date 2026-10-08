// How a linked Build's `recipeCurrency` is surfaced on the Graph.
//
// `recipeCurrency` is a backend read-model comparison recomputed on every
// Build load: the recipe captured when the Build was last saved vs. the
// recipe the *currently active* SDE import yields for the same blueprint /
// formula. It never affects the numbers shown -- every quantity and cost on
// the graph is computed against the current SDE regardless. So most states
// need no user attention: a Build simply saved on an earlier SDE, or one
// the comparison couldn't run for, is not something the user did or must
// act on.

import type { RecipeCurrency } from "../../../../api/industry";

export interface RecipeCurrencyChip {
  label: string;
  tone: "warning" | "danger";
}

/** The Graph card chip for `state`, or `null` when the card should stay
 * quiet (`current`, an unchanged recipe on a newer SDE, an inconclusive
 * comparison). */
export function recipeCurrencyChip(state: RecipeCurrency): RecipeCurrencyChip | null {
  switch (state) {
    case "recipeChanged":
      return { label: "Recipe data changed", tone: "warning" };
    case "blueprintNoLongerAvailable":
      return { label: "Blueprint unavailable", tone: "danger" };
    case "reactionFormulaNoLongerAvailable":
      return { label: "Formula unavailable", tone: "danger" };
    default:
      return null;
  }
}

/** Fuller explanatory text for the selected-node inspector, or `null` when
 * the inspector should say nothing. Never implies the user changed a recipe. */
export function recipeCurrencyExplanation(state: RecipeCurrency): string | null {
  switch (state) {
    case "recipeChanged":
      return (
        "The current SDE recipe for this blueprint differs from the one captured " +
        "when this Build was last saved. The quantities and cost shown here use the " +
        "current SDE; save the Build to adopt it."
      );
    case "blueprintNoLongerAvailable":
      return "This Build's blueprint is not in the current SDE, so it can't be re-checked against current data.";
    case "reactionFormulaNoLongerAvailable":
      return "This Build's reaction formula is not in the current SDE, so it can't be re-checked against current data.";
    case "olderSdeVersion":
      return "This Build was saved on an earlier SDE. Its recipe is unchanged in the current SDE.";
    default:
      return null;
  }
}
