import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import { createBuild, listFacilities, type FacilityProfile, type MarketScope } from "../../../api/industry";
import {
  evaluateOpportunities,
  listOpportunityScopes,
  type EvaluateOpportunitiesCommand,
  type OpportunityCandidate,
  type OpportunityEvaluation,
  type ProfitabilityScopeDefinition,
  type ProfitabilityScopeId,
} from "../../../api/opportunities";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { OpportunitiesTable } from "./opportunities-table";
import { OpportunityContextStrip, type OpportunityContextInput } from "./opportunity-context-strip";
import { ExcludedCandidatesSection } from "./opportunity-excluded-section";
import { applyFilters, DEFAULT_FILTERS, type OpportunityFilters } from "./opportunity-filtering";
import { OpportunityFiltersPanel } from "./opportunity-filters";
import type { ValuationMode } from "./opportunity-formatters";
import { FreshnessBanner } from "./opportunity-freshness-banner";
import { OpportunityInspector } from "./opportunity-inspector";
import { OpportunityScopeSelector } from "./opportunity-scope-selector";
import { sortCandidates, type SortDirection, type SortField } from "./opportunity-sorting";
import { apiMessage, type LoadState } from "./shared";

const DEFAULT_MATERIAL_EFFICIENCY = 10;
const DEFAULT_TIME_EFFICIENCY = 20;
// Jita 4-4 -- the same landing scope Build/Order and the Market Browser
// default to; no configured Price Source is required for Opportunities
// scanning to work at all.
const DEFAULT_SCOPE: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

const VALUATION_MODES: { mode: ValuationMode; label: string }[] = [
  { mode: "sellSide", label: "Sell-side" },
  { mode: "immediateLiquidation", label: "Liquidation" },
];

export function OpportunitiesPage() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [scopesState, setScopesState] = useState<LoadState<ProfitabilityScopeDefinition[]>>({ status: "loading" });
  const [facilities, setFacilities] = useState<FacilityProfile[]>([]);
  const [contextState, setContextState] = useState<LoadState<null>>({ status: "loading" });
  const [evaluationState, setEvaluationState] = useState<LoadState<OpportunityEvaluation>>({ status: "loading" });
  const [mode, setMode] = useState<ValuationMode>("sellSide");
  const [sortField, setSortField] = useState<SortField>("profitPerHour");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");
  const [filters, setFilters] = useState<OpportunityFilters>(DEFAULT_FILTERS);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [facilityIdOverride, setFacilityIdOverride] = useState<string | null>(null);
  const [marketScope, setMarketScope] = useState<MarketScope>(DEFAULT_SCOPE);
  const [materialEfficiency, setMaterialEfficiency] = useState(DEFAULT_MATERIAL_EFFICIENCY);
  const [timeEfficiency, setTimeEfficiency] = useState(DEFAULT_TIME_EFFICIENCY);
  const [creatingBuildFor, setCreatingBuildFor] = useState<number | null>(null);
  const [createBuildError, setCreateBuildError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([listOpportunityScopes(), listFacilities()])
      .then(([scopes, loadedFacilities]) => {
        setScopesState({ status: "ready", data: scopes });
        setFacilities(loadedFacilities);
        setContextState({ status: "ready", data: null });
      })
      .catch((error) => {
        setScopesState({ status: "error", message: apiMessage(error) });
        setContextState({ status: "error", message: apiMessage(error) });
      });
  }, []);

  const scopes = scopesState.status === "ready" ? scopesState.data : [];
  const requestedScope = params.get("scope");
  const activeScope: ProfitabilityScopeDefinition | null = useMemo(() => {
    const requested = scopes.find((scope) => scope.id === requestedScope);
    return requested ?? scopes[0] ?? null;
  }, [scopes, requestedScope]);
  const scopeId: ProfitabilityScopeId | null = activeScope?.id ?? null;
  const isReactionScope = activeScope?.recipeKind === "reaction";
  const requiredFacilityRole = isReactionScope ? "reaction" : "manufacturing";

  const facility = useMemo(() => {
    if (facilityIdOverride) {
      const overridden = facilities.find((candidate) => candidate.id === facilityIdOverride);
      if (overridden && overridden.role === requiredFacilityRole) return overridden;
    }
    return facilities.find((candidate) => candidate.role === requiredFacilityRole && candidate.archivedAt === null) ?? null;
  }, [facilities, facilityIdOverride, requiredFacilityRole]);

  const evaluateCommand: EvaluateOpportunitiesCommand | null = useMemo(() => {
    if (!scopeId || !facility) return null;
    return {
      scopeId,
      facilityProfileId: facility.id,
      materialEfficiency: isReactionScope ? null : materialEfficiency,
      timeEfficiency: isReactionScope ? null : timeEfficiency,
      marketScope,
    };
  }, [scopeId, facility, marketScope, materialEfficiency, timeEfficiency, isReactionScope]);

  function applyContext(input: OpportunityContextInput) {
    setFacilityIdOverride(input.facilityId);
    setMarketScope(input.marketScope);
    // null for a reaction scope's context edit -- ME/TE only ever apply to
    // manufacturing, so the stored value is left as-is for next time a
    // manufacturing scope is active.
    if (input.materialEfficiency !== null) setMaterialEfficiency(input.materialEfficiency);
    if (input.timeEfficiency !== null) setTimeEfficiency(input.timeEfficiency);
  }

  async function createBuildFromCandidate(candidate: OpportunityCandidate) {
    setCreateBuildError(null);
    setCreatingBuildFor(candidate.productTypeId);
    try {
      const recipe =
        candidate.recipe.kind === "manufacturing"
          ? { mode: "manufacturing" as const, blueprintTypeId: candidate.recipe.blueprintTypeId }
          : { mode: "reaction" as const, reactionFormulaTypeId: candidate.recipe.reactionFormulaTypeId };
      const build = await createBuild({
        name: candidate.productName,
        recipe,
        runs: 1,
        notes: "",
      });
      navigate(`/builds/${build.id}`);
    } catch (error) {
      setCreateBuildError(apiMessage(error));
      setCreatingBuildFor(null);
    }
  }

  useEffect(() => {
    if (!evaluateCommand) return;
    const controller = new AbortController();
    setEvaluationState({ status: "loading" });
    evaluateOpportunities(evaluateCommand, controller.signal)
      .then((evaluation) => setEvaluationState({ status: "ready", data: evaluation }))
      .catch((error) => {
        if (controller.signal.aborted) return;
        setEvaluationState({ status: "error", message: apiMessage(error) });
      });
    return () => controller.abort();
  }, [evaluateCommand]);

  function selectScope(next: ProfitabilityScopeId) {
    const nextParams = new URLSearchParams(params);
    nextParams.set("scope", next);
    nextParams.delete("candidate");
    setParams(nextParams);
  }

  const selectedProductTypeId = params.has("candidate") ? Number(params.get("candidate")) : null;
  function selectCandidate(productTypeId: number) {
    const nextParams = new URLSearchParams(params);
    if (selectedProductTypeId === productTypeId) nextParams.delete("candidate");
    else nextParams.set("candidate", String(productTypeId));
    setParams(nextParams);
  }
  function closeInspector() {
    const nextParams = new URLSearchParams(params);
    nextParams.delete("candidate");
    setParams(nextParams);
  }
  const selectedCandidate =
    evaluationState.status === "ready"
      ? (evaluationState.data.candidates.find((candidate) => candidate.productTypeId === selectedProductTypeId) ?? null)
      : null;

  const rankedCandidates = useMemo(() => {
    if (evaluationState.status !== "ready") return [];
    const eligible = evaluationState.data.candidates.filter(
      (candidate) => candidate.eligibility.status !== "excludedFromDefaultRanking",
    );
    const filtered = applyFilters(eligible, filters, mode);
    return sortCandidates(filtered, evaluationState.data.rankings, mode, sortField, sortDirection);
  }, [evaluationState, mode, sortField, sortDirection, filters]);

  function toggleSort(field: SortField) {
    if (field === sortField) setSortDirection((direction) => (direction === "desc" ? "asc" : "desc"));
    else {
      setSortField(field);
      setSortDirection("desc");
    }
  }

  return (
    <>
      <PageHeader eyebrow="Manufacturing" title="Opportunities">
        What should I manufacture right now? Category-scoped profitability against current locally cached market
        evidence.
      </PageHeader>

      {scopesState.status === "loading" ? <Panel>Loading Opportunities...</Panel> : null}
      {scopesState.status === "error" ? <InlineAlert title="Opportunities unavailable">{scopesState.message}</InlineAlert> : null}

      {scopesState.status === "ready" && scopes.length > 0 ? (
        <OpportunityScopeSelector onSelect={selectScope} scopes={scopes} selectedScopeId={scopeId} />
      ) : null}

      {contextState.status === "ready" && !facility ? (
        <Panel>
          <EmptyState title={`No ${requiredFacilityRole} facility configured`}>
            Opportunities needs a {requiredFacilityRole}-role facility profile to evaluate against. Set one up
            on the Facilities page first.
          </EmptyState>
        </Panel>
      ) : null}
      {evaluationState.status === "loading" && facility ? <Panel>Evaluating {scopeId}...</Panel> : null}
      {evaluationState.status === "error" ? (
        <InlineAlert title="Evaluation failed">{evaluationState.message}</InlineAlert>
      ) : null}
      {evaluationState.status === "ready" && evaluateCommand ? (
        <>
          <FreshnessBanner evaluation={evaluationState.data} requestCommand={evaluateCommand} />
          <OpportunityContextStrip
            facilities={facilities}
            facility={facility}
            facilityRole={requiredFacilityRole}
            marketScope={marketScope}
            materialEfficiency={evaluationState.data.context.materialEfficiency}
            onApply={applyContext}
            runs={evaluationState.data.context.runs}
            timeEfficiency={evaluationState.data.context.timeEfficiency}
          />
          <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs text-muted">
              {rankedCandidates.length} ranked
              {evaluationState.data.excludedCount > 0 ? ` · ${evaluationState.data.excludedCount} excluded` : ""}
            </p>
            <div className="flex items-center gap-2">
              <button
                aria-pressed={filtersOpen}
                className={`iw-button-secondary ${filtersOpen ? "border-primary text-primary" : ""}`}
                onClick={() => setFiltersOpen((open) => !open)}
                type="button"
              >
                Filters
              </button>
              <div className="flex overflow-hidden rounded-md border border-border text-xs">
                {VALUATION_MODES.map(({ mode: candidateMode, label }) => (
                  <button
                    key={candidateMode}
                    aria-pressed={mode === candidateMode}
                    className={`px-2.5 py-1 font-semibold ${mode === candidateMode ? "bg-primary/15 text-primary" : "text-muted"}`}
                    onClick={() => setMode(candidateMode)}
                    type="button"
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className="flex items-start gap-4">
            {filtersOpen ? (
              <OpportunityFiltersPanel filters={filters} onChange={setFilters} onClose={() => setFiltersOpen(false)} />
            ) : null}
            <div className="min-w-0 flex-1">
              {rankedCandidates.length === 0 ? (
                <Panel>
                  {evaluationState.data.defaultRankingEligibleCount === 0 ? (
                    <EmptyState title="No eligible candidates">
                      Every candidate in this category is either excluded or hasn't been evaluated yet.
                    </EmptyState>
                  ) : (
                    <EmptyState
                      action={
                        <button className="iw-button-secondary" onClick={() => setFilters(DEFAULT_FILTERS)} type="button">
                          Reset filters
                        </button>
                      }
                      title="No results match your filters"
                    >
                      Every eligible candidate in this category is hidden by the current filters.
                    </EmptyState>
                  )}
                </Panel>
              ) : (
                <OpportunitiesTable
                  candidates={rankedCandidates}
                  creatingBuildFor={creatingBuildFor}
                  mode={mode}
                  onCreateBuild={createBuildFromCandidate}
                  onSelectCandidate={selectCandidate}
                  onSort={toggleSort}
                  selectedProductTypeId={selectedProductTypeId}
                  sortDirection={sortDirection}
                  sortField={sortField}
                />
              )}
              {createBuildError ? <InlineAlert title="Create Build failed">{createBuildError}</InlineAlert> : null}
              <ExcludedCandidatesSection candidates={evaluationState.data.candidates} />
            </div>
          </div>
          {selectedCandidate ? (
            <OpportunityInspector
              candidate={selectedCandidate}
              creatingBuild={creatingBuildFor === selectedCandidate.productTypeId}
              excludedCosts={evaluationState.data.excludedCosts}
              facility={facility}
              marketScope={marketScope}
              onClose={closeInspector}
              onCreateBuild={() => createBuildFromCandidate(selectedCandidate)}
            />
          ) : null}
        </>
      ) : null}
    </>
  );
}
