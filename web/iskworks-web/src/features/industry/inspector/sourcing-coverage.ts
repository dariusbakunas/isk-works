// Neutral, pure sourcing-presentation helper -- ONE interpretation of
// `required` + `remaining` + fulfillment scope + sourcing kind, shared by
// the Build Graph cards, the Graph inspector adapter and the shared
// linked-Build inspector (Worksheet + Graph).
//
// PRESENTATION ONLY. Callers pass the *live* "remaining after inventory"
// quantity for their surface -- an acquisition node's `missingQuantity`, a
// production node's `netRequiredQuantity`, or (Worksheet) `requiredQuantity
// - reusedQuantity`. All three already reflect fulfillment scope: an
// explicitly `Full`-scoped item arrives with `remaining === required`, so it
// classifies as `fresh` and still presents as BUY / BUILD / REACT even when
// physical stock exists. Never pass raw physical inventory here.

export type CoverageState = "inventory" | "partial" | "fresh";

export interface CoverageSplit {
  /** Full demand, clamped to `>= 0`. */
  required: number;
  /** Covered from existing inventory: `clamp(required - remaining, 0, required)`. */
  inventory: number;
  /** Still to acquire / build / react: `clamp(remaining, 0, required)`. */
  remaining: number;
  state: CoverageState;
  /** `required > 0` and nothing left to source -> present as INVENTORY / "Use Inventory". */
  fullyCovered: boolean;
  /** Some covered, some still to source -> a Need / Inventory / <verb> split. */
  partiallyCovered: boolean;
}

/**
 * @param required  full demand
 * @param remaining amount still outstanding after inventory. `null`/`undefined`
 *   is treated as "all of it remains" (no coverage).
 */
export function coverageSplit(
  required: number | null | undefined,
  remaining: number | null | undefined,
): CoverageSplit {
  const req = Math.max(0, Math.trunc(required ?? 0));
  const rawRemaining = remaining == null ? req : Math.trunc(remaining);
  const rem = Math.min(Math.max(0, rawRemaining), req);
  const inventory = req - rem;
  const state: CoverageState =
    req > 0 && rem <= 0 ? "inventory" : inventory > 0 ? "partial" : "fresh";
  return {
    required: req,
    inventory,
    remaining: rem,
    state,
    fullyCovered: state === "inventory",
    partiallyCovered: state === "partial",
  };
}

/** Compact quantity -- matches the Worksheet's own `formatQuantityCompact`. */
function formatCoverageQty(value: number): string {
  if (Math.abs(value) < 10_000) return value.toLocaleString("en-US");
  return new Intl.NumberFormat("en-US", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

/** e.g. `"100% covered"`, `"44% covered"`. */
export function coveragePercentLabel(cov: CoverageSplit): string {
  const pct =
    cov.required > 0 ? Math.round((cov.inventory / cov.required) * 100) : 0;
  return `${pct}% covered`;
}

/**
 * The Sourcing-section digest, matching the Worksheet's `sourcingLabel`
 * output shape exactly:
 *   full     -> `"Use Inventory"`
 *   partial  -> `"<base> · Missing (N)"`  (base = "Buy" | "Build" | "Reaction")
 *   fresh    -> `"<base>"`  (or `"<base> · buildable"` for a buildable Buy)
 */
export function sourcingSummary(
  cov: CoverageSplit,
  opts: { base?: "Buy" | "Build" | "Reaction"; buildable?: boolean } = {},
): string {
  const base = opts.base ?? "Buy";
  if (cov.fullyCovered) return "Use Inventory";
  if (cov.partiallyCovered) return `${base} · Missing (${formatCoverageQty(cov.remaining)})`;
  return opts.buildable ? `${base} · buildable` : base;
}

/**
 * The one-line explanatory sentence, matching the Worksheet's
 * `sourcingSentence`. `verb` is `"buy"` (acquisition), `"build"`
 * (manufacturing) or `"react"` (reaction).
 */
export function fulfillmentSentence(
  cov: CoverageSplit,
  verb: "buy" | "build" | "react",
): string | null {
  if (cov.fullyCovered) return `Use ${formatCoverageQty(cov.inventory)} from inventory.`;
  if (cov.partiallyCovered) {
    return `Use ${formatCoverageQty(cov.inventory)} from inventory and ${verb} ${formatCoverageQty(cov.remaining)}.`;
  }
  return null;
}
