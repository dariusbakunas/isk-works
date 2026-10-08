// The canonical inspector model for a LINKED Build target -- one manufacturing
// or reaction Build, identified by its own `buildId` plus its
// `parentBuildId` / `componentTypeId` relationship.
//
// This is the single place that decides which sections a linked Build gets
// and what they contain. `buildWorksheetInspector` and `buildGraphInspector`
// both resolve their selection to a `LinkedBuildInput` and return
// `linkedBuildInspector(input)` -- the selection SOURCE (Worksheet vs Graph)
// is transport only. For the same `buildId` + relationship the rendered
// inspector is identical regardless of which view made the selection.

import type {
  BlueprintKind,
  BlueprintObservation,
  BlueprintSelection,
  ComponentFacilityOverride,
  FacilityProfile,
  Money,
  PlannerPricingSelection,
  RecipeCurrency,
  RecipeSelection,
  WorksheetItem,
} from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";
import { reconcileObservedSelection } from "../../builds/planner/available-blueprint";
import { sumDecimalStrings } from "../../shared/decimal";
import { formatPercent } from "../../shared/formatting";
import {
  recipeCurrencyChip,
  recipeCurrencyExplanation,
} from "../../builds/graph/recipe-currency";
import {
  coverageSplit,
  fulfillmentSentence as sourcingFulfillmentSentence,
  sourcingSummary,
} from "../sourcing-coverage";
import type { BuildSettings } from "../../builds/use-build-settings";
import type {
  CostState,
  InspectorActions,
  InspectorModel,
  InspectorWarning,
  ProvenanceSlice,
} from "../inspector-model";
import { pricingSlice } from "./material-slices";
import { withJobSplit } from "../../builds/planner/job-split";

const NUMBER = new Intl.NumberFormat("en-US");

/** Raw (unformatted) cost inputs for the linked Build's own job. */
export interface LinkedBuildCostInput {
  /** Manufacturing material cost. `null` -> derived from total - installation. */
  material: Money | null;
  installation: Money | null;
  total: Money | null;
  state: CostState;
}

/** Everything the canonical linked-Build model needs, in a host-neutral
 * shape. Both the Worksheet and the Graph resolve their selection to one of
 * these. */
export interface LinkedBuildInput {
  buildId: string;
  parentBuildId: string | null;
  componentTypeId: number;
  typeId: number;
  typeName: string;
  recipeMode: "manufacturing" | "reaction";
  blueprintTypeId: number | null;
  blueprintName: string | null;
  /** Reaction formula name -- `null` for a manufacturing Build. */
  formulaName: string | null;
  /** Origin of the blueprint actually selected, when known. */
  selectedBlueprintOrigin: BlueprintKind | null;
  /** The persisted blueprint-selection (manual assumptions or an observed
   * asset id). */
  blueprintSelection: BlueprintSelection | null;
  /** Effective ME/TE resolved by the same preview both views use -- a manual
   * value, or an owned blueprint's ME/TE for an `observedAsset` selection.
   * `null` for a reaction or when the per-node snapshot hasn't run. */
  effectiveMe: number | null;
  effectiveTe: number | null;
  /** Owned instances of this Build's blueprint from the latest ESI sync. */
  observations: BlueprintObservation[];
  /** The blueprint preview hasn't resolved yet. */
  blueprintComputing: boolean;

  /** The parent worksheet's material row for this component -- the demand,
   * coverage, pricing, inventory-cost and used-by facts. Both hosts have it
   * (Worksheet: the selected row; Graph: looked up in the resident
   * worksheet). `null` only when the Graph has no resident worksheet. */
  worksheetRow: WorksheetItem | null;

  /** Production facts of the linked Build's own preview. */
  runs: number;
  /** This Build's own persisted `runs` (Graph only) -- purely
   * informational (see `RunsDiverged`), never the quantity/cost
   * authority. `null`/omitted from the Worksheet, which has no other
   * "effective" run count to diverge from. Shown only when it differs
   * from `runs`. */
  persistedRuns?: number | null;
  producingQuantity: number;
  surplus: number;
  /** Demand when there is no worksheet row (Graph-without-worksheet only). */
  fallbackRequiredQuantity: number | null;
  /** The projection's own scope-aware net demand for this component (Graph
   * only -- `ProductionNode.netRequiredQuantity`). `null` from the Worksheet,
   * which derives the same figure from `worksheetRow.reusedQuantity`. Drives
   * the inventory-aware sourcing digest (INVENTORY / partial / fresh). */
  netRequiredQuantity?: number | null;

  cost: LinkedBuildCostInput;

  /** Resolved facility for this Build's job kind. */
  facilityProfileId: string | null;
  facilities: FacilityProfile[];
  /** The facility snapshot couldn't be resolved (id set, profile missing). */
  facilityUnresolved: boolean;

  pricingContext: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  allowMarketPolicyOverride: boolean;
  fulfillmentScope: "missing" | "full";
  recipeCurrency: RecipeCurrency;
  warnings: InspectorWarning[];
  statusLine: { text: string; tone: "neutral" | "positive" | "blocking" } | null;
  calculationEvidence?: import("react").ReactNode;
  /** Editable controls (blueprint/facility/sourcing) are offered. */
  interactive: boolean;

  /** Build-ID-addressable settings -- the mutation seam, identical in both
   * views. `null` before the linked Build's own settings have loaded. */
  settings: BuildSettings | null;
  handlers: {
    onOpenLinkedBuild?: (buildId: string) => void;
    onCopyBuildId?: (buildId: string) => void;
    onPricingChange?: (selection: PlannerPricingSelection) => void;
    /** Switch this component back to BUY. */
    onSwitchToBuy?: () => void;
    /** Draw the whole requirement from inventory (drop the linked Build). */
    onUseInventory?: () => void;
    /** Change the shortage-only / full quantity scope. */
    onScope?: (scope: "missing" | "full") => void;
    /** Fallback blueprint / facility mutation when `settings` isn't ready
     * (seeds the parent Build's per-component override). */
    onBlueprintSelectionChange?: (typeId: number, selection: BlueprintSelection | null) => void;
    onFacilityOverrideChange?: (typeId: number, override: ComponentFacilityOverride | null) => void;
  };
}

function money(value: Money | null): string | null {
  return value != null ? formatIskSummary(value) : null;
}

function negate(value: Money | null): string | null {
  if (value == null) return null;
  return value.startsWith("-") ? value.slice(1) : `-${value}`;
}

function signed(value: number): string {
  return `${value > 0 ? "+" : ""}${NUMBER.format(value)}`;
}

function recipeCurrencyBadge(state: RecipeCurrency): ProvenanceSlice["recipeCurrency"] {
  const chip = recipeCurrencyChip(state);
  if (!chip) return null;
  return {
    label: chip.label,
    tone: chip.tone === "danger" ? "blocking" : "neutral",
    explanation: recipeCurrencyExplanation(state),
  };
}

/**
 * Canonical inspector model + actions for a linked manufacturing / reaction
 * Build. Section set and content are fixed by the TARGET, never by which view
 * made the selection.
 */
export function linkedBuildInspector(input: LinkedBuildInput): {
  model: InspectorModel;
  actions: InspectorActions;
} {
  const row = input.worksheetRow;
  const isReaction = input.recipeMode === "reaction";
  const required = row?.requiredQuantity ?? input.fallbackRequiredQuantity ?? 0;
  const available = row?.availableQuantity ?? 0;
  const covered = row?.coveredQuantity ?? 0;
  const missing = row?.missingQuantity ?? 0;
  const coveragePct = row ? Number(row.coveragePercentage) || 0 : 0;

  // ── identity ──
  const model: InspectorModel = {
    identity: {
      kind: isReaction ? "reaction" : "linkedBuild",
      kindLabel: isReaction ? "REACTION" : "LINKED BUILD",
      name: input.typeName,
      subtitle: isReaction ? "Reaction" : "Manufacturing",
      typeId: input.typeId,
      showImage: true,
      summary: `Need ${NUMBER.format(required)} · Making ${NUMBER.format(input.producingQuantity)}`,
    },
    warnings: input.warnings,
    statusLine: input.statusLine,
  };

  // ── QUANTITIES (material demand + production operation, one section) ──
  const quantityMetrics = [
    { label: "Required", value: NUMBER.format(required) },
    { label: "Available", value: NUMBER.format(available) },
    { label: "Covered", value: NUMBER.format(covered), tone: covered > 0 ? ("positive" as const) : undefined },
    {
      label: "Shortage",
      value: NUMBER.format(missing),
      tone: missing > 0 ? ("blocking" as const) : undefined,
    },
    { label: "Making", value: NUMBER.format(input.producingQuantity) },
    {
      label: input.surplus < 0 ? "Shortfall" : "Surplus",
      value: signed(input.surplus),
      tone:
        input.surplus < 0 ? ("blocking" as const) : input.surplus > 0 ? ("positive" as const) : undefined,
    },
    { label: "Runs", value: NUMBER.format(input.runs) },
  ];
  // The effective (projected) runs above is what this plan is actually
  // priced/sized at -- always the primary figure. Surface the Build's own
  // saved runs alongside it only when they diverge (`RunsDiverged`),
  // purely informational: never implies this plan needs a resave, and
  // never looks like the authoritative number.
  if (input.persistedRuns != null && input.persistedRuns !== input.runs) {
    quantityMetrics.push({
      label: "Saved Build runs",
      value: NUMBER.format(input.persistedRuns),
    });
  }
  if (row?.projectedInventoryCost) {
    quantityMetrics.push({ label: "Inventory cost", value: formatIskSummary(row.projectedInventoryCost) });
  }
  model.quantities = {
    metrics: quantityMetrics,
    percentage: coveragePct,
    hasShortage: missing > 0,
    summary:
      missing > 0
        ? `Need ${NUMBER.format(required)} · ${NUMBER.format(missing)} short`
        : `Making ${NUMBER.format(input.producingQuantity)} · ${signed(input.surplus)}`,
  };

  // ── SOURCING (identical control set + inventory-aware digest, wherever
  //    it applies) ──
  //
  // "Remaining to produce after inventory", honoring fulfillment scope:
  //  - Graph: the projection's own scope-aware net (`netRequiredQuantity`).
  //  - Worksheet: `required - reusedQuantity`. Model B freezes NO reuse for a
  //    `Full`-scoped row, so `Full` naturally yields `remaining === required`;
  //    the explicit guard is belt-and-braces against a stale `reusedQuantity`.
  //  - `missing` (physical `WorksheetItem.missingQuantity`) is deliberately
  //    NOT used to classify -- it ignores an explicit `Full` scope.
  const reused =
    input.fulfillmentScope === "full" ? 0 : Math.max(0, row?.reusedQuantity ?? 0);
  const remaining = input.netRequiredQuantity ?? Math.max(0, required - reused);
  const cov = coverageSplit(required, remaining);
  const sourcingBase = isReaction ? "Reaction" : "Build";
  const sourcingVerb: "build" | "react" = isReaction ? "react" : "build";
  model.sourcing = {
    mode: "build",
    buildable: true,
    recipeSummary: isReaction ? "Reaction" : "Manufacturing",
    // Both flags are scope-aware -- driven by `cov` (which already folds in
    // fulfillment scope), never raw physical `missing`/`available`. So an
    // explicit `Full` scope reads as BUILD / REACT even with stock on hand,
    // and a `Missing`-scoped row whose current demand is entirely met by
    // inventory reads as "Use Inventory" -- identical from either host. The
    // physical Available / Covered figures still show in QUANTITIES.
    fullyCoveredByInventory: cov.fullyCovered,
    usingInventory: cov.fullyCovered,
    hasShortfall: cov.partiallyCovered,
    scope: input.fulfillmentScope,
    requiredQuantity: cov.required,
    missingQuantity: cov.remaining,
    availableQuantity: available,
    fulfillmentSentence: sourcingFulfillmentSentence(cov, sourcingVerb),
    summary: sourcingSummary(cov, { base: sourcingBase }),
  };

  // ── RECIPE (reaction only) ──
  if (isReaction) {
    model.recipe = {
      kind: "formula",
      name: input.formulaName,
      computing: input.blueprintComputing,
      summary: input.formulaName ?? (input.blueprintComputing ? "Computing…" : "—"),
    };
  }

  // ── BLUEPRINT (manufacturing only) ──
  if (!isReaction) {
    const selection = input.blueprintSelection;
    const manual = selection && selection.mode === "manual" ? selection : null;
    // A saved `observedAsset` selection points at an observation id that a
    // later ESI sync will have superseded (fresh rows, new ids). Recover the
    // intended current observation the same way the Build editor does, so
    // the inspector's radio + ME/TE agree with the Build page instead of
    // showing "not selected".
    const observedSelection =
      selection?.mode === "observedAsset"
        ? reconcileObservedSelection(input.observations, selection, input.runs)
        : null;
    const origin =
      input.selectedBlueprintOrigin === "copy"
        ? ("BPC" as const)
        : input.selectedBlueprintOrigin === "original"
          ? ("BPO" as const)
          : manual
            ? manual.kind === "copy"
              ? ("BPC" as const)
              : ("BPO" as const)
            : observedSelection
              ? observedSelection.kind === "copy"
                ? ("BPC" as const)
                : ("BPO" as const)
              : null;
    const effectiveMe = input.effectiveMe ?? observedSelection?.materialEfficiency ?? null;
    const effectiveTe = input.effectiveTe ?? observedSelection?.timeEfficiency ?? null;
    model.blueprint = {
      kind: "blueprint",
      name: input.blueprintName,
      blueprintTypeId: input.blueprintTypeId,
      mode: selection?.mode === "observedAsset" ? "existing" : manual ? "manual" : "unresearched",
      origin,
      me: effectiveMe ?? manual?.materialEfficiency ?? null,
      te: effectiveTe ?? manual?.timeEfficiency ?? null,
      licensedRuns: manual?.kind === "copy" ? manual.licensedRuns : null,
      notes: manual?.notes ?? "",
      observations: input.observations,
      selectedObservationId:
        selection?.mode === "observedAsset"
          ? (observedSelection?.id ?? selection.observationId)
          : null,
      requiredRuns: input.runs,
      computing: input.blueprintComputing,
      // Stages owns descendant production configuration editing (blueprint
      // selection, facility, ME/TE) -- Graph and Worksheet show it read-only
      // here so there is one obvious editing surface. `input.interactive` is
      // otherwise unused by this adapter; sourcing (Buy/Build/Reaction) stays
      // fully editable via `model.sourcing`/`actions.sourcing` below, since
      // that decision remains a Graph/Worksheet concern.
      editable: false,
      summary: withJobSplit(
        manual
          ? `${manual.kind === "copy" ? "BPC" : "BPO"} · ME ${input.effectiveMe ?? manual.materialEfficiency} · TE ${input.effectiveTe ?? manual.timeEfficiency}`
          : selection?.mode === "observedAsset"
            ? effectiveMe != null && effectiveTe != null
              ? `Owned blueprint · ME ${effectiveMe} · TE ${effectiveTe}`
              : "Owned blueprint"
            : "Unresearched (ME 0 · TE 0)",
        input.runs,
        manual?.kind === "copy"
          ? manual.licensedRuns
          : selection?.mode === "observedAsset"
            ? (selection.licensedRuns ?? (observedSelection?.kind === "copy" ? observedSelection.licensedRuns : null))
            : null,
      ),
    };
  }

  // ── FACILITY ──
  const facility = input.facilityProfileId
    ? input.facilities.find((p) => p.id === input.facilityProfileId) ?? null
    : null;
  model.facility = {
    name: facility?.name ?? null,
    location: facility
      ? [facility.solarSystemName, facility.structureTypeName].filter(Boolean).join(" · ")
      : null,
    bonuses: facility
      ? `Material −${formatPercent(facility.materialReductionPercent)} · Time −${formatPercent(facility.timeReductionPercent)}`
      : null,
    rigCount: facility?.rigs.length ?? 0,
    state: facility ? "set" : input.facilityUnresolved ? "unresolved" : input.facilityProfileId ? "unresolved" : "unset",
    // See the matching comment on `model.blueprint.editable` above.
    editable: false,
    role: isReaction ? "reaction" : "manufacturing",
    selectedFacilityId: input.facilityProfileId,
    summary: facility?.name ?? (input.facilityUnresolved ? "Unavailable" : "Uses build facility"),
  };

  // ── COST (manufacturing material / installation / total) ──
  const totalRaw = input.cost.total;
  const installationRaw = input.cost.installation;
  const materialRaw =
    input.cost.material ??
    (totalRaw != null && installationRaw != null
      ? sumDecimalStrings([totalRaw, negate(installationRaw)]).value
      : totalRaw != null && installationRaw == null
        ? totalRaw
        : null);
  const total = money(totalRaw);
  const installation = money(installationRaw);
  const material = money(materialRaw);
  model.cost = {
    material,
    installation,
    total,
    state: input.cost.state,
    summary: total ? `${total} total` : material ? `${material} materials` : "Incomplete",
  };

  // ── PRICING (the component's market valuation controls) ──
  if (row) {
    model.pricing = pricingSlice(row, {
      allowPolicyOverride: input.allowMarketPolicyOverride,
      readOnly: false,
      sourceName: input.pricingContext?.sourceName,
    });
  }

  // ── USED BY ──
  if (row && row.contributions.length > 0) {
    model.usedBy = {
      label: "Used By",
      fallbackTypeId: null,
      entries: row.contributions.map((c) => ({
        typeId: c.parentTypeId,
        name: c.parentTypeName,
        quantity: c.quantity,
      })),
    };
  }

  // ── PROVENANCE ──
  model.provenance = {
    summary: input.pricingContext?.sourceName ?? "Linked build",
    lines: input.pricingContext
      ? [
          { label: "Revision", value: NUMBER.format(input.pricingContext.sourceRevision) },
          { label: "Captured", value: new Date(input.pricingContext.capturedAt).toLocaleString() },
        ]
      : [],
    note: row?.pricing.sourceNote || null,
    buildId: input.buildId,
    recipeCurrency: recipeCurrencyBadge(input.recipeCurrency),
  };

  // ── actions ──
  const actions: InspectorActions = { calculationEvidence: input.calculationEvidence };
  const settings = input.settings;

  if (input.handlers.onPricingChange && model.pricing) {
    actions.pricing = { onChange: input.handlers.onPricingChange };
  }

  // Sourcing switches target the PARENT Build, so they work independent of
  // whether this linked Build's own settings have loaded.
  if (
    input.handlers.onSwitchToBuy ||
    input.handlers.onUseInventory ||
    input.handlers.onScope
  ) {
    actions.sourcing = {
      onBuy: input.handlers.onSwitchToBuy,
      onUseInventory: input.handlers.onUseInventory,
      onScope: input.handlers.onScope,
      pending: settings?.pending,
    };
  }

  if (model.blueprint?.editable) {
    actions.blueprint = {
      pending: settings?.pending,
      error: settings?.error ?? null,
      onSelectObservation: (observationId) => {
        if (settings) void settings.updateBlueprintSelection({ mode: "observedAsset", observationId });
        else input.handlers.onBlueprintSelectionChange?.(input.componentTypeId, { mode: "observedAsset", observationId });
      },
      onModelManually: (mm) => {
        const selection: BlueprintSelection = { mode: "manual", ...mm };
        if (settings) void settings.updateBlueprintSelection(selection);
        else input.handlers.onBlueprintSelectionChange?.(input.componentTypeId, selection);
      },
    };
  }

  if (model.facility?.editable) {
    actions.facility = {
      options: input.facilities,
      pending: settings?.pending,
      error: settings?.error ?? null,
      onSelect: (facilityProfileId) => {
        if (settings) {
          void settings.updateFacility({ facilityProfileId });
        } else if (!facilityProfileId) {
          input.handlers.onFacilityOverrideChange?.(input.componentTypeId, null);
        } else {
          const profile = input.facilities.find((p) => p.id === facilityProfileId);
          if (profile) {
            input.handlers.onFacilityOverrideChange?.(input.componentTypeId, {
              facilityProfileId: profile.id,
            });
          }
        }
      },
    };
  }

  if (input.parentBuildId && input.handlers.onOpenLinkedBuild) {
    actions.openLinkedBuild = () => input.handlers.onOpenLinkedBuild?.(input.buildId);
  }
  if (input.handlers.onCopyBuildId) {
    actions.copyBuildId = () => input.handlers.onCopyBuildId?.(input.buildId);
  }

  return { model, actions };
}

/** The linked Build's own producing quantity + surplus, from its recipe and
 * run count -- so the Worksheet can show the same Making / Surplus / Runs the
 * Graph gets from the projection, without a graph request. */
export function producingFromRecipe(
  recipe: RecipeSelection | { kind: "manufacturing" | "reaction"; products?: Array<{ typeId: number; quantityPerRun: number }> },
  runs: number,
  outputTypeId: number,
  requiredQuantity: number,
): { producingQuantity: number; surplus: number } {
  const products = "products" in recipe ? recipe.products ?? [] : [];
  const perRun = products.find((p) => p.typeId === outputTypeId)?.quantityPerRun ?? 1;
  const producingQuantity = runs * perRun;
  return { producingQuantity, surplus: producingQuantity - requiredQuantity };
}
