import type {
  BlueprintObservation,
  BlueprintSelection,
  ComponentFacilityOverride,
  FacilityProfile,
  FulfillmentScope,
  PlannerPricingSelection,
  RecipeSelection,
  WorksheetItem,
} from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";
import { reconcileObservedSelection } from "../../builds/planner/available-blueprint";
import { formatPercent } from "../../shared/formatting";
import {
  coverageSlice,
  pricingSlice,
  provenanceSlice,
  valueSlice,
} from "./material-slices";
import { linkedBuildInspector, producingFromRecipe, type LinkedBuildInput } from "./linked-build";
import type { BuildSettings } from "../../builds/use-build-settings";
import type {
  CostState,
  InspectorActions,
  InspectorModel,
  InspectorWarning,
} from "../inspector-model";
import { withJobSplit } from "../../builds/planner/job-split";

const NUMBER = new Intl.NumberFormat("en-US");

export interface WorksheetInspectorContext {
  item: WorksheetItem;
  /** Row is currently resolved to BUILD. */
  buildResolved: boolean;
  /** The row's active linked Build (editor-owned), or `null`. */
  linkedBuild: import("../../../../api/industry").Build | null;
  creatingLinkedBuild: boolean;
  linkedBuildError: string;
  /** The parent Build's per-component blueprint override (seeds a not-yet
   * created linked Build). */
  blueprintSelection?: BlueprintSelection;
  facilityOverride?: ComponentFacilityOverride;
  fulfillmentScope?: FulfillmentScope;
  allFacilities: FacilityProfile[];
  /** Persisted parent Build id -- enables linked-build navigation. */
  parentBuildId?: string;
  rootTypeId: number | null;
  /** The row's own producible recipe, `null` if not buildable, `undefined`
   * while still checking. */
  recipe: RecipeSelection | null | undefined;
  /** Owned blueprint instances for this row's blueprint type. */
  observations: BlueprintObservation[];
  pricingContext?: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  readOnly: boolean;
  allowMarketPolicyOverride: boolean;
  calculationEvidence?: import("react").ReactNode;
  /** Build-ID-addressable settings for the active linked Build (blueprint /
   * facility edits route here, same as the Graph). `null` when there is no
   * linked Build yet. */
  linkedSettings: BuildSettings | null;
  handlers: {
    onPricingChange?: (selection: PlannerPricingSelection) => void;
    onResolutionChange?: (typeId: number, recipe: RecipeSelection | null) => void;
    onBlueprintSelectionChange?: (typeId: number, selection: BlueprintSelection | null) => void;
    onFacilityOverrideChange?: (typeId: number, override: ComponentFacilityOverride | null) => void;
    onFulfillmentScopeChange?: (typeId: number, scope: FulfillmentScope | null) => void;
    onOpenLinkedBuild?: (buildId: string) => void;
    onCopyBuildId?: (buildId: string) => void;
  };
}

function collectWarnings(context: WorksheetInspectorContext): InspectorWarning[] {
  const { item, buildResolved } = context;
  const warnings: InspectorWarning[] = [];
  if (item.pricing.missing) {
    warnings.push({
      label: "Pricing incomplete",
      detail: "This row has no unit price from the selected source.",
      tone: "blocking",
    });
  }
  if (!item.lineTotal) {
    warnings.push({
      label: "Value incomplete",
      detail: "A line total can't be computed until this row is priced.",
      tone: "blocking",
    });
  }
  if (buildResolved && !item.installationCost) {
    warnings.push({
      label: "Installation cost missing",
      detail: "Select a facility on the linked build to include its installation cost.",
      tone: "neutral",
    });
  }
  return warnings;
}

function fulfillmentSentence(item: WorksheetItem, buildResolved: boolean): string | null {
  if (!item.reusedQuantity) return null;
  const missing = item.requiredQuantity - item.reusedQuantity;
  const reused = item.reusedQuantity.toLocaleString();
  if (missing === 0) return `Use ${reused} from inventory.`;
  return `Use ${reused} from inventory and ${buildResolved ? "build" : "buy"} ${missing.toLocaleString()}.`;
}

/** Effective blueprint ME/TE for a linked Build, from the worksheet side:
 * a manual selection's own values, or an owned blueprint observation's
 * ME/TE for an `observedAsset` selection. `null` when neither is known --
 * the same value the Graph's `effectiveMe`/`effectiveTe` carries. */
function effectiveMeTe(
  selection: BlueprintSelection | null,
  observations: BlueprintObservation[],
  requiredRuns: number,
): { me: number | null; te: number | null } {
  if (!selection) return { me: null, te: null };
  if (selection.mode === "manual") {
    return { me: selection.materialEfficiency, te: selection.timeEfficiency };
  }
  // Tolerate a stale observation id left behind by a later ESI sync -- same
  // recovery the Build editor applies.
  const observed = reconcileObservedSelection(observations, selection, requiredRuns);
  return { me: observed?.materialEfficiency ?? null, te: observed?.timeEfficiency ?? null };
}

/** Resolve a selected build-resolved Worksheet row to the canonical
 * `LinkedBuildInput`, then build the model. The Graph builds the same input
 * for the same `buildId`. */
function linkedBuildFromWorksheet(
  context: WorksheetInspectorContext,
  opts: { scope: "missing" | "full" },
): { model: InspectorModel; actions: InspectorActions } {
  const { item, linkedBuild, linkedSettings, allFacilities, observations, handlers, readOnly } = context;
  const build = linkedBuild!;
  const isReaction = build.recipe.kind === "reaction";
  const settingsInput = linkedSettings?.build?.draftPlanning?.input ?? build.draftPlanning?.input;
  const blueprintSelection =
    settingsInput?.blueprintSelection ?? context.blueprintSelection ?? null;
  const { me, te } = effectiveMeTe(blueprintSelection, observations, build.runs);
  const { producingQuantity, surplus } = producingFromRecipe(
    build.recipe,
    build.runs,
    item.typeId,
    item.requiredQuantity,
  );
  const facilityProfileId =
    (isReaction ? settingsInput?.reactionFacility : settingsInput?.manufacturingFacility)
      ?.facilityProfileId ??
    context.facilityOverride?.facilityProfileId ??
    null;
  const interactive = !readOnly && Boolean(handlers.onResolutionChange);
  const costState: CostState = item.lineTotal ? "known" : item.installationCost ? "incomplete" : "notComputed";

  const input: LinkedBuildInput = {
    buildId: build.id,
    parentBuildId: context.parentBuildId ?? null,
    componentTypeId: item.typeId,
    typeId: item.typeId,
    typeName: item.typeName,
    recipeMode: isReaction ? "reaction" : "manufacturing",
    blueprintTypeId: build.recipe.kind === "manufacturing" ? build.recipe.blueprintTypeId : null,
    blueprintName: build.recipe.kind === "manufacturing" ? build.recipe.blueprintName : null,
    formulaName: build.recipe.kind === "reaction" ? build.recipe.reactionFormulaName : null,
    selectedBlueprintOrigin: build.selectedBlueprintOrigin,
    blueprintSelection,
    effectiveMe: me,
    effectiveTe: te,
    observations,
    blueprintComputing: false,
    worksheetRow: item,
    runs: build.runs,
    producingQuantity,
    surplus,
    fallbackRequiredQuantity: item.requiredQuantity,
    cost: { material: null, installation: item.installationCost, total: item.lineTotal, state: costState },
    facilityProfileId,
    facilities: allFacilities,
    facilityUnresolved: Boolean(facilityProfileId) && !allFacilities.some((p) => p.id === facilityProfileId),
    pricingContext: context.pricingContext ?? null,
    allowMarketPolicyOverride: context.allowMarketPolicyOverride,
    fulfillmentScope: opts.scope,
    recipeCurrency: build.recipeCurrency,
    warnings: collectWarnings(context),
    statusLine: context.linkedBuildError
      ? { text: context.linkedBuildError, tone: "blocking" }
      : context.creatingLinkedBuild
        ? { text: "Creating linked build…", tone: "neutral" }
        : null,
    calculationEvidence: context.calculationEvidence,
    interactive,
    settings: linkedSettings,
    handlers: {
      onOpenLinkedBuild: handlers.onOpenLinkedBuild,
      onCopyBuildId: handlers.onCopyBuildId,
      onPricingChange: handlers.onPricingChange,
      onSwitchToBuy: () => {
        handlers.onResolutionChange?.(item.typeId, null);
        if (item.missingQuantity === 0 && item.availableQuantity > 0) {
          handlers.onFulfillmentScopeChange?.(item.typeId, "full");
        }
      },
      onUseInventory:
        item.missingQuantity === 0 && item.availableQuantity > 0
          ? () => {
              handlers.onResolutionChange?.(item.typeId, null);
              handlers.onFulfillmentScopeChange?.(item.typeId, null);
            }
          : undefined,
      onScope: (next) => handlers.onFulfillmentScopeChange?.(item.typeId, next === "full" ? "full" : null),
      onBlueprintSelectionChange: handlers.onBlueprintSelectionChange,
      onFacilityOverrideChange: handlers.onFacilityOverrideChange,
    },
  };
  return linkedBuildInspector(input);
}

/**
 * Canonical inspector model + actions for a selected Worksheet material /
 * output row. The Graph counterpart is `buildGraphInspector`; both feed the
 * one `UnifiedItemInspector`.
 */
export function buildWorksheetInspector(context: WorksheetInspectorContext): {
  model: InspectorModel;
  actions: InspectorActions;
} {
  const {
    item,
    buildResolved,
    linkedBuild,
    linkedSettings,
    readOnly,
    handlers,
    allFacilities,
    observations,
  } = context;
  const isOutput = item.role === "output";
  const interactive = !readOnly && Boolean(handlers.onResolutionChange);
  const fullyCoveredByInventory = item.missingQuantity === 0 && item.availableQuantity > 0;
  const scope: "missing" | "full" = context.fulfillmentScope === "full" ? "full" : "missing";

  // ── LINKED BUILD target: the canonical model, identical to what the Graph
  //    produces for the same `buildId` + relationship. ──
  if (buildResolved && item.role === "material" && linkedBuild) {
    return linkedBuildFromWorksheet(context, { scope });
  }

  const kind = isOutput
    ? "outputItem"
    : buildResolved
      ? "linkedBuild"
      : context.recipe
        ? "buildableBuyMaterial"
        : "buyMaterial";
  const usingInventory = !buildResolved && fullyCoveredByInventory && scope === "missing";

  const model: InspectorModel = {
    identity: {
      kind,
      kindLabel: isOutput ? "OUTPUT" : buildResolved ? "LINKED BUILD" : "BUY MATERIAL",
      name: item.typeName,
      subtitle: buildResolved ? "Linked build" : context.recipe ? "Buildable component" : null,
      typeId: item.typeId,
      showImage: buildResolved || isOutput,
      summary: isOutput
        ? `Producing ${NUMBER.format(item.requiredQuantity)}`
        : item.missingQuantity > 0
          ? `Required ${NUMBER.format(item.requiredQuantity)} · ${NUMBER.format(item.missingQuantity)} short`
          : `Required ${NUMBER.format(item.requiredQuantity)}`,
    },
    warnings: collectWarnings(context),
    statusLine: context.linkedBuildError
      ? { text: context.linkedBuildError, tone: "blocking" }
      : context.creatingLinkedBuild
        ? { text: "Creating linked build…", tone: "neutral" }
        : null,
    coverage: coverageSlice(item),
    value: valueSlice(
      item,
      allFacilities.find((p) => p.id === context.facilityOverride?.facilityProfileId)?.name,
    ),
    provenance: provenanceSlice({
      pricingContext: context.pricingContext,
      sourceNote: item.pricing.sourceNote,
      buildId: linkedBuild?.id ?? null,
      recipeCurrency: null,
    }),
    pricing: pricingSlice(item, {
      allowPolicyOverride: context.allowMarketPolicyOverride,
      readOnly,
      sourceName: context.pricingContext?.sourceName,
    }),
  };

  if (item.contributions.length > 0) {
    model.usedBy = {
      label: "Used By",
      fallbackTypeId: context.rootTypeId,
      entries: item.contributions.map((c) => ({
        typeId: c.parentTypeId,
        name: c.parentTypeName,
        quantity: c.quantity,
      })),
    };
  }

  // Sourcing (material rows only).
  if (item.role === "material" && interactive) {
    model.sourcing = {
      mode: buildResolved ? "build" : "buy",
      buildable: Boolean(context.recipe),
      recipeSummary: context.recipe
        ? context.recipe.mode === "manufacturing"
          ? "Manufacturing"
          : "Reaction"
        : null,
      fullyCoveredByInventory,
      usingInventory,
      hasShortfall: item.missingQuantity > 0,
      scope,
      requiredQuantity: item.requiredQuantity,
      missingQuantity: item.missingQuantity,
      availableQuantity: item.availableQuantity,
      fulfillmentSentence: fulfillmentSentence(item, buildResolved),
      summary: buildResolved
        ? `Build · ${scope === "full" ? "Full" : "Shortage only"}`
        : usingInventory
          ? "From inventory"
          : "Buy",
    };
  }

  // Cost of the build operation (Material / Installation / Total), for a
  // build-resolved row -- the same shape the Graph shows for a linked node.
  if (buildResolved) {
    const total = item.lineTotal ? formatIskSummary(item.lineTotal) : null;
    const installation = item.installationCost ? formatIskSummary(item.installationCost) : null;
    model.cost = {
      material: null,
      installation,
      total,
      state: total ? "known" : installation ? "incomplete" : "notComputed",
      summary: total ? `${total} total` : installation ? `${installation} installation` : "Incomplete",
    };
  }

  // Blueprint / facility for a build-resolved manufacturing row.
  const linkedRecipeMode =
    linkedBuild?.recipe.kind ?? (context.recipe ? context.recipe.mode : null);
  if (buildResolved && linkedRecipeMode === "manufacturing") {
    const settingsInput = linkedSettings?.build?.draftPlanning?.input ?? linkedBuild?.draftPlanning?.input;
    const selection = settingsInput?.blueprintSelection ?? context.blueprintSelection ?? null;
    const manual = selection && selection.mode === "manual" ? selection : null;
    const requiredRuns = linkedBuild?.runs ?? 1;
    // Recover a stale observedAsset id (superseded by a later ESI sync) so
    // the radio + ME/TE match the Build page -- see reconcileObservedSelection.
    const observedSelection =
      selection?.mode === "observedAsset"
        ? reconcileObservedSelection(observations, selection, requiredRuns)
        : null;
    model.blueprint = {
      kind: "blueprint",
      name: linkedBuild?.recipe.kind === "manufacturing" ? linkedBuild.recipe.blueprintName : null,
      blueprintTypeId:
        linkedBuild?.recipe.kind === "manufacturing"
          ? linkedBuild.recipe.blueprintTypeId
          : context.recipe?.mode === "manufacturing"
            ? context.recipe.blueprintTypeId
            : null,
      mode: selection?.mode === "observedAsset" ? "existing" : manual ? "manual" : "unresearched",
      origin: manual
        ? (manual.kind === "copy" ? "BPC" : "BPO")
        : observedSelection
          ? (observedSelection.kind === "copy" ? "BPC" : "BPO")
          : linkedBuild?.selectedBlueprintOrigin === "copy"
            ? "BPC"
            : linkedBuild?.selectedBlueprintOrigin === "original"
              ? "BPO"
              : null,
      me: manual?.materialEfficiency ?? observedSelection?.materialEfficiency ?? null,
      te: manual?.timeEfficiency ?? observedSelection?.timeEfficiency ?? null,
      licensedRuns: manual?.kind === "copy" ? manual.licensedRuns : null,
      notes: manual?.notes ?? "",
      observations,
      selectedObservationId:
        selection?.mode === "observedAsset"
          ? (observedSelection?.id ?? selection.observationId)
          : null,
      requiredRuns,
      computing: false,
      editable: interactive,
      summary: withJobSplit(
        manual
          ? `${manual.kind === "copy" ? "BPC" : "BPO"} · ME ${manual.materialEfficiency} · TE ${manual.timeEfficiency}`
          : selection?.mode === "observedAsset"
            ? observedSelection
              ? `Owned blueprint · ME ${observedSelection.materialEfficiency} · TE ${observedSelection.timeEfficiency}`
              : "Owned blueprint"
            : "Unresearched (ME 0 · TE 0)",
        requiredRuns,
        manual?.kind === "copy"
          ? manual.licensedRuns
          : selection?.mode === "observedAsset"
            ? (selection.licensedRuns ?? (observedSelection?.kind === "copy" ? observedSelection.licensedRuns : null))
            : null,
      ),
    };
  }

  if (buildResolved && (linkedRecipeMode === "manufacturing" || linkedRecipeMode === "reaction")) {
    const role: "manufacturing" | "reaction" = linkedRecipeMode === "reaction" ? "reaction" : "manufacturing";
    const settingsInput = linkedSettings?.build?.draftPlanning?.input;
    const currentFacilityId =
      (role === "reaction" ? settingsInput?.reactionFacility : settingsInput?.manufacturingFacility)
        ?.facilityProfileId ??
      context.facilityOverride?.facilityProfileId ??
      null;
    const facility = allFacilities.find((p) => p.id === currentFacilityId) ?? null;
    model.facility = {
      name: facility?.name ?? null,
      location: facility
        ? [facility.solarSystemName, facility.structureTypeName].filter(Boolean).join(" · ")
        : null,
      bonuses: facility
        ? `Material −${formatPercent(facility.materialReductionPercent)} · Time −${formatPercent(facility.timeReductionPercent)}`
        : null,
      rigCount: facility?.rigs.length ?? 0,
      state: facility ? "set" : currentFacilityId ? "unresolved" : "unset",
      editable: interactive,
      role,
      selectedFacilityId: currentFacilityId,
      summary: facility?.name ?? "Uses build facility",
    };
  }

  // ── actions ──
  const actions: InspectorActions = {
    calculationEvidence: context.calculationEvidence,
  };
  if (model.pricing && handlers.onPricingChange) {
    actions.pricing = { onChange: handlers.onPricingChange };
  }
  if (model.sourcing) {
    actions.sourcing = {
      onBuy: () => {
        handlers.onResolutionChange?.(item.typeId, null);
        if (fullyCoveredByInventory) handlers.onFulfillmentScopeChange?.(item.typeId, "full");
      },
      onBuild: context.recipe
        ? () => handlers.onResolutionChange?.(item.typeId, context.recipe as RecipeSelection)
        : undefined,
      onUseInventory: fullyCoveredByInventory
        ? () => {
            handlers.onResolutionChange?.(item.typeId, null);
            handlers.onFulfillmentScopeChange?.(item.typeId, null);
          }
        : undefined,
      onScope: (next) => handlers.onFulfillmentScopeChange?.(item.typeId, next === "full" ? "full" : null),
    };
  }
  if (model.blueprint) {
    actions.blueprint = {
      pending: linkedSettings?.pending,
      error: linkedSettings?.error ?? null,
      onSelectObservation: (observationId) => {
        if (linkedSettings) void linkedSettings.updateBlueprintSelection({ mode: "observedAsset", observationId });
        else handlers.onBlueprintSelectionChange?.(item.typeId, { mode: "observedAsset", observationId });
      },
      onModelManually: (input) => {
        const selection: BlueprintSelection = { mode: "manual", ...input };
        if (linkedSettings) void linkedSettings.updateBlueprintSelection(selection);
        else handlers.onBlueprintSelectionChange?.(item.typeId, selection);
      },
    };
  }
  if (model.facility) {
    actions.facility = {
      options: allFacilities,
      pending: linkedSettings?.pending,
      error: linkedSettings?.error ?? null,
      onSelect: (facilityProfileId) => {
        if (linkedSettings) {
          void linkedSettings.updateFacility({ facilityProfileId });
        } else if (!facilityProfileId) {
          handlers.onFacilityOverrideChange?.(item.typeId, null);
        } else {
          const profile = allFacilities.find((p) => p.id === facilityProfileId);
          if (profile) {
            handlers.onFacilityOverrideChange?.(item.typeId, {
              facilityProfileId: profile.id,
            });
          }
        }
      },
    };
  }
  if (linkedBuild && context.parentBuildId) {
    actions.openLinkedBuild = () => handlers.onOpenLinkedBuild?.(linkedBuild.id);
    if (handlers.onCopyBuildId) actions.copyBuildId = () => handlers.onCopyBuildId?.(linkedBuild.id);
  }

  return { model, actions };
}

/** Kept for existing tests / lightweight callers that only need the read
 * model (warnings strip, section-shape parity). */
export function worksheetItemToInspectorModel(
  item: WorksheetItem,
  context: { buildResolved?: boolean; pricingContext?: WorksheetInspectorContext["pricingContext"] } = {},
): InspectorModel {
  return buildWorksheetInspector({
    item,
    buildResolved: context.buildResolved ?? item.isBuildResolved,
    linkedBuild: null,
    creatingLinkedBuild: false,
    linkedBuildError: "",
    allFacilities: [],
    rootTypeId: null,
    recipe: null,
    observations: [],
    pricingContext: context.pricingContext ?? null,
    readOnly: true,
    allowMarketPolicyOverride: false,
    linkedSettings: null,
    handlers: {},
  }).model;
}
