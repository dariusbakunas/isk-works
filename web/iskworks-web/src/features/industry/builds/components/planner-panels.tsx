import { Pencil } from "lucide-react";
import {
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";

import type {
  AutomaticEiv,
  BlueprintSelection,
  ComponentFacilityOverride,
  ComponentResolution,
  FulfillmentScope,
  FulfillmentScopeOverride,
  MarketPricingPolicy,
  RecipeSelection,
  PlannerPricingSelection,
} from "../../../../api/industry";
import type { BuildPlanLine } from "../../../../api/sde";
import { MoneyInput } from "../../../../components/money-input";
import { formatDate, formatMoney } from "../../shared/formatting";
export { BlueprintSelectionSection, BlueprintPlanningDialog } from "../planner/blueprint-selection-section";

export function pricingPolicyLabel(policy: MarketPricingPolicy | null): string {
  switch (policy) {
    case "highestBuy": return "Use highest buy order";
    case "acquireQuantityFromSellOrders": return "Buy immediately";
    case "lowestSell": return "Use lowest sell order";
    case "liquidateQuantityIntoBuyOrders": return "Sell immediately";
    default: return "Manual price";
  }
}

/** Same policies as {@link pricingPolicyLabel}, compressed for the collapsed
 * build-settings summary chips -- the dialog's own selects keep the fuller
 * option text above. */
export function pricingPolicyShortLabel(policy: MarketPricingPolicy | null): string {
  switch (policy) {
    case "highestBuy": return "Highest buy";
    case "acquireQuantityFromSellOrders": return "Buy immediately";
    case "lowestSell": return "Lowest sell";
    case "liquidateQuantityIntoBuyOrders": return "Sell immediately";
    default: return "Manual price";
  }
}

/**
 * The Blueprint/Recipe cell -- its own standalone cell (not folded into
 * `BuildSettingsDialog`) because Blueprint selection has its own dialog,
 * gates `runs`, and doesn't apply to reaction builds.
 */
export function BlueprintSummaryCell({
  cellLabel = "Blueprint",
  label,
  onEdit,
}: {
  cellLabel?: string;
  label: string;
  onEdit?: () => void;
}) {
  return (
    <div className="flex min-w-0 items-center gap-1">
      <span className="min-w-0">
        <span className="iw-eyebrow block">{cellLabel}</span>
        <strong className="block max-w-[16rem] truncate text-xs" title={label}>{label}</strong>
      </span>
      {onEdit ? (
        <button aria-label="Edit blueprint" className="iw-icon-button shrink-0" onClick={onEdit} title="Edit blueprint" type="button">
          <Pencil aria-hidden="true" className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}

export function parseRuns(value: string): number | null {
  const runs = Number(value);
  return Number.isInteger(runs) && runs >= 1 && runs <= 1_000_000 ? runs : null;
}

export function createManualBlueprintSelection(
  kind: "original" | "copy",
  meValue: string,
  teValue: string,
  licensedRunsValue: string,
  notes: string,
): Extract<BlueprintSelection, { mode: "manual" }> {
  const me = Number(meValue);
  const te = Number(teValue);
  const copyRuns = licensedRunsValue ? Number(licensedRunsValue) : null;
  if (!Number.isInteger(me) || me < 0 || me > 10) throw new Error("Blueprint ME must be a whole number from 0 through 10.");
  if (!Number.isInteger(te) || te < 0 || te > 20) throw new Error("Blueprint TE must be a whole number from 0 through 20.");
  if (kind === "copy" && (!copyRuns || !Number.isInteger(copyRuns) || copyRuns < 1)) throw new Error("A blueprint copy requires positive licensed runs.");
  return { mode: "manual", kind, materialEfficiency: me, timeEfficiency: te, licensedRuns: kind === "copy" ? copyRuns : null, notes };
}

export function FacilityEivControl({
  automaticEiv,
  error,
  loading,
  manual,
  value,
  onManual,
  onCommitValue,
  onClearValue,
}: {
  automaticEiv: AutomaticEiv | null;
  error: string;
  loading: boolean;
  manual: boolean;
  value: string;
  onManual: (manual: boolean) => void;
  /** A validated, canonical EIV string -- the only value that reaches the build. */
  onCommitValue: (canonical: string) => void;
  /** The user cleared the manual EIV -- revert to the automatic basis. */
  onClearValue: () => void;
}) {
  return (
    <details className="border-t border-border px-2 py-1.5">
      <summary className="cursor-pointer text-xs font-semibold">
        Installation cost basis · {loading ? "calculating EIV" : value ? formatMoney(value) : "EIV unavailable"}
      </summary>
      <div className="mt-2 flex flex-wrap items-end gap-2">
        {manual || error || (!loading && automaticEiv && !automaticEiv.value) ? (
          <label className="min-w-56 flex-1 block text-sm font-semibold">
            Adjusted-price EIV (ISK)
            <MoneyInput
              aria-label="Adjusted-price EIV (ISK)"
              className="mt-1"
              commitDebounceMs={250}
              onClear={onClearValue}
              onCommit={onCommitValue}
              value={value}
            />
          </label>
        ) : (
          <p className="iw-muted flex-1 text-xs">
            CCP adjusted-price EIV synced {automaticEiv?.observedAt ? formatDate(automaticEiv.observedAt) : "automatically"}.
          </p>
        )}
        <button
          className="iw-button-secondary"
          onClick={() => onManual(!manual)}
          type="button"
        >
          {manual ? "Use automatic EIV" : "Enter manually"}
        </button>
      </div>
      {error ? <p className="mt-2 text-xs text-warning">{error}</p> : null}
    </details>
  );
}

export function Metric({ label, value, detail }: { label: string; value: ReactNode; detail: string }) {
  return <div className="border-l-2 border-primary bg-panel px-3 py-2"><span className="iw-eyebrow">{label}</span><strong className="mt-1 block break-words font-mono text-lg">{value}</strong><small className="text-muted">{detail}</small></div>;
}

export function buildPricingSelections(
  recipe: { materials: BuildPlanLine[]; products: BuildPlanLine[] },
  prices: Record<number, string>,
  policies: Record<number, MarketPricingPolicy>,
  materialPolicy: MarketPricingPolicy,
  outputPolicy: MarketPricingPolicy,
): PlannerPricingSelection[] {
  const productTypeIds = new Set(recipe.products.map((item) => item.typeId));
  const lines = [...recipe.materials, ...recipe.products];
  const seen = new Set<string>();
  const selections: PlannerPricingSelection[] = [];
  for (const line of lines) {
    const role = productTypeIds.has(line.typeId) ? "output" as const : "material" as const;
    const key = `${role}:${line.typeId}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const manualPrice = prices[line.typeId]?.trim();
    if (manualPrice) {
      selections.push({
        typeId: line.typeId,
        role,
        selection: { kind: "manual" as const, unit_price: manualPrice },
      });
      continue;
    }
    const policy = policies[line.typeId] ?? (role === "material" ? materialPolicy : outputPolicy);
    const defaultPolicy = role === "material" ? "highestBuy" : "lowestSell";
    if (policy !== defaultPolicy) {
      selections.push({
        typeId: line.typeId,
        role,
        selection: { kind: "market_policy" as const, policy },
      });
    }
  }
  return selections;
}

export function applyPricingSelection(
  selection: PlannerPricingSelection,
  setPrices: Dispatch<SetStateAction<Record<number, string>>>,
  setPolicies: Dispatch<SetStateAction<Record<number, MarketPricingPolicy>>>,
) {
  setPrices((current) => {
    const next = { ...current };
    if (selection.selection.kind === "manual") {
      next[selection.typeId] = selection.selection.unit_price;
    } else {
      delete next[selection.typeId];
    }
    return next;
  });
  setPolicies((current) => {
    const next = { ...current };
    if (selection.selection.kind === "market_policy") {
      next[selection.typeId] = selection.selection.policy;
    } else {
      delete next[selection.typeId];
    }
    return next;
  });
}

export interface ComponentResolutionState {
  recipe: RecipeSelection;
  facilityOverride?: ComponentFacilityOverride;
  blueprintSelection?: BlueprintSelection;
}

export function buildComponentResolutions(
  resolutions: Record<number, ComponentResolutionState>,
): ComponentResolution[] {
  return Object.entries(resolutions).map(([typeId, { recipe, facilityOverride, blueprintSelection }]) => ({
    typeId: Number(typeId),
    recipe,
    ...(facilityOverride ? { facilityOverride } : {}),
    ...(blueprintSelection ? { blueprintSelection } : {}),
  }));
}

export function applyComponentResolution(
  typeId: number,
  recipe: RecipeSelection | null,
  setComponentResolutions: Dispatch<SetStateAction<Record<number, ComponentResolutionState>>>,
) {
  setComponentResolutions((current) => {
    const next = { ...current };
    if (recipe) {
      next[typeId] = { recipe };
    } else {
      delete next[typeId];
    }
    return next;
  });
}

export function applyComponentFacilityOverride(
  typeId: number,
  facilityOverride: ComponentFacilityOverride | null,
  setComponentResolutions: Dispatch<SetStateAction<Record<number, ComponentResolutionState>>>,
) {
  setComponentResolutions((current) => {
    const existing = current[typeId];
    if (!existing) return current;
    return {
      ...current,
      [typeId]: {
        recipe: existing.recipe,
        ...(existing.blueprintSelection ? { blueprintSelection: existing.blueprintSelection } : {}),
        ...(facilityOverride ? { facilityOverride } : {}),
      },
    };
  });
}

export function applyComponentBlueprintSelection(
  typeId: number,
  blueprintSelection: BlueprintSelection | null,
  setComponentResolutions: Dispatch<SetStateAction<Record<number, ComponentResolutionState>>>,
) {
  setComponentResolutions((current) => {
    const existing = current[typeId];
    if (!existing) return current;
    return {
      ...current,
      [typeId]: {
        recipe: existing.recipe,
        ...(existing.facilityOverride ? { facilityOverride: existing.facilityOverride } : {}),
        ...(blueprintSelection ? { blueprintSelection } : {}),
      },
    };
  });
}

export function buildFulfillmentScopes(
  scopes: Record<number, FulfillmentScope>,
): FulfillmentScopeOverride[] {
  return Object.entries(scopes).map(([typeId, scope]) => ({ typeId: Number(typeId), scope }));
}

export function applyFulfillmentScope(
  typeId: number,
  scope: FulfillmentScope | null,
  setFulfillmentScopes: Dispatch<SetStateAction<Record<number, FulfillmentScope>>>,
) {
  setFulfillmentScopes((current) => {
    const next = { ...current };
    if (scope) {
      next[typeId] = scope;
    } else {
      delete next[typeId];
    }
    return next;
  });
}

export { CreateCandidateResults, WorksheetResults } from "./plan-results";
