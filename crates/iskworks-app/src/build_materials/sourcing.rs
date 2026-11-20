//! Optimize build sourcing -- the application orchestration behind
//! `POST /api/builds/:id/optimize-sourcing/{preview,apply}`.
//!
//! Preview projects and costs three versions of the root plan, all from one
//! inventory observation: the current plan, the all-produce candidate
//! (`IndustryService::sourcing_candidate`), and the plan under the decisions
//! (`decide_sourcing` + `chosen_records`), so the reported margins are
//! exact. Apply is `IndustryService::apply_sourcing_changes`.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_core::build_cost::{project_build_cost, BuildCostProjection};
use iskworks_core::build_materials::{InventoryBasisEntry, PlanningInventory};
use iskworks_core::industry::sourcing_optimizer::{
    build_proposal, chosen_records, decide_sourcing, SourcingApplyChange, SourcingApplyOutcome,
    SourcingOptions, SourcingProposal,
};
use iskworks_core::industry::BuildMaterialsProjectionOutcome;
use iskworks_core::production_dependency::RootPlanRecords;
use iskworks_core::{BuildId, OwnerId, WorkspaceId};

use super::{BuildMaterialsCoordinator, BuildMaterialsError};
use crate::BuildPreviewError;

/// One inventory observation shared by every projection of a request.
struct InventorySnapshot {
    stock: Vec<(i64, u64, u64)>,
    basis: Vec<InventoryBasisEntry>,
}

impl InventorySnapshot {
    fn pool(&self) -> PlanningInventory {
        PlanningInventory::seed_free(self.stock.iter().copied())
    }
}

/// Why a projection produced no cost.
enum PlanCost {
    Costed(Box<BuildCostProjection>),
    Unavailable(String),
}

impl BuildMaterialsCoordinator {
    /// Preview the sourcing optimization of root plan `root`.
    pub async fn optimize_sourcing_preview(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        root: BuildId,
        options: &SourcingOptions,
    ) -> Result<SourcingProposal, BuildMaterialsError> {
        let industry = self.preview.industry_service();
        let candidate = industry
            .sourcing_candidate(workspace_id, root, options)
            .await?;
        if candidate.current.root.owner_id != owner_id {
            return Err(iskworks_core::IndustryError::BuildNotFound.into());
        }
        let inventory = self.inventory_snapshot(workspace_id, owner_id).await?;
        let mut adjusted = AdjustedPriceCache::default();

        let current = match self
            .cost_plan(
                workspace_id,
                owner_id,
                &candidate.current,
                &inventory,
                &mut adjusted,
                "current",
            )
            .await?
        {
            PlanCost::Costed(cost) => cost,
            PlanCost::Unavailable(reason) => {
                return Err(BuildMaterialsError::Industry(
                    iskworks_core::IndustryError::Validation(format!(
                        "The plan cannot be costed: {reason}"
                    )),
                ));
            }
        };
        let current_cost = current.root.planning_total_production_cost;
        let revenue = self
            .expected_revenue(workspace_id, owner_id, &candidate.current)
            .await?;

        let candidate_cost = match self
            .cost_plan(
                workspace_id,
                owner_id,
                &candidate.records,
                &inventory,
                &mut adjusted,
                "candidate",
            )
            .await?
        {
            PlanCost::Costed(cost) => cost,
            PlanCost::Unavailable(reason) => {
                let decided = Default::default();
                let mut proposal = build_proposal(
                    &candidate,
                    &decided,
                    &candidate.current,
                    current_cost,
                    current_cost,
                    revenue,
                );
                proposal.notice = Some(format!(
                    "Producing every component could not be costed ({reason}), so nothing is proposed."
                ));
                return Ok(proposal);
            }
        };
        let decided = decide_sourcing(&candidate, &candidate_cost, options);
        let chosen = chosen_records(&candidate, &decided.decisions);
        let proposed_cost = if same_sourcing(&chosen, &candidate.current) {
            current_cost
        } else {
            match self
                .cost_plan(
                    workspace_id,
                    owner_id,
                    &chosen,
                    &inventory,
                    &mut adjusted,
                    "chosen",
                )
                .await?
            {
                PlanCost::Costed(cost) => cost.root.planning_total_production_cost,
                PlanCost::Unavailable(_) => None,
            }
        };
        Ok(build_proposal(
            &candidate,
            &decided,
            &chosen,
            current_cost,
            proposed_cost,
            revenue,
        ))
    }

    /// Apply chosen sourcing to root plan `root` (see
    /// `IndustryService::apply_sourcing_changes`).
    pub async fn optimize_sourcing_apply(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        root: BuildId,
        fingerprint: &str,
        changes: &[SourcingApplyChange],
    ) -> Result<SourcingApplyOutcome, BuildMaterialsError> {
        Ok(self
            .preview
            .industry_service()
            .apply_sourcing_changes(workspace_id, owner_id, root, fingerprint, changes)
            .await?)
    }

    async fn inventory_snapshot(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<InventorySnapshot, BuildMaterialsError> {
        let inventory = self.preview.inventory_repository()?;
        let balances = inventory
            .list_balances(workspace_id, owner_id)
            .await
            .map_err(BuildPreviewError::from)?;
        let reserved = inventory
            .active_reservations(workspace_id, owner_id)
            .await
            .map_err(BuildPreviewError::from)?;
        let reserved_of = |type_id: i64| reserved.get(&type_id).copied().unwrap_or(0);
        Ok(InventorySnapshot {
            stock: balances
                .iter()
                .map(|balance| {
                    (
                        balance.key.type_id,
                        balance.quantity,
                        reserved_of(balance.key.type_id),
                    )
                })
                .collect(),
            basis: balances
                .iter()
                .map(|balance| InventoryBasisEntry {
                    type_id: balance.key.type_id,
                    quantity: balance.quantity,
                    reserved_quantity: reserved_of(balance.key.type_id),
                    unit_basis: balance.average_unit_cost.map(|money| money.0),
                    total_basis: balance.total_historical_cost.0,
                })
                .collect(),
        })
    }

    /// Project and cost one version of the plan from a fresh pool.
    async fn cost_plan(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        records: &RootPlanRecords,
        inventory: &InventorySnapshot,
        adjusted: &mut AdjustedPriceCache,
        label: &'static str,
    ) -> Result<PlanCost, BuildMaterialsError> {
        let started = std::time::Instant::now();
        let mut pool = inventory.pool();
        let projection = self
            .preview
            .industry_service()
            .project_plan_records(workspace_id, owner_id, records, &mut pool)
            .await?;
        let metrics = projection.metrics;
        let aggregate = match projection.outcome {
            BuildMaterialsProjectionOutcome::Complete(aggregate) => aggregate,
            BuildMaterialsProjectionOutcome::Incomplete { missing_nodes } => {
                return Ok(PlanCost::Unavailable(format!(
                    "{} producer(s) could not be previewed",
                    missing_nodes.len()
                )));
            }
            BuildMaterialsProjectionOutcome::MixedOwnerTree { .. } => {
                return Ok(PlanCost::Unavailable("mixed owners".to_string()));
            }
            BuildMaterialsProjectionOutcome::CanonicalGraphInvalid { error } => {
                return Ok(PlanCost::Unavailable(error.to_string()));
            }
        };
        let (prices, observed_at) = adjusted
            .resolve(self, &aggregate.verification_inputs)
            .await?;
        let cost = project_build_cost(
            &aggregate.verification_operations,
            &aggregate.verification_inputs,
            &inventory.basis,
            &prices,
            observed_at,
        );
        tracing::info!(
            target: "iskworks_app::planner",
            build_id = %records.root.id.0,
            plan = label,
            producers = records.producers.len(),
            operations = aggregate.verification_operations.len(),
            exact_previews = metrics.exact_previews,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "sourcing optimizer projection"
        );
        Ok(PlanCost::Costed(Box::new(cost)))
    }

    /// The root's expected revenue: its output is the same in every version
    /// of the plan, so one root preview prices it.
    async fn expected_revenue(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        records: &RootPlanRecords,
    ) -> Result<Option<iskworks_core::Money>, BuildMaterialsError> {
        let industry = self.preview.industry_service();
        let Some(command) = industry
            .reconstruct_preview_command(workspace_id, &records.root)
            .await?
        else {
            return Ok(None);
        };
        let revision = industry
            .preview_plan(
                workspace_id,
                owner_id,
                command,
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
            )
            .await?;
        Ok(revision.expected_revenue)
    }
}

/// Adjusted prices resolved once per type across the request's projections.
#[derive(Default)]
struct AdjustedPriceCache {
    prices: BTreeMap<i64, rust_decimal::Decimal>,
    observed_at: Option<DateTime<Utc>>,
}

impl AdjustedPriceCache {
    async fn resolve(
        &mut self,
        coordinator: &BuildMaterialsCoordinator,
        boundaries: &[iskworks_core::build_materials::VerificationBoundaryInput],
    ) -> Result<(BTreeMap<i64, rust_decimal::Decimal>, Option<DateTime<Utc>>), BuildMaterialsError>
    {
        let missing: Vec<iskworks_core::build_materials::VerificationBoundaryInput> = boundaries
            .iter()
            .filter(|boundary| {
                boundary.base_quantity_per_run > 0
                    && boundary.type_id > 0
                    && !self.prices.contains_key(&boundary.type_id)
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            let (prices, observed_at) = coordinator.resolve_adjusted_prices(&missing).await?;
            self.prices.extend(prices);
            self.observed_at = self.observed_at.or(observed_at);
        }
        Ok((self.prices.clone(), self.observed_at))
    }
}

/// Whether two versions of a plan source every edge the same way.
fn same_sourcing(a: &RootPlanRecords, b: &RootPlanRecords) -> bool {
    let edges = |records: &RootPlanRecords| {
        let mut edges: Vec<_> = records
            .dependencies
            .iter()
            .map(|edge| {
                (
                    edge.consumer_build_id.0,
                    edge.component_type_id,
                    format!("{:?}", edge.sourcing),
                    edge.producer_build_id.map(|id| id.0),
                )
            })
            .collect();
        edges.sort();
        edges
    };
    edges(a) == edges(b)
}
