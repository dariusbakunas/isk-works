//! Per-candidate projection: adapt an SDE candidate into a `BuildRecipe`
//! against the scope's published-catalog predicate (`capture_candidate_recipe`),
//! then run the full projection (`project_candidate`) -- transient plan,
//! warnings, sell/liquidation depth, valuations, metrics, eligibility and
//! evidence quality -- into an `OpportunityCandidate`.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use iskworks_sde::{
    CandidateRecipeIdentity, CandidateRecipeKind, ManufacturableCandidateRecipe,
    ManufacturableCandidateScope, ManufacturingRecipe,
};

use crate::{
    BlueprintKind, Build, BuildId, BuildRecipe, CapturedReactionFormula, CapturedRecipe,
    FacilityPlanPreview, FacilityProfileId, MarketOrderBook, MarketPricingPolicy,
    OpportunityEligibility, OpportunityEligibilityStatus, OpportunityQuality, OwnerId,
    RecipeCurrency, WorkspaceId,
};

use super::command::{
    material_price_request, output_price_request, NormalizedOpportunityEvaluation, OpportunityError,
};
use super::evidence::{
    OpportunityCompleteness, OpportunityEivBasis, OpportunityEvidenceState, OpportunityWarning,
    OpportunityWarningDetails, OpportunityWarningKind,
};
use super::metrics::{derive_opportunity_metrics, OpportunityMetrics};
use super::scope_catalog::ProfitabilityScopeId;
#[cfg(test)]
use super::valuation::OpportunityValuation;
use super::valuation::{
    project_eiv_basis, project_output_market_evidence, project_output_valuation,
    OpportunityOutputMarketEvidence, OpportunityValuations,
};

/// Freshness thresholds for `resolve_market_price_items`'s note-formatting
/// only (staleness for `OpportunityWarningKind::StaleMarketEvidence` is
/// computed separately, straight off each book's own `observed_at` against
/// `self.freshness.market` -- see `project_candidate`), matching
/// `derive_market_price_items`'s own constants (Build/Order pricing) for
/// consistency across every scope-based pricing call site.
const MARKET_PRICE_FRESH_AFTER_HOURS: u32 = 1;
const MARKET_PRICE_STALE_AFTER_HOURS: u32 = 24;

/// A candidate's recipe identity: either the manufacturing blueprint or the
/// reaction formula that produces it. Mirrors `BuildRecipe`/`RecipeSelection`
/// exactly (same tag/field shape) so the frontend can pass this straight
/// through to `createBuild()` without reconstructing it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpportunityRecipeIdentity {
    Manufacturing {
        blueprint_type_id: i64,
        blueprint_name: String,
    },
    Reaction {
        reaction_formula_type_id: i64,
        reaction_formula_name: String,
    },
}

impl OpportunityRecipeIdentity {
    #[must_use]
    pub const fn type_id(&self) -> i64 {
        match self {
            Self::Manufacturing {
                blueprint_type_id, ..
            } => *blueprint_type_id,
            Self::Reaction {
                reaction_formula_type_id,
                ..
            } => *reaction_formula_type_id,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityCandidate {
    pub scope_id: ProfitabilityScopeId,
    pub product_type_id: i64,
    pub product_name: String,
    pub recipe: OpportunityRecipeIdentity,
    pub sde_version: String,
    pub recipe_fingerprint: String,
    pub runs: u64,
    pub output_quantity: u64,
    pub base_duration_seconds: Option<u64>,
    pub effective_duration_seconds: Option<u64>,
    /// `None` for a reaction candidate -- reaction formulas have no
    /// material/time efficiency concept in EVE.
    pub material_efficiency: Option<u8>,
    pub time_efficiency: Option<u8>,
    pub facility_profile_id: FacilityProfileId,
    pub facility_revision: u64,
    pub metrics: OpportunityMetrics,
    pub completeness: OpportunityCompleteness,
    pub warnings: Vec<OpportunityWarning>,
    pub missing_price_type_ids: Vec<i64>,
    pub valuations: OpportunityValuations,
    pub output_market_evidence: Option<OpportunityOutputMarketEvidence>,
    pub eiv_basis: OpportunityEivBasis,
    pub eligibility: OpportunityEligibility,
    pub quality: OpportunityQuality,
}

#[cfg(test)]
impl OpportunityCandidate {
    pub(super) fn ranking_fixture(
        product_type_id: i64,
        product_name: &str,
        complete: bool,
        profit_per_hour: Option<&str>,
    ) -> Self {
        Self {
            scope_id: ProfitabilityScopeId::T1Frigates,
            product_type_id,
            product_name: product_name.to_string(),
            recipe: OpportunityRecipeIdentity::Manufacturing {
                blueprint_type_id: product_type_id + 1_000,
                blueprint_name: format!("{product_name} Blueprint"),
            },
            sde_version: "test".to_string(),
            recipe_fingerprint: product_type_id.to_string(),
            runs: 1,
            output_quantity: 1,
            base_duration_seconds: Some(3600),
            effective_duration_seconds: Some(3600),
            material_efficiency: Some(10),
            time_efficiency: Some(20),
            facility_profile_id: FacilityProfileId(uuid::Uuid::nil()),
            facility_revision: 1,
            metrics: OpportunityMetrics {
                material_cost: None,
                installation_cost: None,
                total_estimated_manufacturing_cost: None,
                estimated_output_value: None,
                estimated_gross_profit: None,
                gross_margin_percent: None,
                estimated_gross_profit_per_unit: None,
                estimated_gross_profit_per_run: None,
                estimated_gross_profit_per_manufacturing_hour: profit_per_hour.map(str::to_string),
                capital_required: None,
            },
            completeness: if complete {
                OpportunityCompleteness::Complete
            } else {
                OpportunityCompleteness::Incomplete
            },
            warnings: Vec::new(),
            missing_price_type_ids: Vec::new(),
            valuations: OpportunityValuations {
                sell_side: OpportunityValuation {
                    revenue: None,
                    gross_profit: None,
                    gross_margin_percent: None,
                    gross_profit_per_manufacturing_hour: profit_per_hour.map(str::to_string),
                    completeness: if complete {
                        OpportunityCompleteness::Complete
                    } else {
                        OpportunityCompleteness::Incomplete
                    },
                },
                immediate_liquidation: OpportunityValuation {
                    revenue: None,
                    gross_profit: None,
                    gross_margin_percent: None,
                    gross_profit_per_manufacturing_hour: None,
                    completeness: OpportunityCompleteness::Incomplete,
                },
            },
            output_market_evidence: None,
            eiv_basis: OpportunityEivBasis {
                complete: true,
                required_material_count: 0,
                observed_material_count: 0,
                missing_materials: Vec::new(),
            },
            eligibility: OpportunityEligibility {
                status: OpportunityEligibilityStatus::Eligible,
                exclusion_reasons: Vec::new(),
            },
            quality: OpportunityQuality {
                evidence_quality: crate::OpportunityEvidenceQuality::Strong,
            },
        }
    }
}

#[must_use]
pub fn opportunity_market_demand(
    recipes: &[BuildRecipe],
) -> Vec<crate::MarketCoverageRegistration> {
    let mut required = BTreeMap::new();
    for recipe in recipes {
        for line in recipe.materials().iter().chain(recipe.products().iter()) {
            required
                .entry(line.type_id)
                .or_insert_with(|| line.type_name.clone());
        }
    }
    required
        .into_iter()
        .map(|(type_id, type_name)| crate::MarketCoverageRegistration { type_id, type_name })
        .collect()
}

/// True when an optional classification dimension satisfies the scope's
/// filter for it. An empty `allowed` set means the scope places no
/// restriction on this dimension at all -- matching the SQL query's own
/// `cardinality($n) = 0 OR ... = ANY($n)` semantics exactly, including when
/// the candidate's own value is absent (e.g. Attack Battlecruisers and
/// reaction-formula products, which the real SDE leaves without a
/// `meta_group_id` at all). A *non-empty* `allowed` set still requires the
/// candidate to carry a matching value -- unchanged from before.
fn satisfies_classification_filter(value: Option<i64>, allowed: &BTreeSet<i64>) -> bool {
    allowed.is_empty() || value.is_some_and(|id| allowed.contains(&id))
}

pub(super) fn capture_candidate_recipe(
    candidate: ManufacturableCandidateRecipe,
    scope: &ManufacturableCandidateScope,
) -> Result<BuildRecipe, OpportunityError> {
    let candidate_kind = match candidate.identity {
        CandidateRecipeIdentity::Manufacturing { .. } => CandidateRecipeKind::Manufacturing,
        CandidateRecipeIdentity::Reaction { .. } => CandidateRecipeKind::Reaction,
    };
    if candidate.primary_product_type_id <= 0
        || !candidate.primary_product_published
        || !candidate.recipe_type_published
        || !scope.recipe_kinds.contains(&candidate_kind)
        || !satisfies_classification_filter(
            candidate.classification.category_id,
            &scope.category_ids,
        )
        || !satisfies_classification_filter(candidate.classification.group_id, &scope.group_ids)
        || !satisfies_classification_filter(
            candidate.classification.meta_group_id,
            &scope.meta_group_ids,
        )
        || (!scope.market_group_root_ids.is_empty()
            && !candidate
                .classification
                .market_group_ancestry
                .iter()
                .any(|group| scope.market_group_root_ids.contains(&group.market_group_id)))
    {
        return Err(OpportunityError::InvalidCandidate(
            "candidate does not satisfy the published catalog for this scope".to_string(),
        ));
    }
    match candidate.identity {
        CandidateRecipeIdentity::Manufacturing { blueprint_type_id } => CapturedRecipe::capture(
            candidate.import_id,
            candidate.source_version,
            ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: candidate.recipe_name,
                duration_seconds: candidate.duration_seconds,
                materials: candidate.materials,
                products: candidate.products,
            },
        )
        .map(BuildRecipe::Manufacturing)
        .map_err(OpportunityError::from),
        CandidateRecipeIdentity::Reaction {
            reaction_formula_type_id,
        } => CapturedReactionFormula::capture(
            candidate.import_id,
            candidate.source_version,
            iskworks_sde::ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: candidate.recipe_name,
                duration_seconds: candidate.duration_seconds,
                materials: candidate.materials,
                products: candidate.products,
            },
        )
        .map(BuildRecipe::Reaction)
        .map_err(OpportunityError::from),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_candidate(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    normalized: &NormalizedOpportunityEvaluation,
    books: &BTreeMap<i64, MarketOrderBook>,
    recipe: BuildRecipe,
    classification: &iskworks_sde::CandidateProductClassification,
    adjusted_prices: &BTreeMap<i64, Decimal>,
    facility_preview: FacilityPlanPreview,
    calculated_at: DateTime<Utc>,
    market_freshness: chrono::Duration,
) -> Result<OpportunityCandidate, OpportunityError> {
    let product = recipe.primary_product().clone();
    let output_quantity = product
        .quantity_per_run
        .checked_mul(normalized.runs)
        .ok_or(OpportunityError::ArithmeticOverflow)?;
    let requests = facility_preview
        .requirements
        .iter()
        .map(|line| {
            material_price_request(
                line.type_id,
                line.type_name.clone(),
                line.final_required_quantity,
            )
        })
        .chain(std::iter::once(output_price_request(
            product.type_id,
            product.type_name.clone(),
            output_quantity,
        )))
        .collect::<Vec<_>>();
    let resolved = crate::resolve_market_price_items(
        &requests,
        |type_id| books.get(&type_id).map(|book| book.orders.as_slice()),
        calculated_at,
        MARKET_PRICE_FRESH_AFTER_HOURS,
        MARKET_PRICE_STALE_AFTER_HOURS,
        true,
    )?;
    let pricing_policies = requests
        .iter()
        .map(|request| (request.type_id, request.pricing_policy))
        .collect::<BTreeMap<_, _>>();
    let build_id = BuildId::new();
    let build = Build {
        id: build_id,
        workspace_id,
        owner_id,
        name: product.type_name.clone(),
        recipe: recipe.clone(),
        runs: normalized.runs,
        notes: String::new(),
        revision: 0,
        created_at: calculated_at,
        updated_at: calculated_at,
        draft_planning: None,
        recipe_currency: RecipeCurrency::Current,
        active_sde_version: Some(recipe.source_sde_version().to_string()),
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: false,
    };
    // Reaction formulas have no blueprint at all in EVE -- nothing to
    // snapshot, matching how a real reaction Build's own preview flow
    // behaves (`capture_blueprint_snapshot_for_preview` returns `Ok(None)`).
    let blueprint: Option<crate::ResolvedBlueprintAssumptions> = match &recipe {
        BuildRecipe::Manufacturing(manufacturing) => {
            let mut blueprint: crate::ResolvedBlueprintAssumptions =
                crate::capture_manual_snapshot(
                    build_id,
                    manufacturing.blueprint_type_id,
                    &manufacturing.blueprint_name,
                    normalized.runs,
                    BlueprintKind::Original,
                    // `normalize_evaluation` guarantees Some for a manufacturing
                    // scope; the fallback is unreachable, not a real default.
                    normalized.material_efficiency.unwrap_or(0),
                    normalized.time_efficiency.unwrap_or(0),
                    None,
                    "",
                    calculated_at,
                )?
                .into();
            blueprint.planned_duration_seconds = facility_preview.planned_duration_seconds;
            Some(blueprint)
        }
        BuildRecipe::Reaction(_) => None,
    };
    let empty_profiles = BTreeMap::new();
    let empty_money = BTreeMap::new();
    let empty_coverage = BTreeMap::new();
    // Opportunities scanning doesn't yet distinguish material/output scope
    // -- it resolves both against the one scanning scope the caller picked
    // (converging this onto Build's own material/output split is a
    // separate, deferred product decision).
    // `manual_price_list: None` -- there is no manual-price-list fallback
    // in scanning, only scope-based pricing.
    let plan =
        crate::industry::calculate_transient_plan(crate::industry::TransientCalculationInput {
            build: &build,
            material_scope: normalized.market_scope,
            output_scope: normalized.market_scope,
            market_items: &resolved.items,
            manual_price_list: None,
            overrides: Vec::new(),
            pricing_policies: &pricing_policies,
            effective_requirements: &facility_preview.requirements,
            root_facility: Some(facility_preview.clone()),
            blueprint,
            expansion: None,
            manufacturing_profile: None,
            reaction_profile: None,
            component_facility_overrides: &empty_profiles,
            component_eivs: &empty_money,
            linked_build_material_costs: &empty_money,
            material_coverage: &empty_coverage,
        })?;

    let mut warnings = Vec::new();
    let mut missing_price_type_ids = Vec::new();
    for request in &requests {
        match resolved.depth.get(&request.type_id) {
            None => {
                missing_price_type_ids.push(request.type_id);
                warnings.push(OpportunityWarning {
                    kind: if request.type_id == product.type_id {
                        OpportunityWarningKind::MissingOutputPrice
                    } else {
                        OpportunityWarningKind::MissingMaterialPrice
                    },
                    message: format!("No compatible market order book for {}.", request.type_name),
                    type_ids: vec![request.type_id],
                    details: None,
                });
            }
            Some(depth) if !depth.fully_covered => {
                missing_price_type_ids.push(request.type_id);
                warnings.push(OpportunityWarning {
                    kind: OpportunityWarningKind::InsufficientMarketDepth,
                    message: format!(
                        "Market depth covers {} of {} units of {}.",
                        depth.covered_quantity, depth.requested_quantity, request.type_name
                    ),
                    type_ids: vec![request.type_id],
                    details: None,
                });
            }
            Some(_) => {
                if let Some(book) = books.get(&request.type_id) {
                    // Age gate uses effective freshness (a `304` re-confirms
                    // the snapshot); `details.observed_at` stays the
                    // physical fetch time as an operator breadcrumb.
                    let age = calculated_at.signed_duration_since(book.effective_observed_at());
                    if age > market_freshness {
                        let age_seconds = u64::try_from(age.num_seconds().max(0)).unwrap_or(0);
                        let freshness_target_seconds =
                            u64::try_from(market_freshness.num_seconds().max(0)).unwrap_or(0);
                        warnings.push(OpportunityWarning {
                            kind: OpportunityWarningKind::StaleMarketEvidence,
                            message: format!(
                                "Market observations for {} are stale.",
                                request.type_name
                            ),
                            type_ids: vec![request.type_id],
                            details: Some(OpportunityWarningDetails::StaleMarketEvidence {
                                observed_at: book.observed_at,
                                age_seconds,
                                freshness_target_seconds,
                                refresh_state: OpportunityEvidenceState::Stale,
                            }),
                        });
                    }
                }
            }
        }
    }
    if recipe.products().len() != 1 {
        warnings.push(OpportunityWarning {
            kind: OpportunityWarningKind::MultipleOutputsUnsupported,
            message: "Recipes with multiple outputs are not comparable in Phase 1.".to_string(),
            type_ids: recipe.products().iter().map(|line| line.type_id).collect(),
            details: None,
        });
    }
    let installation_cost = facility_preview
        .installation_cost
        .complete
        .then_some(facility_preview.installation_cost.total)
        .flatten();
    if installation_cost.is_none() {
        warnings.push(OpportunityWarning {
            kind: OpportunityWarningKind::IncompleteInstallationCost,
            message: "Installation cost is incomplete because adjusted-price or facility evidence is missing."
                .to_string(),
            type_ids: facility_preview
                .requirements
                .iter()
                .map(|line| line.type_id)
                .collect(),
            details: None,
        });
    }
    missing_price_type_ids.sort_unstable();
    missing_price_type_ids.dedup();
    let material_cost = plan
        .pricing_complete
        .then_some(plan.estimated_material_cost);

    let eiv_basis = project_eiv_basis(recipe.materials(), adjusted_prices);
    if !eiv_basis.complete {
        warnings.push(OpportunityWarning {
            kind: OpportunityWarningKind::IncompleteEivBasis,
            message: format!(
                "{} of {} required materials are missing an adjusted-price observation.",
                eiv_basis.missing_materials.len(),
                eiv_basis.required_material_count
            ),
            type_ids: eiv_basis
                .missing_materials
                .iter()
                .map(|material| material.type_id)
                .collect(),
            details: Some(OpportunityWarningDetails::IncompleteEivBasis {
                missing_materials: eiv_basis.missing_materials.clone(),
            }),
        });
    }

    let output_book = books.get(&product.type_id);
    let sell_depth = output_book
        .map(|book| {
            crate::calculate_market_depth(
                &book.orders,
                MarketPricingPolicy::AcquireQuantityFromSellOrders,
                output_quantity,
            )
        })
        .transpose()?;
    // The `requests`/`resolved` loop above only asks the output book
    // for a single unit (MarketPricingPolicy::LowestSell), so it can call a
    // book "fully covered" even when it cannot actually supply
    // `output_quantity` units -- exactly the case `sell_depth` (quantity-aware)
    // exists to catch. Reconcile them here so a candidate whose sell-side
    // valuation is incomplete for depth reasons always carries the warning
    // that explains why, instead of silently disagreeing with `completeness`.
    if let Some(depth) = &sell_depth {
        if !depth.fully_covered && !missing_price_type_ids.contains(&product.type_id) {
            missing_price_type_ids.push(product.type_id);
            missing_price_type_ids.sort_unstable();
            missing_price_type_ids.dedup();
            warnings.push(OpportunityWarning {
                kind: OpportunityWarningKind::InsufficientMarketDepth,
                message: format!(
                    "Market depth covers {} of {} units of {}.",
                    depth.covered_quantity, depth.requested_quantity, product.type_name
                ),
                type_ids: vec![product.type_id],
                details: None,
            });
        }
    }
    let liquidation_depth = output_book
        .map(|book| {
            crate::calculate_market_depth(
                &book.orders,
                MarketPricingPolicy::LiquidateQuantityIntoBuyOrders,
                output_quantity,
            )
        })
        .transpose()?;
    let sell_side = project_output_valuation(
        material_cost,
        installation_cost,
        sell_depth.as_ref(),
        output_quantity,
        facility_preview.planned_duration_seconds,
        recipe.products().len(),
    )?;
    let immediate_liquidation = project_output_valuation(
        material_cost,
        installation_cost,
        liquidation_depth.as_ref(),
        output_quantity,
        facility_preview.planned_duration_seconds,
        recipe.products().len(),
    )?;
    let output_market_evidence = output_book.map(|book| {
        project_output_market_evidence(book, output_quantity, calculated_at, market_freshness)
    });
    if let Some(evidence) = &output_market_evidence {
        let reasons = crate::opportunity_quality::is_thin_output_book(
            output_quantity,
            evidence.best_sell_level_quantity,
            evidence.total_visible_sell_quantity,
        );
        if !reasons.is_empty() {
            warnings.push(OpportunityWarning {
                kind: OpportunityWarningKind::ThinOutputBook,
                message: "Visible sell-order depth for the output is thin relative to the candidate's output quantity."
                    .to_string(),
                type_ids: vec![product.type_id],
                details: Some(OpportunityWarningDetails::ThinBook { reasons }),
            });
        }
    }

    let completeness = sell_side.completeness;
    let metrics = derive_opportunity_metrics(
        material_cost,
        installation_cost,
        sell_side.revenue,
        output_quantity,
        facility_preview.planned_duration_seconds,
    )?;

    let eligibility = {
        let exclusion_reasons =
            crate::opportunity_quality::classify_recipe_eligibility(classification);
        let status = if !exclusion_reasons.is_empty() {
            OpportunityEligibilityStatus::ExcludedFromDefaultRanking
        } else if !warnings.is_empty() {
            OpportunityEligibilityStatus::EligibleWithWarnings
        } else {
            OpportunityEligibilityStatus::Eligible
        };
        OpportunityEligibility {
            status,
            exclusion_reasons,
        }
    };
    let quality = OpportunityQuality {
        evidence_quality: crate::opportunity_quality::derive_evidence_quality(&warnings),
    };
    let recipe_identity = match &recipe {
        BuildRecipe::Manufacturing(manufacturing) => OpportunityRecipeIdentity::Manufacturing {
            blueprint_type_id: manufacturing.blueprint_type_id,
            blueprint_name: manufacturing.blueprint_name.clone(),
        },
        BuildRecipe::Reaction(formula) => OpportunityRecipeIdentity::Reaction {
            reaction_formula_type_id: formula.reaction_formula_type_id,
            reaction_formula_name: formula.reaction_formula_name.clone(),
        },
    };

    Ok(OpportunityCandidate {
        scope_id: normalized.scope_id,
        product_type_id: product.type_id,
        product_name: product.type_name,
        recipe: recipe_identity,
        sde_version: recipe.source_sde_version().to_string(),
        recipe_fingerprint: recipe.fingerprint().to_string(),
        runs: normalized.runs,
        output_quantity,
        base_duration_seconds: recipe.duration_seconds_per_run(),
        effective_duration_seconds: facility_preview.planned_duration_seconds,
        material_efficiency: normalized.material_efficiency,
        time_efficiency: normalized.time_efficiency,
        facility_profile_id: normalized.facility_profile_id,
        // The actually-resolved current profile revision this candidate was
        // calculated against.
        facility_revision: facility_preview.profile.revision,
        metrics,
        completeness,
        warnings,
        missing_price_type_ids,
        valuations: OpportunityValuations {
            sell_side,
            immediate_liquidation,
        },
        output_market_evidence,
        eiv_basis,
        eligibility,
        quality,
    })
}

#[cfg(test)]
mod tests {
    use iskworks_sde::CandidateRecipeIdentity;
    use uuid::Uuid;

    use super::super::tests_common::*;
    use super::*;
    use crate::opportunity::*;
    use crate::BuildRecipe;

    #[test]
    fn candidate_adapter_preserves_authoritative_recipe_identity_and_rejects_reactions() {
        let scope = profitability_scope(ProfitabilityScopeId::T1Frigates).unwrap();
        let captured = capture_candidate_recipe(
            candidate_recipe(CandidateRecipeIdentity::Manufacturing {
                blueprint_type_id: 68_357,
            }),
            &scope.candidate_scope,
        )
        .unwrap();

        let BuildRecipe::Manufacturing(captured) = captured else {
            panic!("expected a manufacturing recipe");
        };
        assert_eq!(captured.source_sde_dataset_id, Uuid::from_u128(9));
        assert_eq!(captured.source_sde_version, "2026.08");
        assert_eq!(captured.blueprint_type_id, 68_357);
        assert_eq!(captured.materials[0].sort_order, 0);
        assert_eq!(captured.primary_product().type_id, 5_876);
        assert!(!captured.fingerprint.is_empty());

        // A reaction candidate returned for a manufacturing-only scope is
        // rejected by the generic `recipe_kinds` filter.
        assert!(matches!(
            capture_candidate_recipe(
                candidate_recipe(CandidateRecipeIdentity::Reaction {
                    reaction_formula_type_id: 123,
                }),
                &scope.candidate_scope,
            ),
            Err(OpportunityError::InvalidCandidate(_))
        ));

        let mut mismatched = candidate_recipe(CandidateRecipeIdentity::Manufacturing {
            blueprint_type_id: 68_357,
        });
        mismatched.classification.meta_group_id = Some(2);
        assert!(matches!(
            capture_candidate_recipe(mismatched, &scope.candidate_scope),
            Err(OpportunityError::InvalidCandidate(_))
        ));
    }

    #[test]
    fn candidate_adapter_captures_reaction_formulas_under_the_reactions_scope() {
        let scope = profitability_scope(ProfitabilityScopeId::Reactions).unwrap();
        let captured = capture_candidate_recipe(
            reaction_candidate_recipe(CandidateRecipeIdentity::Reaction {
                reaction_formula_type_id: 46_171,
            }),
            &scope.candidate_scope,
        )
        .unwrap();

        let BuildRecipe::Reaction(formula) = captured else {
            panic!("expected a reaction formula");
        };
        assert_eq!(formula.reaction_formula_type_id, 46_171);
        assert_eq!(formula.materials[0].type_id, 16_633);
        assert_eq!(formula.primary_product().type_id, 16_656);

        // A manufacturing candidate returned for a reaction-only scope is
        // rejected by the same `recipe_kinds` filter, symmetrically.
        assert!(matches!(
            capture_candidate_recipe(
                candidate_recipe(CandidateRecipeIdentity::Manufacturing {
                    blueprint_type_id: 68_357,
                }),
                &scope.candidate_scope,
            ),
            Err(OpportunityError::InvalidCandidate(_))
        ));
    }

    #[test]
    fn recipe_identity_serializes_every_field_camel_case_not_just_the_tag() {
        // `#[serde(tag = "...", rename_all = "camelCase")]` alone only
        // camel-cases the tag's own value ("manufacturing"/"reaction") --
        // NOT the struct-variant fields nested inside each arm, which need
        // the separate `rename_all_fields` attribute. A live end-to-end
        // check caught this: the wire format silently regressed to
        // `reaction_formula_type_id` (snake_case), and createBuild() on a
        // reaction candidate failed with "missing field
        // `reactionFormulaTypeId`". This test pins the wire shape so that
        // regression can't recur silently.
        let manufacturing = OpportunityRecipeIdentity::Manufacturing {
            blueprint_type_id: 68_357,
            blueprint_name: "Rifter Blueprint".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&manufacturing).unwrap(),
            serde_json::json!({
                "kind": "manufacturing",
                "blueprintTypeId": 68_357,
                "blueprintName": "Rifter Blueprint",
            })
        );

        let reaction = OpportunityRecipeIdentity::Reaction {
            reaction_formula_type_id: 46_171,
            reaction_formula_name: "Fernite Alloy Reaction Formula".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&reaction).unwrap(),
            serde_json::json!({
                "kind": "reaction",
                "reactionFormulaTypeId": 46_171,
                "reactionFormulaName": "Fernite Alloy Reaction Formula",
            })
        );
    }

    #[test]
    fn scope_market_demand_unions_products_and_materials_by_type_id() {
        let scope = profitability_scope(ProfitabilityScopeId::T1Frigates).unwrap();
        let first = capture_candidate_recipe(
            candidate_recipe(CandidateRecipeIdentity::Manufacturing {
                blueprint_type_id: 68_357,
            }),
            &scope.candidate_scope,
        )
        .unwrap();
        let mut second_candidate = candidate_recipe(CandidateRecipeIdentity::Manufacturing {
            blueprint_type_id: 68_358,
        });
        second_candidate.materials.push(iskworks_sde::RecipeLine {
            type_id: 35,
            type_name: "Pyerite".to_string(),
            quantity: 100,
        });
        second_candidate.products[0].type_id = 5_877;
        second_candidate.products[0].type_name = "Punisher".to_string();
        second_candidate.primary_product_type_id = 5_877;
        let second = capture_candidate_recipe(second_candidate, &scope.candidate_scope).unwrap();

        let demand = opportunity_market_demand(&[first, second]);

        assert_eq!(
            demand,
            vec![
                crate::MarketCoverageRegistration {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                },
                crate::MarketCoverageRegistration {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                },
                crate::MarketCoverageRegistration {
                    type_id: 5_876,
                    type_name: "Rifter".to_string(),
                },
                crate::MarketCoverageRegistration {
                    type_id: 5_877,
                    type_name: "Punisher".to_string(),
                },
            ]
        );
    }
}
