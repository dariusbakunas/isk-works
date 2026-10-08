import type {
  BlueprintObservation,
  FacilityProfile,
  GraphWarning,
  PlannerPricingSelection,
  RecipeCurrency,
  RecipeSelection,
  WorksheetItem,
} from "../../../../api/industry";
import {
  type ProductionEnrichment,
} from "../../builds/graph/graph-inspector-enrichment";
import {
  coverageSplit,
  fulfillmentSentence,
  sourcingSummary,
} from "../sourcing-coverage";
import { warningLabel } from "../../builds/graph/graph-warnings";
import type { BuildGraphNodeData } from "../../builds/graph/to-react-flow";
import type { BuildSettings } from "../../builds/use-build-settings";
import type {
  InspectorActions,
  InspectorModel,
  InspectorWarning,
} from "../inspector-model";
import {
  coverageSlice,
  pricingSlice,
  provenanceSlice,
  valueSlice,
} from "./material-slices";
import { linkedBuildInspector, type LinkedBuildInput } from "./linked-build";
import type { ProductionNode } from "../../../../api/industry/build-graph";

const NUMBER = new Intl.NumberFormat("en-US");

export interface GraphInspectorContext {
  enrichment?: ProductionEnrichment | null;
  warnings?: GraphWarning[];
  recipeCurrency?: RecipeCurrency;
  /** Build-ID-addressable settings for a selected *linked* Production node. */
  linkedSettings?: BuildSettings;
  allFacilities?: FacilityProfile[];
  /** Owned blueprint instances for a linked manufacturing node's blueprint. */
  observations?: BlueprintObservation[];
  /** The matching worksheet row for a selected acquisition OR linked
   * production node -- supplies the same coverage / pricing / value /
   * used-by facts the Worksheet inspector shows for the same component. */
  worksheetItem?: WorksheetItem | null;
  pricingContext?: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  allowMarketPolicyOverride?: boolean;
  rootTypeId?: number | null;
  sourcingPending?: boolean;
  /** Quantity scope override for a linked production node whose parent is
   * the root (the same override the Worksheet reads). `undefined` -> the
   * scope control isn't offered (deep node, no worksheet counterpart). */
  fulfillmentScope?: "missing" | "full";
  handlers?: {
    onBuild?: () => void;
    onBuy?: () => void;
    onScope?: (scope: "missing" | "full") => void;
    onOpenLinkedBuild?: (buildId: string) => void;
    onCopyBuildId?: (buildId: string) => void;
    onPricingChange?: (selection: PlannerPricingSelection) => void;
  };
}

/** Informational codes never imply the displayed plan/cost is wrong --
 * `runsDiverged` is priced at the projected runs already, and a stale
 * price is still a usable, complete cost. Everything else (an actually
 * incomplete cost, an unresolved or cyclic child) is blocking. */
function isInformationalWarning(code: GraphWarning["code"]): boolean {
  return (
    code === "marketPriceUnavailable" ||
    code === "staleRecipe" ||
    code === "runsDiverged" ||
    code === "staleMarketEvidence"
  );
}

function mapWarnings(warnings: GraphWarning[]): InspectorWarning[] {
  return warnings.map((warning) => ({
    label: warningLabel(warning.code),
    detail: warning.message,
    tone: isInformationalWarning(warning.code) ? "neutral" : "blocking",
  }));
}

/**
 * Canonical inspector model + actions for a selected Build Graph node -- the
 * Graph-side counterpart of `buildWorksheetInspector`. Both feed the one
 * `UnifiedItemInspector`.
 */
export function buildGraphInspector(
  data: BuildGraphNodeData,
  context: GraphInspectorContext = {},
): { model: InspectorModel; actions: InspectorActions } {
  const warnings = mapWarnings(context.warnings ?? []);
  if (data.nodeType === "production") {
    return graphLinkedBuild(data.node, context, warnings);
  }
  if (data.nodeType === "acquisition") {
    return acquisitionInspector(data, context, warnings);
  }
  if (data.nodeType === "unresolvedBuild") {
    return unresolvedInspector(data, warnings);
  }
  // The root Build is not a graph-node target -- the host builds it with
  // `buildRootInspector(editor)` directly.
  throw new Error("buildGraphInspector: root nodes are handled by buildRootInspector");
}

/** Resolve a selected linked Production node to the canonical
 * `LinkedBuildInput`. The Worksheet builds the same input for the same
 * `buildId`, so the rendered inspector is identical. */
function graphLinkedBuild(
  node: ProductionNode,
  context: GraphInspectorContext,
  warnings: InspectorWarning[],
): { model: InspectorModel; actions: InspectorActions } {
  const isReaction = node.kind === "reaction" || node.kind === "rootReaction";
  const enrichment = context.enrichment ?? null;
  const settings = context.linkedSettings ?? null;
  const row = context.worksheetItem ?? null;
  const settingsInput = settings?.build?.draftPlanning?.input;
  const blueprintSelection = settingsInput?.blueprintSelection ?? null;
  // The linked-node enrichment already resolved the facility profile from the
  // persisted Build; prefer it (the graph context may not carry the full
  // facility collection).
  const facilityProfileId =
    enrichment?.facility?.id ??
    (isReaction ? settingsInput?.reactionFacility : settingsInput?.manufacturingFacility)
      ?.facilityProfileId ??
    null;
  const contextFacilities = context.allFacilities ?? [];
  const facilities =
    enrichment?.facility && !contextFacilities.some((p) => p.id === enrichment.facility!.id)
      ? [...contextFacilities, enrichment.facility]
      : contextFacilities;
  const input: LinkedBuildInput = {
    buildId: node.buildId,
    parentBuildId: node.parentBuildId,
    componentTypeId: node.parentComponentTypeId ?? node.typeId,
    typeId: node.typeId,
    typeName: node.typeName,
    recipeMode: isReaction ? "reaction" : "manufacturing",
    blueprintTypeId: node.recipe.mode === "manufacturing" ? node.recipe.blueprintTypeId : null,
    blueprintName: enrichment?.blueprintName ?? null,
    formulaName: enrichment?.formulaName ?? null,
    selectedBlueprintOrigin:
      settings?.build?.selectedBlueprintOrigin ?? enrichment?.blueprintOrigin ?? null,
    blueprintSelection,
    effectiveMe: node.effectiveMe,
    effectiveTe: node.effectiveTe,
    observations: context.observations ?? [],
    blueprintComputing: enrichment?.computing ?? !enrichment,
    worksheetRow: row,
    runs: node.runs,
    persistedRuns: node.persistedRuns,
    producingQuantity: node.producingQuantity,
    surplus: node.surplus,
    fallbackRequiredQuantity: node.requiredQuantity,
    netRequiredQuantity: node.netRequiredQuantity,
    // This node's own material / installation / total -- its
    // own operation's full cost, additive (`estimatedCost ==
    // materialComponentCost + ownInstallationCost`), never the parent's
    // prorated *consumed* contribution (`row.lineTotal`/`installationCost`,
    // which differs from this whenever there's a discrete-output surplus).
    cost: {
      material: node.materialComponentCost,
      installation: node.ownInstallationCost,
      total: node.estimatedCost,
      state: node.costState,
    },
    facilityProfileId,
    facilities,
    facilityUnresolved: enrichment?.facilityUnresolved ?? false,
    pricingContext: context.pricingContext ?? null,
    allowMarketPolicyOverride: context.allowMarketPolicyOverride ?? false,
    fulfillmentScope: context.fulfillmentScope ?? "missing",
    recipeCurrency: context.recipeCurrency ?? node.recipeCurrency ?? "current",
    warnings,
    statusLine: null,
    interactive: Boolean(settings?.build),
    settings,
    handlers: {
      onOpenLinkedBuild: context.handlers?.onOpenLinkedBuild,
      onCopyBuildId: context.handlers?.onCopyBuildId,
      onPricingChange: context.handlers?.onPricingChange,
      onSwitchToBuy: context.handlers?.onBuy,
      onScope: context.handlers?.onScope,
    },
  };
  return linkedBuildInspector(input);
}

function acquisitionInspector(
  data: Extract<BuildGraphNodeData, { nodeType: "acquisition" }>,
  context: GraphInspectorContext,
  warnings: InspectorWarning[],
): { model: InspectorModel; actions: InspectorActions } {
  const node = data.node;
  const buildable = node.buildableRecipe != null;
  const wsItem = context.worksheetItem ?? null;
  // Live coverage from the graph DTO. `missingQuantity` already reflects
  // fulfillment scope (a `Full`-scoped node has `missing === required`), so
  // this never needs raw physical inventory. Never read a frozen Epic
  // `reused_quantity` here -- the graph is the *live* Build.
  const cov = coverageSplit(node.requiredQuantity, node.missingQuantity);

  const model: InspectorModel = {
    identity: {
      kind: buildable ? "buildableBuyMaterial" : "buyMaterial",
      kindLabel: "BUY MATERIAL",
      name: node.typeName,
      subtitle: buildable ? "Buildable component" : "Raw material",
      typeId: node.typeId,
      showImage: false,
      summary: cov.partiallyCovered
        ? `Required ${NUMBER.format(cov.required)} · ${NUMBER.format(cov.remaining)} short`
        : `Required ${NUMBER.format(cov.required)}`,
    },
    warnings,
    sourcing: {
      mode: "buy",
      buildable,
      recipeSummary: buildable
        ? (node.buildableRecipe as RecipeSelection).mode === "manufacturing"
          ? "Manufacturing"
          : "Reaction"
        : null,
      fullyCoveredByInventory: cov.fullyCovered,
      usingInventory: cov.fullyCovered,
      hasShortfall: cov.partiallyCovered,
      scope: "missing",
      requiredQuantity: cov.required,
      missingQuantity: cov.remaining,
      availableQuantity: wsItem?.availableQuantity ?? cov.inventory,
      fulfillmentSentence: fulfillmentSentence(cov, "buy"),
      summary: sourcingSummary(cov, { base: "Buy", buildable }),
    },
  };

  if (wsItem) {
    model.coverage = coverageSlice(wsItem);
    model.value = valueSlice(wsItem);
    model.pricing = pricingSlice(wsItem, {
      allowPolicyOverride: context.allowMarketPolicyOverride ?? false,
      readOnly: false,
      sourceName: context.pricingContext?.sourceName,
    });
    model.provenance = provenanceSlice({
      pricingContext: context.pricingContext,
      sourceNote: wsItem.pricing.sourceNote,
      buildId: null,
      recipeCurrency: null,
    });
    if (wsItem.contributions.length > 0) {
      model.usedBy = {
        label: "Used By",
        fallbackTypeId: context.rootTypeId ?? null,
        entries: wsItem.contributions.map((c) => ({
          typeId: c.parentTypeId,
          name: c.parentTypeName,
          quantity: c.quantity,
        })),
      };
    }
  } else {
    // No matching worksheet row (e.g. graph without a resident estimate):
    // fall back to the projection's own quantities.
    model.quantities = {
      metrics: [
        { label: "Required", value: NUMBER.format(cov.required) },
        {
          label: "Inventory",
          value: NUMBER.format(cov.inventory),
          tone: cov.inventory > 0 ? "positive" : "neutral",
        },
        {
          label: cov.fullyCovered ? "Covered" : "To buy",
          value: NUMBER.format(cov.fullyCovered ? cov.inventory : cov.remaining),
          tone: cov.fullyCovered ? "positive" : cov.partiallyCovered ? "blocking" : "neutral",
        },
      ],
      summary: cov.fullyCovered
        ? "Fully covered by inventory"
        : cov.partiallyCovered
          ? `${NUMBER.format(cov.remaining)} to buy`
          : `Buy ${NUMBER.format(cov.remaining)}`,
    };
  }

  const actions: InspectorActions = {};
  if (buildable && context.handlers?.onBuild) {
    actions.sourcing = { onBuild: context.handlers.onBuild, pending: context.sourcingPending };
  }
  if (model.pricing && context.handlers?.onPricingChange) {
    actions.pricing = { onChange: context.handlers.onPricingChange };
  }
  return { model, actions };
}

function unresolvedInspector(
  data: Extract<BuildGraphNodeData, { nodeType: "unresolvedBuild" }>,
  warnings: InspectorWarning[],
): { model: InspectorModel; actions: InspectorActions } {
  const node = data.node;
  return {
    model: {
      identity: {
        kind: "unresolvedBuild",
        kindLabel: "BUILD (RESOLVING)",
        name: node.typeName,
        subtitle: "Linked build pending",
        typeId: node.typeId,
        showImage: false,
        summary: `Required ${NUMBER.format(node.requiredQuantity)}`,
      },
      warnings,
      quantities: {
        metrics: [
          { label: "Required", value: NUMBER.format(node.requiredQuantity) },
          ...(node.netRequiredQuantity !== node.requiredQuantity
            ? [{ label: "Net required", value: NUMBER.format(node.netRequiredQuantity) }]
            : []),
        ],
        summary: "Resolving linked build…",
      },
    },
    actions: {},
  };
}
