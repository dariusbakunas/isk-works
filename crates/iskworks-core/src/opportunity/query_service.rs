//! `OpportunityQueryService`: the request -> discovery -> per-candidate
//! projection -> ranking -> response pipeline (`evaluate`) and the
//! refresh-prioritization entry point (`prioritize_evidence`), plus the
//! response DTOs those two methods assemble. Orchestration only -- ordering,
//! validation, and repository batching are unchanged.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::Serialize;

use iskworks_sde::{CandidateRecipeKind, SdeReadRepository};

use crate::{
    AdjustedPriceRepository, BuildRecipe, FacilityProfileId, FacilityRole, IndustryRepository,
    IndustryService, MarketPricingPolicy, MarketRefreshState, MarketRepository, MarketScope,
    OpportunityEligibilityStatus, OwnerId, WorkspaceId,
};

use super::candidate_projection::{
    capture_candidate_recipe, opportunity_market_demand, project_candidate, OpportunityCandidate,
};
use super::command::{normalize_evaluation, EvaluateOpportunitiesCommand, OpportunityError};
use super::evidence::{
    adjusted_price_readiness, classify_evidence, OpportunityCompleteness, OpportunityEvidenceState,
    OpportunityEvidenceStatus, OpportunityWarning, OpportunityWarningKind,
};
use super::ranking::{project_rankings, sort_opportunity_candidates, OpportunityRankings};
use super::scope_catalog::{
    profitability_scope, supported_profitability_scopes, ProfitabilityScopeDefinition,
    ProfitabilityScopeId,
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityEvaluationContext {
    pub scope_id: ProfitabilityScopeId,
    pub facility_profile_id: FacilityProfileId,
    pub facility_revision: u64,
    pub market_region_id: i64,
    pub market_location_id: Option<i64>,
    pub material_efficiency: Option<u8>,
    pub time_efficiency: Option<u8>,
    pub runs: u64,
    pub material_pricing_policy: MarketPricingPolicy,
    pub output_pricing_policy: MarketPricingPolicy,
    pub inventory_reuse_enabled: bool,
    pub recursive_component_expansion_enabled: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityEvaluation {
    pub context: OpportunityEvaluationContext,
    pub calculated_at: DateTime<Utc>,
    pub elapsed_milliseconds: u64,
    pub candidate_count: u64,
    pub complete_count: u64,
    pub incomplete_count: u64,
    pub default_ranking_eligible_count: u64,
    pub excluded_count: u64,
    pub strong_evidence_count: u64,
    pub qualified_evidence_count: u64,
    pub weak_evidence_count: u64,
    #[serde(skip)]
    pub required_market_type_ids: Vec<i64>,
    pub readiness: OpportunityReadiness,
    pub candidates: Vec<OpportunityCandidate>,
    pub rankings: OpportunityRankings,
    pub excluded_costs: Vec<crate::OpportunityExcludedCost>,
    pub warnings: Vec<OpportunityWarning>,
    pub assumptions: Vec<String>,
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunitySystemIndexSource {
    Manual,
    Observed,
    Missing,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityReadiness {
    pub registered_at: DateTime<Utc>,
    pub local_read_at: DateTime<Utc>,
    pub required_material_type_count: u64,
    pub required_output_type_count: u64,
    pub market_fresh_count: u64,
    pub market_stale_count: u64,
    pub market_missing_count: u64,
    pub market_pending_count: u64,
    pub market_failed_count: u64,
    pub oldest_market_observed_at: Option<DateTime<Utc>>,
    pub newest_market_observed_at: Option<DateTime<Utc>>,
    pub adjusted_prices: OpportunityEvidenceStatus,
    pub system_index_source: OpportunitySystemIndexSource,
    pub system_index: OpportunityEvidenceStatus,
    pub stale_but_complete_count: u64,
    pub refresh_pending: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityRefreshDisposition {
    Accepted,
    AlreadyPending,
    NotRequired,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityRefreshAcceptance {
    pub scope_id: ProfitabilityScopeId,
    pub market: OpportunityRefreshDisposition,
    pub adjusted_prices: OpportunityRefreshDisposition,
    pub system_index: OpportunityRefreshDisposition,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct OpportunityQueryService {
    industry_repository: Arc<dyn IndustryRepository>,
    sde_repository: Arc<dyn SdeReadRepository>,
    market_repository: Arc<dyn MarketRepository>,
    adjusted_price_repository: Arc<dyn AdjustedPriceRepository>,
    freshness: OpportunityEvidenceFreshness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpportunityEvidenceFreshness {
    pub market: chrono::Duration,
    pub adjusted_prices: chrono::Duration,
    pub system_index: chrono::Duration,
}

impl Default for OpportunityEvidenceFreshness {
    fn default() -> Self {
        Self {
            market: chrono::Duration::minutes(15),
            adjusted_prices: chrono::Duration::hours(6),
            system_index: chrono::Duration::hours(1),
        }
    }
}

impl OpportunityQueryService {
    #[must_use]
    pub fn new(
        industry_repository: Arc<dyn IndustryRepository>,
        sde_repository: Arc<dyn SdeReadRepository>,
        market_repository: Arc<dyn MarketRepository>,
        adjusted_price_repository: Arc<dyn AdjustedPriceRepository>,
    ) -> Self {
        Self {
            industry_repository,
            sde_repository,
            market_repository,
            adjusted_price_repository,
            freshness: OpportunityEvidenceFreshness::default(),
        }
    }

    #[must_use]
    pub fn with_freshness(mut self, freshness: OpportunityEvidenceFreshness) -> Self {
        self.freshness = freshness;
        self
    }

    #[must_use]
    pub fn scopes(&self) -> Vec<ProfitabilityScopeDefinition> {
        supported_profitability_scopes()
    }

    pub async fn evaluate(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: EvaluateOpportunitiesCommand,
        calculated_at: DateTime<Utc>,
    ) -> Result<OpportunityEvaluation, OpportunityError> {
        let started = Instant::now();
        let normalized = normalize_evaluation(command)?;
        let scope =
            profitability_scope(normalized.scope_id).ok_or(OpportunityError::UnknownScope)?;
        let active_sde = self.sde_repository.active_sde().await?.ok_or_else(|| {
            OpportunityError::StaticData("no active SDE is available".to_string())
        })?;
        if active_sde.counts.categories == 0
            || active_sde.counts.groups == 0
            || active_sde.counts.meta_groups == 0
            || active_sde.counts.classified_types == 0
        {
            return Err(OpportunityError::StaticData(
                "the active SDE does not include category/group/meta-group classification"
                    .to_string(),
            ));
        }
        let candidates = self
            .sde_repository
            .manufacturable_candidates(&scope.candidate_scope)
            .await?;
        let industry = IndustryService::new(
            Arc::clone(&self.industry_repository),
            Arc::clone(&self.sde_repository),
        );
        let mut facility = industry
            .resolve_facility_context(
                workspace_id,
                normalized.facility_profile_id,
                facility_role_for_recipe_kind(scope.recipe_kind),
            )
            .await?;
        // Get-or-create the workspace's ESI source for this scope
        // (`ensure_esi_price_source_for_scope`, same call Build/Order preview
        // makes) -- coverage/refresh bookkeeping (`market_source_coverage`,
        // the worker's refresh loop) and the order-book fetch below are
        // keyed internally by a `PriceSourceId`/`MarketPriceSource`,
        // but the evaluation command/response never expose one: `MarketScope`
        // is the only identity a caller selects or sees. Fetched fresh every
        // call, so there's nothing stale to validate the way an explicit
        // caller-supplied selection would need (revision/archived/kind
        // checks) -- this is always the just-resolved current source.
        let source_id = self
            .market_repository
            .ensure_esi_price_source_for_scope(workspace_id, normalized.market_scope)
            .await?;
        let market_source = self
            .market_repository
            .get_market_price_source(workspace_id, source_id)
            .await?;

        let recipes = candidates
            .into_iter()
            .map(|candidate| {
                let classification = candidate.classification.clone();
                capture_candidate_recipe(candidate, &scope.candidate_scope)
                    .map(|recipe| (recipe, classification))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let market_demand = opportunity_market_demand(
            &recipes
                .iter()
                .map(|(recipe, _)| recipe.clone())
                .collect::<Vec<_>>(),
        );
        let required_material_type_count = u64::try_from(
            recipes
                .iter()
                .flat_map(|(recipe, _)| recipe.materials().iter().map(|line| line.type_id))
                .collect::<BTreeSet<_>>()
                .len(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let required_output_type_count = u64::try_from(
            recipes
                .iter()
                .map(|(recipe, _)| recipe.primary_product().type_id)
                .collect::<BTreeSet<_>>()
                .len(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        // A public scope's books are app-wide: register demand there and read
        // its refresh state from the app-wide rows. A structure scope keeps
        // the workspace's own coverage.
        let coverage = match self
            .public_region(workspace_id, normalized.market_scope)
            .await?
        {
            Some(region_id) => {
                let type_ids: Vec<i64> = market_demand.iter().map(|item| item.type_id).collect();
                self.market_repository
                    .register_public_market_demand(region_id, market_demand, false, calculated_at)
                    .await?;
                self.market_repository
                    .public_market_coverage(region_id, &type_ids)
                    .await?
            }
            None => {
                self.market_repository
                    .register_market_coverage(workspace_id, source_id, market_demand)
                    .await?
            }
        };
        self.adjusted_price_repository
            .register_adjusted_price_refresh(calculated_at)
            .await?;
        let (system_index_source, system_index) = if facility.manual_system_cost_index.is_some() {
            (
                OpportunitySystemIndexSource::Manual,
                OpportunityEvidenceStatus {
                    state: OpportunityEvidenceState::Fresh,
                    usable: true,
                    observed_at: None,
                    age_seconds: None,
                    refresh_pending: false,
                    last_refresh_error: None,
                },
            )
        } else if let Some(solar_system_id) = facility.solar_system_id {
            self.adjusted_price_repository
                .register_system_cost_index(solar_system_id, calculated_at)
                .await?;
            let observed = self
                .adjusted_price_repository
                .latest_system_cost_index(solar_system_id)
                .await?;
            facility.manual_system_cost_index = observed.map(|(value, _)| value);
            let overlay = self
                .adjusted_price_repository
                .system_cost_index_refresh_overlay(solar_system_id)
                .await?;
            let mut status = classify_evidence(
                observed.map(|(_, observed_at)| observed_at),
                calculated_at,
                self.freshness.system_index,
            );
            status.refresh_pending = overlay.pending;
            status.last_refresh_error = overlay.last_error;
            (
                if observed.is_some() {
                    OpportunitySystemIndexSource::Observed
                } else {
                    OpportunitySystemIndexSource::Missing
                },
                status,
            )
        } else {
            (
                OpportunitySystemIndexSource::Missing,
                classify_evidence(None, calculated_at, self.freshness.system_index),
            )
        };
        let material_type_ids = recipes
            .iter()
            .flat_map(|(recipe, _)| recipe.materials().iter().map(|line| line.type_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let adjusted_prices = self
            .adjusted_price_repository
            .latest_adjusted_prices(&material_type_ids, calculated_at)
            .await?;
        let adjusted_price_observed_at = self
            .adjusted_price_repository
            .latest_adjusted_price_observed_at(&material_type_ids)
            .await?;
        let adjusted_overlay = self
            .adjusted_price_repository
            .adjusted_price_refresh_overlay()
            .await?;

        let mut facility_previews = Vec::with_capacity(recipes.len());
        let mut candidate_exclusions = Vec::new();
        let mut market_type_ids = BTreeSet::new();
        for (recipe, classification) in recipes {
            let product = crate::ProductClassification {
                category_id: classification.category_id,
                group_id: classification.group_id,
            };
            let preview =
                crate::calculate_adjusted_price_eiv(recipe.materials(), 1, &adjusted_prices)
                    .and_then(|eiv| match &recipe {
                        BuildRecipe::Manufacturing(manufacturing) => crate::preview_facility(
                            manufacturing,
                            1,
                            facility.clone(),
                            product,
                            // `normalize_evaluation` guarantees Some for a manufacturing
                            // scope; the fallback is unreachable, not a real default.
                            normalized.material_efficiency.unwrap_or(0),
                            normalized.time_efficiency.unwrap_or(0),
                            eiv.value,
                            None,
                        ),
                        BuildRecipe::Reaction(formula) => crate::preview_reaction_facility(
                            formula,
                            1,
                            facility.clone(),
                            product,
                            eiv.value,
                        )
                        .map(Into::into),
                    });
            let preview = match preview {
                Ok(preview) => preview,
                Err(error) => {
                    candidate_exclusions.push(format!(
                        "{} could not be evaluated: {error}",
                        recipe.primary_product().type_name
                    ));
                    continue;
                }
            };
            market_type_ids.extend(preview.requirements.iter().map(|line| line.type_id));
            market_type_ids.insert(recipe.primary_product().type_id);
            facility_previews.push((recipe, classification, preview));
        }
        let market_type_ids = market_type_ids.into_iter().collect::<Vec<_>>();
        let books = self
            .market_repository
            .get_source_order_books(
                workspace_id,
                market_source.id,
                &market_type_ids,
                market_source.config.location_id,
                market_source.config.pinned_batch_id,
            )
            .await?;
        let local_read_at = Utc::now();
        // Physical provenance -- feeds `oldest_market_observed_at` /
        // `newest_market_observed_at`, which report how old the actual
        // order rows are regardless of any later `304` confirmation.
        let market_observed_at = books
            .values()
            .map(|book| book.observed_at)
            .collect::<Vec<_>>();
        // Effective freshness -- a `304` re-confirms the snapshot, so the
        // fresh/stale split counts a recently-revalidated old snapshot as
        // fresh.
        let market_fresh_count = u64::try_from(
            books
                .values()
                .filter(|book| {
                    calculated_at.signed_duration_since(book.effective_observed_at())
                        <= self.freshness.market
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let market_stale_count = u64::try_from(market_observed_at.len())
            .map_err(|_| OpportunityError::ArithmeticOverflow)?
            .checked_sub(market_fresh_count)
            .ok_or(OpportunityError::ArithmeticOverflow)?;
        let market_missing_count = u64::try_from(market_type_ids.len().saturating_sub(books.len()))
            .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let market_pending_count = u64::try_from(
            coverage
                .iter()
                .filter(|item| {
                    market_type_ids.binary_search(&item.type_id).is_ok()
                        && (item.refresh_state == MarketRefreshState::Refreshing
                            || item
                                .next_refresh_at
                                .map_or(true, |due| due <= calculated_at))
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let market_failed_count = u64::try_from(
            coverage
                .iter()
                .filter(|item| {
                    market_type_ids.binary_search(&item.type_id).is_ok()
                        && item.last_error.is_some()
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;

        let mut projected = Vec::with_capacity(facility_previews.len());
        for (recipe, classification, facility_preview) in facility_previews {
            let candidate_name = recipe.primary_product().type_name.clone();
            match project_candidate(
                workspace_id,
                owner_id,
                &normalized,
                &books,
                recipe,
                &classification,
                &adjusted_prices,
                facility_preview,
                calculated_at,
                self.freshness.market,
            ) {
                Ok(candidate) => projected.push(candidate),
                Err(error) => candidate_exclusions
                    .push(format!("{candidate_name} could not be evaluated: {error}")),
            }
        }
        sort_opportunity_candidates(&mut projected);
        let complete_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| candidate.completeness == OpportunityCompleteness::Complete)
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let candidate_count =
            u64::try_from(projected.len()).map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let incomplete_count = candidate_count
            .checked_sub(complete_count)
            .ok_or(OpportunityError::ArithmeticOverflow)?;
        let stale_economic_evidence = adjusted_price_readiness(
            material_type_ids.len(),
            adjusted_prices.len(),
            adjusted_price_observed_at,
            calculated_at,
            self.freshness.adjusted_prices,
        )
        .state
            == OpportunityEvidenceState::Stale
            || system_index.state == OpportunityEvidenceState::Stale;
        let stale_but_complete_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| {
                    candidate.completeness == OpportunityCompleteness::Complete
                        && (stale_economic_evidence
                            || candidate.warnings.iter().any(|warning| {
                                warning.kind == OpportunityWarningKind::StaleMarketEvidence
                            }))
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis())
            .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let mut adjusted_price_readiness = adjusted_price_readiness(
            material_type_ids.len(),
            adjusted_prices.len(),
            adjusted_price_observed_at,
            calculated_at,
            self.freshness.adjusted_prices,
        );
        adjusted_price_readiness.refresh_pending = adjusted_overlay.pending;
        adjusted_price_readiness.last_refresh_error = adjusted_overlay.last_error;
        let refresh_pending = market_pending_count > 0
            || adjusted_price_readiness.refresh_pending
            || system_index.refresh_pending;
        let default_ranking_eligible_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| {
                    candidate.eligibility.status
                        != OpportunityEligibilityStatus::ExcludedFromDefaultRanking
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let excluded_count = candidate_count
            .checked_sub(default_ranking_eligible_count)
            .ok_or(OpportunityError::ArithmeticOverflow)?;
        let strong_evidence_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| {
                    candidate.quality.evidence_quality == crate::OpportunityEvidenceQuality::Strong
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let qualified_evidence_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| {
                    candidate.quality.evidence_quality
                        == crate::OpportunityEvidenceQuality::Qualified
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let weak_evidence_count = u64::try_from(
            projected
                .iter()
                .filter(|candidate| {
                    candidate.quality.evidence_quality == crate::OpportunityEvidenceQuality::Weak
                })
                .count(),
        )
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
        let rankings = project_rankings(&projected);
        let excluded_costs = crate::opportunity_quality::opportunity_excluded_costs();

        Ok(OpportunityEvaluation {
            context: OpportunityEvaluationContext {
                scope_id: normalized.scope_id,
                facility_profile_id: normalized.facility_profile_id,
                // The actually-resolved current profile revision -- evidence
                // of what this evaluation calculated against, not a
                // caller-supplied expectation.
                facility_revision: facility.revision,
                market_region_id: normalized.market_scope.region_id,
                market_location_id: normalized.market_scope.location_id,
                material_efficiency: normalized.material_efficiency,
                time_efficiency: normalized.time_efficiency,
                runs: normalized.runs,
                material_pricing_policy: normalized.material_pricing_policy,
                output_pricing_policy: normalized.output_pricing_policy,
                inventory_reuse_enabled: false,
                recursive_component_expansion_enabled: false,
            },
            calculated_at,
            elapsed_milliseconds,
            candidate_count,
            complete_count,
            incomplete_count,
            default_ranking_eligible_count,
            excluded_count,
            strong_evidence_count,
            qualified_evidence_count,
            weak_evidence_count,
            required_market_type_ids: market_type_ids,
            readiness: OpportunityReadiness {
                registered_at: calculated_at,
                local_read_at,
                required_material_type_count,
                required_output_type_count,
                market_fresh_count,
                market_stale_count,
                market_missing_count,
                market_pending_count,
                market_failed_count,
                oldest_market_observed_at: market_observed_at.iter().min().copied(),
                newest_market_observed_at: market_observed_at.iter().max().copied(),
                adjusted_prices: adjusted_price_readiness,
                system_index_source,
                system_index,
                stale_but_complete_count,
                refresh_pending,
            },
            candidates: projected,
            rankings,
            excluded_costs,
            warnings: Vec::new(),
            assumptions: opportunity_assumptions(),
            exclusions: opportunity_exclusions()
                .into_iter()
                .chain(candidate_exclusions)
                .collect(),
        })
    }

    pub async fn prioritize_evidence(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: EvaluateOpportunitiesCommand,
        accepted_at: DateTime<Utc>,
    ) -> Result<OpportunityRefreshAcceptance, OpportunityError> {
        let normalized = normalize_evaluation(command.clone())?;
        let scope =
            profitability_scope(normalized.scope_id).ok_or(OpportunityError::UnknownScope)?;
        let evaluation = self
            .evaluate(workspace_id, owner_id, command, accepted_at)
            .await?;
        // Idempotent get-or-create, same as `evaluate`'s own resolution --
        // cheap to call twice per request rather than threading the
        // resolved id back out through `OpportunityEvaluation`'s public
        // response shape.
        let source_id = self
            .market_repository
            .ensure_esi_price_source_for_scope(workspace_id, normalized.market_scope)
            .await?;
        let market = disposition(
            match self
                .public_region(workspace_id, normalized.market_scope)
                .await?
            {
                Some(region_id) => {
                    self.market_repository
                        .prioritize_public_market_refresh(
                            region_id,
                            &evaluation.required_market_type_ids,
                            accepted_at,
                        )
                        .await?
                }
                None => {
                    self.market_repository
                        .prioritize_market_refresh(
                            workspace_id,
                            source_id,
                            &evaluation.required_market_type_ids,
                            accepted_at,
                        )
                        .await?
                }
            },
        );
        let adjusted_prices = disposition(
            self.adjusted_price_repository
                .prioritize_adjusted_price_refresh(accepted_at)
                .await?,
        );
        let industry = IndustryService::new(
            Arc::clone(&self.industry_repository),
            Arc::clone(&self.sde_repository),
        );
        let facility = industry
            .resolve_facility_context(
                workspace_id,
                normalized.facility_profile_id,
                facility_role_for_recipe_kind(scope.recipe_kind),
            )
            .await?;
        let system_index = if facility.manual_system_cost_index.is_some() {
            OpportunityRefreshDisposition::NotRequired
        } else if let Some(solar_system_id) = facility.solar_system_id {
            disposition(
                self.adjusted_price_repository
                    .prioritize_system_cost_index_refresh(solar_system_id, accepted_at)
                    .await?,
            )
        } else {
            OpportunityRefreshDisposition::NotRequired
        };
        Ok(OpportunityRefreshAcceptance {
            scope_id: normalized.scope_id,
            market,
            adjusted_prices,
            system_index,
            accepted_at,
        })
    }
}

impl OpportunityQueryService {
    /// The region whose app-wide public book serves `scope`, when it has one
    /// (`MarketScope::public_region`).
    async fn public_region(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<Option<i64>, crate::MarketError> {
        let classification = match scope.location_id {
            Some(location_id) => Some(
                self.market_repository
                    .classify_location(workspace_id, location_id)
                    .await?,
            ),
            None => None,
        };
        Ok(scope.public_region(classification))
    }
}

fn disposition(accepted: bool) -> OpportunityRefreshDisposition {
    if accepted {
        OpportunityRefreshDisposition::Accepted
    } else {
        OpportunityRefreshDisposition::AlreadyPending
    }
}

const fn facility_role_for_recipe_kind(kind: CandidateRecipeKind) -> FacilityRole {
    match kind {
        CandidateRecipeKind::Manufacturing => FacilityRole::Manufacturing,
        CandidateRecipeKind::Reaction => FacilityRole::Reaction,
    }
}

fn opportunity_assumptions() -> Vec<String> {
    vec![
        "One manufacturing run with explicit blueprint ME/TE.".to_string(),
        "All immediate inputs are purchased from current sell-order depth.".to_string(),
        "The primary output is valued at the current lowest sell order.".to_string(),
        "Inventory reuse and recursive component builds are disabled.".to_string(),
    ]
}

fn opportunity_exclusions() -> Vec<String> {
    vec![
        "Broker fees and output sales taxes".to_string(),
        "Hauling and logistics".to_string(),
        "Blueprint acquisition, invention, and copying".to_string(),
        "Liquidity, sale time, and slot concurrency".to_string(),
    ]
}

#[cfg(test)]
mod tests;
