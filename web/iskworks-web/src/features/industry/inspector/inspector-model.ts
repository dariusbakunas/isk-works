import type {
  AutomaticEiv,
  BlueprintObservation,
  MarketPricingPolicy,
  MarketScope,
  PriceSource,
} from "../../../api/industry";

import type { InspectorTone } from "./inspector-section";

/**
 * The canonical, view-agnostic inspector model. The Worksheet item inspector
 * and the Graph node inspector both build one of these (plus an
 * `InspectorActions` capability bundle) from their own source object, and
 * render the SAME `UnifiedItemInspector` component from it. Which view made
 * the selection is not represented here -- only what the selected object is
 * and which capabilities apply to it.
 */
export type InspectorKind =
  | "rootBuild"
  | "linkedBuild"
  | "reaction"
  | "buyMaterial"
  | "buildableBuyMaterial"
  | "unresolvedBuild"
  | "outputItem";

export interface InspectorWarning {
  /** Short stable label, e.g. "Pricing incomplete". */
  label: string;
  /** Longer human-readable detail. */
  detail: string;
  tone: InspectorTone;
}

export interface InspectorIdentity {
  kind: InspectorKind;
  /** Eyebrow, e.g. "BUY MATERIAL", "LINKED BUILD", "REACTION". */
  kindLabel: string;
  name: string;
  /** Subtype / category line, e.g. "Electronic Component" or
   * "T2 Component · Manufacturing". `null` when there's nothing useful. */
  subtitle: string | null;
  typeId: number | null;
  /** Whether to show the type image in the header (Builds/reactions yes,
   * plain materials no -- keeps the header compact). */
  showImage: boolean;
  /** Compact operational digest for the always-visible header. Kept short --
   * "Need 34 · Making 34", "Required 4,800 · 1,200 short". No trailing
   * "· +0". `null` when there's nothing worth a summary line. */
  summary: string | null;
}

export interface InspectorMetric {
  label: string;
  value: string;
  tone?: InspectorTone;
}

export interface CoverageSlice {
  metrics: InspectorMetric[];
  percentage: number;
  hasShortage: boolean;
  /** One-line digest for the collapsed header. */
  summary: string;
}

export interface QuantitiesSlice {
  metrics: InspectorMetric[];
  /** One-line digest for the collapsed header, e.g. "45 runs · +601". */
  summary: string;
  /** Coverage-bar percentage -- present when the target has a material-demand
   * aspect (a linked Build). `undefined` for a pure production/root node. */
  percentage?: number;
  hasShortage?: boolean;
}

/** BUY / BUILD sourcing plus, where it applies, use-from-inventory and the
 * shortage-only / full quantity scope. */
export interface SourcingSlice {
  /** How this dependency is currently sourced. */
  mode: "build" | "buy";
  /** A buildable recipe exists -> the BUY <-> BUILD switch is offered. */
  buildable: boolean;
  /** Short recipe digest, e.g. "Manufacturing". */
  recipeSummary: string | null;
  /** Inventory fully covers this row -> a "Use inventory" option applies. */
  fullyCoveredByInventory: boolean;
  /** Currently drawing this row entirely from inventory. */
  usingInventory: boolean;
  /** There is a shortfall -> the quantity-scope sub-choice applies. */
  hasShortfall: boolean;
  /** "missing" (shortage only) vs "full" (buy/build the whole requirement). */
  scope: "missing" | "full";
  requiredQuantity: number;
  missingQuantity: number;
  availableQuantity: number;
  /** e.g. "Use 900 from inventory and build 100." -- `null` when not split. */
  fulfillmentSentence: string | null;
  /** One-line digest for the collapsed header. */
  summary: string;
}

export type BlueprintMode = "existing" | "manual" | "unresearched";

/** Blueprint selection for a manufacturing Build -- an owned blueprint
 * instance (ME/TE/origin/runs derived from it) or a modelled planning
 * assumption. `null` for a reaction (see `RecipeSlice`). */
export interface BlueprintSlice {
  /** `null` for a reaction. */
  kind: "blueprint";
  name: string | null;
  blueprintTypeId: number | null;
  /** Which planning stance is active. */
  mode: BlueprintMode;
  /** "BPO" / "BPC", from the selected blueprint / modelled kind. */
  origin: "BPO" | "BPC" | null;
  me: number | null;
  te: number | null;
  /** BPC runs remaining, when tracked. */
  licensedRuns: number | null;
  notes: string;
  /** Owned instances of this blueprint from the latest ESI sync. */
  observations: BlueprintObservation[];
  /** The selected observation id (mode === "existing"). */
  selectedObservationId: string | null;
  /** Runs this Build needs -- drives the observed-BPC sufficiency hint. */
  requiredRuns: number;
  /** The backing preview hasn't resolved yet. */
  computing: boolean;
  /** ME/TE are edited on THIS Build (root editor or a linked-Build patch)
   * rather than being read-only. */
  editable: boolean;
  summary: string;
}

/** Reaction recipe -- no ME/TE, no BPO/BPC. */
export interface RecipeSlice {
  kind: "formula";
  name: string | null;
  computing: boolean;
  summary: string;
}

export interface FacilitySlice {
  name: string | null;
  location: string | null;
  bonuses: string | null;
  rigCount: number;
  state: "set" | "unset" | "unresolved" | "computing";
  /** The facility selection is edited on THIS Build. */
  editable: boolean;
  /** Role to filter facility options by. */
  role: "manufacturing" | "reaction";
  /** The facility profile id currently selected on this Build. */
  selectedFacilityId: string | null;
  /** Estimated Item Value control -- root Build only (a build-resolved
   * sub-component's EIV is always resolved server-side). Present (object or
   * `null`) marks a root facility section; absent (`undefined`) is a linked
   * Build. `null` = root, but no facility selected yet -> no EIV control. */
  eiv?: {
    automaticEiv: AutomaticEiv | null;
    manual: boolean;
    value: string;
    loading: boolean;
    error: string;
  } | null;
  summary: string;
}

export type CostState = "known" | "incomplete" | "stale" | "unresolved" | "notComputed";

/** Never an ambiguous single "Cost" -- material, installation and total are
 * distinguished, and an unknown state is explicit. */
export interface CostSlice {
  material: string | null;
  installation: string | null;
  total: string | null;
  state: CostState;
  /** The backing preview hasn't resolved yet -- rows read "Computing…". */
  computing?: boolean;
  summary: string;
}

export type PricingMode = "default" | "policy" | "manual";

/** Row pricing: the captured Price Source default, a per-row policy
 * override, or a manual unit price. */
export interface RowPricingSlice {
  /** Discriminant -- absent means row pricing (back-compat). */
  kind?: "row";
  mode: PricingMode;
  policy: MarketPricingPolicy;
  manualUnitPrice: string | null;
  unitPrice: string | null;
  /** A per-row market-policy override is offered (material rows only). */
  allowPolicyOverride: boolean;
  role: "material" | "output";
  typeId: number;
  /** Read-only rendering (captured Order snapshot). */
  readOnly: boolean;
  summary: string;
}

/** Root Build pricing configuration -- the build-wide material/output market
 * scope + policy and the price-source fallback. A linked Build has no such
 * section (it inherits/derives these). */
export interface RootPricingSlice {
  kind: "root";
  materialScope: MarketScope;
  materialPolicy: MarketPricingPolicy;
  outputScope: MarketScope;
  outputPolicy: MarketPricingPolicy;
  priceSourceId: string;
  priceSources: PriceSource[];
  summary: string;
}

export type PricingSlice = RowPricingSlice | RootPricingSlice;

export interface ValueSlice {
  metrics: InspectorMetric[];
}

export interface RelatedBuildsSlice {
  /** "Used by" for a material, "Inputs" for a reaction/production node. */
  label: string;
  entries: Array<{ typeId: number | null; name: string; quantity: number }>;
  /** Fallback image type id for a root (no parent) contribution. */
  fallbackTypeId: number | null;
}

export interface ProvenanceSlice {
  summary: string;
  lines: InspectorMetric[];
  note: string | null;
  /** Persisted Build id -- demoted here (developer/advanced metadata) with a
   * copy action rather than a prominent row. */
  buildId: string | null;
  /** Recipe-currency chip label + tone, when not "current". */
  recipeCurrency: { label: string; tone: InspectorTone; explanation: string | null } | null;
}

export interface InspectorModel {
  identity: InspectorIdentity;
  warnings: InspectorWarning[];
  /** A transient status line shown under the header, e.g.
   * "Creating linked build…" or a linked-build creation error. */
  statusLine?: { text: string; tone: InspectorTone } | null;
  quantities?: QuantitiesSlice;
  coverage?: CoverageSlice;
  sourcing?: SourcingSlice;
  blueprint?: BlueprintSlice;
  recipe?: RecipeSlice;
  facility?: FacilitySlice;
  cost?: CostSlice;
  pricing?: PricingSlice;
  /** Planned duration in seconds -- root Production node only. */
  durationSeconds?: number | null;
  /** Reaction inputs. */
  inputs?: RelatedBuildsSlice;
  usedBy?: RelatedBuildsSlice;
  value?: ValueSlice;
  provenance?: ProvenanceSlice;
}

/** The capability callbacks a view wires for the sections it enables. Only
 * the ones that apply to the selected object are populated. */
export interface InspectorActions {
  sourcing?: {
    onBuild?: () => void;
    onBuy?: () => void;
    onUseInventory?: () => void;
    onScope?: (scope: "missing" | "full") => void;
    pending?: boolean;
  };
  blueprint?: {
    onSelectObservation?: (observationId: string) => void;
    onModelManually?: (input: {
      kind: "original" | "copy";
      materialEfficiency: number;
      timeEfficiency: number;
      licensedRuns: number | null;
      notes: string;
    }) => void;
    onUnresearched?: () => void;
    pending?: boolean;
    error?: string | null;
  };
  facility?: {
    options: import("../../../api/industry").FacilityProfile[];
    onSelect?: (facilityProfileId: string | null) => void;
    /** Root-only EIV controls (paired with `FacilitySlice.eiv`). */
    onEivManual?: (manual: boolean) => void;
    onEivCommit?: (canonical: string) => void;
    onEivClear?: () => void;
    pending?: boolean;
    error?: string | null;
  };
  pricing?: {
    /** Row pricing. */
    onChange?: (selection: import("../../../api/industry").PlannerPricingSelection) => void;
    /** Root pricing configuration (paired with `RootPricingSlice`). */
    onMaterialScope?: (scope: import("../../../api/industry").MarketScope) => void;
    onMaterialPolicy?: (policy: import("../../../api/industry").MarketPricingPolicy) => void;
    onOutputScope?: (scope: import("../../../api/industry").MarketScope) => void;
    onOutputPolicy?: (policy: import("../../../api/industry").MarketPricingPolicy) => void;
    onPriceSource?: (sourceId: string) => void;
  };
  /** Secondary navigation to the linked Build's own page. */
  openLinkedBuild?: () => void;
  /** Copy the Build id (from Provenance). */
  copyBuildId?: () => void;
  /** Dismiss the inspector -- the canonical header renders a close control
   * when set, so the host shell doesn't render its own duplicate header. */
  onClose?: () => void;
  /** "Exact calculation evidence" disclosure content (Worksheet). */
  calculationEvidence?: import("react").ReactNode;
  /** Rendered verbatim below the sections -- the root `BuildSettingsPanel`. */
  footer?: import("react").ReactNode;
}

/** Canonical section ids + order for the unified inspector. Both adapters
 * map onto this list; a section renders only when its model slice is set.
 * Shared so Worksheet and Graph collapse-state key identically and a parity
 * test can assert identical section structure for the same object. */
export const INSPECTOR_SECTION_ORDER = [
  "quantities",
  "coverage",
  "sourcing",
  "blueprint",
  "recipe",
  "facility",
  "cost",
  "pricing",
  "duration",
  "inputs",
  "usedBy",
  "value",
  "provenance",
] as const;

