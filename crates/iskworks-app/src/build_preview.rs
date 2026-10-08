//! `BuildPreviewCoordinator` -- the build-plan preview pipeline's shared
//! I/O prelude and collaborator accessors.
//!
//! Saved Builds are previewed through
//! [`crate::BuildMaterialsCoordinator::preview_plan_with_planning_cost`] /
//! [`crate::BuildMaterialsCoordinator::candidate_preview_with_planning_cost`],
//! which read allocation-aware planning cost from `BuildCostProjection`.
//! That projection needs a **persisted** root (`project_build_materials`
//! resolves the Build's plan root, same as Build Graph), which a brand-new,
//! never-saved Build cannot supply -- so this coordinator keeps the
//! unsaved-Build preview, `BuildPreviewCoordinator::preview_unsaved_plan`,
//! which `BuildMaterialsCoordinator` delegates to only for that one case.
//! It also holds the shared prelude both flows call --
//! `prepare_preview_inputs` (ESI market-coverage registration, real
//! material coverage from the `ProductionRepository`, component
//! installation-EIV pre-resolution via `EsiApplicationService`) -- plus
//! `calculate_epic_snapshot` (Epic/ticket freezing, priced by
//! `IndustryService` rather than `BuildCostProjection`) and the collaborator
//! accessors (`industry_service`, `inventory_repository`, `sde_repository`,
//! `esi_service`, `adjusted_price_repository`, `reserved_quantity`)
//! `BuildMaterialsCoordinator` composes with. This code exists at the
//! application layer because `IndustryService` is I/O-free by design and
//! cannot reach ESI or the production/market repositories.
//!
//! Axum-free, `AppState`-free, `ApiError`-free. The HTTP route keeps
//! request extraction, `workspace_context`, coordinator construction from
//! `AppState`, `BuildPreviewError -> ApiError` conversion, and `Json`
//! wrapping.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use iskworks_core::{
    recipe_selection_of, AdjustedPriceRepository, Build, BuildId, BuildPlanRevision,
    ComponentExpansion, ComponentExpansionError, ComponentExpansionService, ComponentResolution,
    ComponentResolutionOutcome, FacilityRole, FulfillmentScope, FulfillmentScopeOverride,
    GraphMarketEvidence, IndustryError, IndustryRepository, IndustryService, InventoryError,
    InventoryRepository, MarketCoverageRegistration, MarketRepository, MarketScope,
    MaterialCoverageSummary, Money, OwnerId, PreviewBuildPlanCommand, ProductionError,
    ProductionRepository, RecipeSelection, WorkspaceId,
};
use iskworks_sde::{SdeError, SdeReadRepository};

use crate::{EsiApplicationError, EsiApplicationService, PublicMarketService};

/// Every error the build-preview application operation can produce.
///
/// The `#[from]` arms carry the domain errors the pipeline `?`-propagates.
/// The three `*Unavailable` arms are semantic markers for a
/// required-at-point-of-use collaborator that was not supplied -- the API
/// layer turns them into the matching `ApiError` (with its message
/// strings) in `From<BuildPreviewError> for ApiError`, keeping this
/// crate free of API wiring/error-message policy.
///
/// `MarketError` is intentionally absent: every market-coverage failure is
/// best-effort (logged and swallowed), so none reaches a caller.
#[derive(Debug, thiserror::Error)]
pub enum BuildPreviewError {
    #[error(transparent)]
    Industry(#[from] IndustryError),
    #[error(transparent)]
    ComponentExpansion(#[from] ComponentExpansionError),
    #[error(transparent)]
    Production(#[from] ProductionError),
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error(transparent)]
    Esi(#[from] EsiApplicationError),
    #[error(transparent)]
    StaticData(#[from] SdeError),
    #[error("production data is required for this build preview but was not provided")]
    ProductionUnavailable,
    #[error("inventory data is required for this build preview but was not provided")]
    InventoryUnavailable,
    #[error("EVE integration is required to resolve installation EIVs but was not provided")]
    EsiUnavailable,
}

/// The repositories and services every Build-planning coordinator is built
/// from. Only the industry and SDE repositories are required; each optional
/// one degrades the projections that need it (see `BuildPreviewError`'s
/// `*Unavailable` variants) instead of failing construction.
#[derive(Clone)]
pub struct BuildPlanningDeps {
    pub industry_repository: Arc<dyn IndustryRepository>,
    pub sde_repository: Arc<dyn SdeReadRepository>,
    pub production_repository: Option<Arc<dyn ProductionRepository>>,
    pub inventory_repository: Option<Arc<dyn InventoryRepository>>,
    pub market_repository: Option<Arc<dyn MarketRepository>>,
    pub public_market_service: Option<Arc<PublicMarketService>>,
    pub esi_service: Option<Arc<EsiApplicationService>>,
    pub adjusted_price_repository: Option<Arc<dyn AdjustedPriceRepository>>,
}

/// Application orchestration for the two build-plan preview flows.
///
/// Constructed per request from `AppState`. `industry_repository` and
/// `sde_repository` are always required; the rest mirror `AppState`'s own
/// optionality, so a missing collaborator fails only the operation that
/// needs it (see the per-method notes).
pub struct BuildPreviewCoordinator {
    industry_repository: Arc<dyn IndustryRepository>,
    sde_repository: Arc<dyn SdeReadRepository>,
    production_repository: Option<Arc<dyn ProductionRepository>>,
    inventory_repository: Option<Arc<dyn InventoryRepository>>,
    market_repository: Option<Arc<dyn MarketRepository>>,
    public_market_service: Option<Arc<PublicMarketService>>,
    esi_service: Option<Arc<EsiApplicationService>>,
    /// Testability seam: the narrow `AdjustedPriceRepository` abstraction
    /// (the same one `AppState` wires independently of a full `EsiApplicationService`, e.g. for
    /// Opportunities scanning), used as a bulk-resolution fallback for
    /// `BuildCostProjection`'s EIV inputs when no `EsiApplicationService` is
    /// wired. Lets API/application tests fake adjusted prices with a plain
    /// repository double -- no Postgres, no ESI transport -- instead of
    /// needing `EsiApplicationService::new_for_tests`'s `PgEsiRepository`.
    /// Production wiring wires both (`AppState::with_esi` sets one
    /// repository behind each); when `esi_service` is present its
    /// cache/fetch-on-miss path is preferred, so this fallback never
    /// weakens production behavior.
    adjusted_price_repository: Option<Arc<dyn AdjustedPriceRepository>>,
}

impl BuildPreviewCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        let BuildPlanningDeps {
            industry_repository,
            sde_repository,
            production_repository,
            inventory_repository,
            market_repository,
            public_market_service,
            esi_service,
            adjusted_price_repository,
        } = deps;
        Self {
            industry_repository,
            sde_repository,
            production_repository,
            inventory_repository,
            market_repository,
            public_market_service,
            esi_service,
            adjusted_price_repository,
        }
    }

    pub(crate) fn industry_service(&self) -> IndustryService {
        IndustryService::new(
            self.industry_repository.clone(),
            self.sde_repository.clone(),
        )
    }

    pub(crate) fn sde_repository(&self) -> Arc<dyn SdeReadRepository> {
        self.sde_repository.clone()
    }

    pub(crate) fn adjusted_price_repository(&self) -> Option<Arc<dyn AdjustedPriceRepository>> {
        self.adjusted_price_repository.clone()
    }

    /// The wired `EsiApplicationService`, if any. `BuildMaterialsCoordinator`
    /// uses it for the single bulk adjusted-price resolution its
    /// allocation-aware cost projection needs; `None` degrades that projection
    /// to "installation incomplete" rather than failing.
    pub(crate) fn esi_service(&self) -> Option<Arc<EsiApplicationService>> {
        self.esi_service.clone()
    }

    /// The inventory repository, or `InventoryUnavailable` -- the same
    /// point-of-use optionality the create-flow candidate preview relies on.
    /// `BuildMaterialsCoordinator` uses this for its single `list_balances`
    /// read.
    pub(crate) fn inventory_repository(
        &self,
    ) -> Result<&Arc<dyn InventoryRepository>, BuildPreviewError> {
        self.inventory_repository
            .as_ref()
            .ok_or(BuildPreviewError::InventoryUnavailable)
    }

    fn component_expansion_service(&self) -> ComponentExpansionService {
        ComponentExpansionService::new(self.sde_repository.clone())
    }

    /// Single-level cost path, used for exactly one case the planning-cost
    /// preview cannot serve: a **brand-new, never-saved**
    /// Build (`command.build_id: None`). `IndustryService::project_build_materials`
    /// (and therefore `BuildCostProjection`) requires a persisted root -- it
    /// rejects a missing `build_id` ("A graph requires a saved build")
    /// because it resolves and loads the Build's root plan, which cannot
    /// exist before the Build itself is saved.
    /// `BuildMaterialsCoordinator::preview_plan_with_planning_cost`
    /// delegates to this exact method when `command.build_id` is `None`; once
    /// the Build is saved, later requests carry a `build_id` and get the
    /// allocation-aware planning-cost path instead. Not otherwise called.
    pub(crate) async fn preview_unsaved_plan(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
    ) -> Result<BuildPlanRevision, BuildPreviewError> {
        let (material_coverage, component_eivs) = self
            .prepare_preview_inputs(workspace_id, owner_id, &command, true)
            .await?;
        let market_evidence = GraphMarketEvidence::new(command.market_evidence.clone());
        Ok(self
            .industry_service()
            .preview_plan(
                workspace_id,
                owner_id,
                command,
                &component_eivs,
                &material_coverage,
                (!market_evidence.is_empty()).then_some(&market_evidence),
            )
            .await?)
    }

    /// Authoritative Epic-snapshot calculation for `create_order`
    /// (`POST /api/builds/:id/orders`). Identical to
    /// `IndustryService::calculate_build_snapshot_with_coverage`, but first resolves live
    /// inventory coverage for the persisted `build` -- the *same*
    /// `ProductionRepository::coverage` path the Build worksheet uses -- and
    /// nets every `Missing`-scoped requirement against it, so the Epic
    /// freezes the reuse the user actually saw.
    ///
    /// **Inventory-neutral**: reads coverage only. No `inventory_events`,
    /// `inventory_allocations`, or balance/revision write. A `Full`-scoped
    /// requirement is excluded from the coverage map by
    /// [`Self::material_coverage`], so it freezes `reused_quantity == 0`.
    ///
    /// Requires `production_repository` at point of use
    /// (`ProductionUnavailable` otherwise -- mirrors the live preview).
    pub async fn calculate_epic_snapshot(
        &self,
        workspace_id: WorkspaceId,
        build: &Build,
    ) -> Result<BuildPlanRevision, BuildPreviewError> {
        let fulfillment_scopes = build
            .draft_planning
            .as_ref()
            .map(|draft| draft.input.fulfillment_scopes.clone())
            .unwrap_or_default();
        let material_coverage = self
            .material_coverage(workspace_id, Some(build.id), &fulfillment_scopes)
            .await?;
        Ok(self
            .industry_service()
            .calculate_build_snapshot_with_coverage(workspace_id, build, &material_coverage)
            .await?)
    }

    /// The prelude shared by both flows: selection market coverage,
    /// then (for a saved build) its plan's producer coverage, then real
    /// material coverage, then the derived `available_quantities`, then
    /// (when `resolve_component_eivs`) component installation EIVs. Coverage
    /// registration deliberately runs before EIV resolution (the frontend
    /// retry contract depends on it).
    ///
    /// `resolve_component_eivs: false` skips
    /// [`Self::resolve_component_installation_eivs`] entirely (returning an
    /// empty map for it) -- for a caller that will immediately overwrite
    /// every line's `installation_cost` from a different source anyway
    /// (`BuildMaterialsCoordinator::preview_plan_with_planning_cost`, via
    /// `BuildCostProjection::apply_to_revision`), so resolving it here would
    /// be pure wasted ESI-dependent work -- and worse, could needlessly fail
    /// the whole request (`EsiUnavailable`) when no `EsiApplicationService`
    /// is wired, even though `BuildCostProjection`'s own
    /// `AdjustedPriceRepository` fallback would have priced the row fine.
    /// Every other caller (the unsaved-Build single-level preview, Build Graph)
    /// keeps this `true`: their `installation_cost` display *is* this value.
    pub(crate) async fn prepare_preview_inputs(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: &PreviewBuildPlanCommand,
        resolve_component_eivs: bool,
    ) -> Result<(BTreeMap<i64, MaterialCoverageSummary>, BTreeMap<i64, Money>), BuildPreviewError>
    {
        self.register_selection_market_coverage(
            workspace_id,
            command.material_scope,
            command.output_scope,
            &command.recipe,
            command.runs,
            &command.component_resolutions,
        )
        .await?;
        if let Some(build_id) = command.build_id {
            self.register_producer_market_coverage(workspace_id, build_id)
                .await?;
        }
        let material_coverage = self
            .material_coverage(workspace_id, command.build_id, &command.fulfillment_scopes)
            .await?;
        let component_eivs = if resolve_component_eivs {
            let available_quantities: BTreeMap<i64, u64> = material_coverage
                .iter()
                .map(|(type_id, summary)| (*type_id, summary.available_to_this_build))
                .collect();
            self.resolve_component_installation_eivs(
                workspace_id,
                owner_id,
                command,
                &available_quantities,
            )
            .await?
        } else {
            BTreeMap::new()
        };
        Ok((material_coverage, component_eivs))
    }

    /// Real inventory availability for every material on the worksheet that
    /// isn't explicitly `Full`-scoped, keyed by type_id -- `Missing` is the
    /// default (an unset row behaves as `Missing`), so this returns coverage
    /// for everything except explicit `Full` overrides. `None` `build_id` (a
    /// brand-new, unsaved build) yields an empty map, meaning every row
    /// behaves like `Full` scope (nothing to subtract) since there's no
    /// inventory data source yet. Needs a `ProductionRepository` only once a
    /// `build_id` is present -- `ProductionUnavailable` if one isn't wired.
    async fn material_coverage(
        &self,
        workspace_id: WorkspaceId,
        build_id: Option<BuildId>,
        fulfillment_scopes: &[FulfillmentScopeOverride],
    ) -> Result<BTreeMap<i64, MaterialCoverageSummary>, BuildPreviewError> {
        let Some(build_id) = build_id else {
            return Ok(BTreeMap::new());
        };
        let full_scoped: BTreeSet<i64> = fulfillment_scopes
            .iter()
            .filter(|scope_override| scope_override.scope == FulfillmentScope::Full)
            .map(|scope_override| scope_override.type_id)
            .collect();
        let coverage = self
            .production_repository
            .as_ref()
            .ok_or(BuildPreviewError::ProductionUnavailable)?
            .coverage(workspace_id, build_id)
            .await?;
        Ok(coverage
            .material_lines
            .into_iter()
            .filter(|line| !full_scoped.contains(&line.type_id))
            .map(|line| {
                (
                    line.type_id,
                    MaterialCoverageSummary {
                        available_to_this_build: line.available_to_this_build,
                        average_historical_unit_cost: line.average_historical_unit_cost,
                    },
                )
            })
            .collect())
    }

    /// Sum of currently-active reservations for `type_id`, or 0 when no
    /// `ProductionRepository` is wired -- mirrors `AppState::reserved_quantity`.
    pub(crate) async fn reserved_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, BuildPreviewError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .reserved_quantity(workspace_id, owner_id, type_id)
                .await?),
            None => Ok(0),
        }
    }

    /// Pre-resolves each build-resolved component's own automatic EIV, keyed
    /// by type_id, for whichever components' kind matches a selected facility
    /// slot. EIV computation needs ESI/HTTP, which `IndustryService` can't do
    /// itself (core is I/O-free by design) -- this is the same automatic-EIV
    /// mechanism already used for the root job, just resolved once per
    /// build-resolved row.
    ///
    /// A row with a `facility_override` is pre-validated (role + revision, no
    /// ESI involved) before it's allowed to trigger an EIV fetch. A row with
    /// a `blueprint_selection` is resolved into its own (ME, TE) pair the
    /// same permissive way, purely to keep this preflight's own `expand()`
    /// call shaped the same as `preview_plan`'s.
    ///
    /// `available_quantities` is threaded into `expand()` so a row fully
    /// covered by inventory resolves to `Buy` here exactly as it will in
    /// `preview_plan`, and no EIV is wastefully fetched for it. ESI is only
    /// touched once at least one row genuinely needs an EIV -- `EsiUnavailable`
    /// if one is needed but no `EsiApplicationService` is wired.
    async fn resolve_component_installation_eivs(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: &PreviewBuildPlanCommand,
        available_quantities: &BTreeMap<i64, u64>,
    ) -> Result<BTreeMap<i64, Money>, BuildPreviewError> {
        let mut eivs = BTreeMap::new();
        if command.component_resolutions.is_empty() {
            return Ok(eivs);
        }
        let industry_service = self.industry_service();
        let mut component_blueprint_efficiencies: BTreeMap<i64, (u8, u8)> = BTreeMap::new();
        for resolution in &command.component_resolutions {
            if let Some(selection) = resolution.blueprint_selection.as_ref() {
                let RecipeSelection::Manufacturing { blueprint_type_id } = resolution.recipe else {
                    // Invalid (blueprint selection on a reaction) -- `preview_plan`
                    // will raise the real validation error shortly.
                    continue;
                };
                if let Ok(efficiency) = industry_service
                    .resolve_component_blueprint_efficiency(
                        workspace_id,
                        owner_id,
                        blueprint_type_id,
                        selection,
                    )
                    .await
                {
                    component_blueprint_efficiencies.insert(resolution.type_id, efficiency);
                }
            }
        }
        let expansion = self
            .component_expansion_service()
            .expand(
                command.recipe,
                command.runs,
                &command.component_resolutions,
                &component_blueprint_efficiencies,
                available_quantities,
            )
            .await?;
        let wanted = components_wanting_installation_eiv(
            command,
            &command.component_resolutions,
            &expansion,
        );
        if wanted.is_empty() {
            return Ok(eivs);
        }
        let resolutions_by_type: BTreeMap<i64, &ComponentResolution> = command
            .component_resolutions
            .iter()
            .map(|resolution| (resolution.type_id, resolution))
            .collect();
        let mut confirmed = Vec::with_capacity(wanted.len());
        for (type_id, recipe, runs) in wanted {
            if let Some(override_) = resolutions_by_type
                .get(&type_id)
                .and_then(|resolution| resolution.facility_override.as_ref())
            {
                let expected_role = match recipe {
                    RecipeSelection::Manufacturing { .. } => FacilityRole::Manufacturing,
                    RecipeSelection::Reaction { .. } => FacilityRole::Reaction,
                };
                if industry_service
                    .resolve_override_profile(workspace_id, override_, expected_role)
                    .await
                    .is_err()
                {
                    // Invalid override -- `preview_plan` will raise the real
                    // validation error shortly; don't fetch EIV for it.
                    continue;
                }
            }
            confirmed.push((type_id, recipe, runs));
        }
        if confirmed.is_empty() {
            return Ok(eivs);
        }
        let esi_service = self
            .esi_service
            .as_ref()
            .ok_or(BuildPreviewError::EsiUnavailable)?;
        for (type_id, recipe, runs) in confirmed {
            let captured = industry_service.capture_recipe(&recipe).await?;
            let eiv = esi_service
                .automatic_eiv(captured.materials(), runs)
                .await?;
            if let Some(value) = eiv.value {
                eivs.insert(type_id, value);
            }
        }
        Ok(eivs)
    }

    /// Get-or-create the workspace's ESI source for `scope` and
    /// register+refresh `coverage` against it -- a no-op for an empty list.
    /// Best-effort: every failure (no market repository, no public market
    /// service, provisioning error, refresh error) is logged and swallowed,
    /// never failing the preview request. Background refresh plumbing, not
    /// something the preview's own accuracy depends on synchronously.
    async fn register_market_coverage_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        coverage: Vec<MarketCoverageRegistration>,
    ) {
        if coverage.is_empty() {
            return;
        }
        let Some(repository) = self.market_repository.as_ref() else {
            tracing::warn!(
                "market coverage registration unavailable: no market repository configured"
            );
            return;
        };
        let source_id = match repository
            .ensure_esi_price_source_for_scope(workspace_id, scope)
            .await
        {
            Ok(source_id) => source_id,
            Err(error) => {
                tracing::warn!(%error, region_id = scope.region_id, "market coverage registration failed");
                return;
            }
        };
        let Some(service) = self.public_market_service.as_ref() else {
            tracing::warn!(
                "market coverage registration unavailable: no public market service configured"
            );
            return;
        };
        if let Err(error) = service
            .register_and_refresh(workspace_id, source_id, coverage)
            .await
        {
            tracing::warn!(%error, region_id = scope.region_id, "market coverage refresh failed");
        }
    }

    async fn register_selection_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        material_scope: MarketScope,
        output_scope: MarketScope,
        selection: &RecipeSelection,
        runs: u64,
        component_resolutions: &[ComponentResolution],
    ) -> Result<(), BuildPreviewError> {
        let recipe = self.industry_service().capture_recipe(selection).await?;
        let mut material_coverage: Vec<MarketCoverageRegistration> = recipe
            .materials()
            .iter()
            .map(|line| MarketCoverageRegistration {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
            })
            .collect();
        if !component_resolutions.is_empty() {
            // Worksheets are single-level, so this only ever expands to this
            // recipe's own direct materials -- a resolved row's own further
            // materials are covered separately, by that row's producer Build's
            // own registration (`register_producer_market_coverage`).
            let expansion = self
                .component_expansion_service()
                .expand(
                    *selection,
                    runs,
                    component_resolutions,
                    &BTreeMap::new(), // only type_ids matter here, unaffected by TE
                    &BTreeMap::new(), // ...or by fulfillment scope
                )
                .await?;
            let mut seen: BTreeSet<i64> =
                material_coverage.iter().map(|item| item.type_id).collect();
            for component in expansion.components {
                if seen.insert(component.type_id) {
                    material_coverage.push(MarketCoverageRegistration {
                        type_id: component.type_id,
                        type_name: component.type_name,
                    });
                }
            }
        }
        let seen_materials: BTreeSet<i64> =
            material_coverage.iter().map(|item| item.type_id).collect();
        let output_coverage: Vec<MarketCoverageRegistration> = recipe
            .products()
            .iter()
            .filter(|line| !seen_materials.contains(&line.type_id))
            .map(|line| MarketCoverageRegistration {
                type_id: line.type_id,
                type_name: line.type_name.clone(),
            })
            .collect();
        self.register_market_coverage_for_scope(workspace_id, material_scope, material_coverage)
            .await;
        self.register_market_coverage_for_scope(workspace_id, output_scope, output_coverage)
            .await;
        Ok(())
    }

    /// Registers ESI market coverage for every producer Build of the plan
    /// `build_id` belongs to -- the operations the plan's cost projection
    /// prices besides the previewed recipe itself. Producers are found
    /// through the plan's Produce demand edges; each registers its own
    /// recipe and resolutions in its own scopes. A Build outside any plan,
    /// or a producer with no draft planning yet, registers nothing.
    async fn register_producer_market_coverage(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<(), BuildPreviewError> {
        let Some(root) = self
            .industry_repository
            .plan_root_of(workspace_id, build_id)
            .await?
        else {
            return Ok(());
        };
        let records = self
            .industry_repository
            .load_root_plan(workspace_id, root)
            .await?;
        let referenced: std::collections::HashSet<BuildId> = records
            .dependencies
            .iter()
            .filter_map(|edge| edge.producer_build_id)
            .collect();
        for producer in records
            .producers
            .iter()
            .filter(|producer| referenced.contains(&producer.id))
        {
            let Some(draft) = producer.draft_planning.as_ref() else {
                continue;
            };
            self.register_selection_market_coverage(
                workspace_id,
                draft.input.material_scope,
                draft.input.output_scope,
                &recipe_selection_of(&producer.recipe),
                producer.runs,
                &draft.input.component_resolutions,
            )
            .await?;
        }
        Ok(())
    }
}

/// A build-resolved row wants its own EIV resolved if either its own per-row
/// `facility_override` is set, or -- absent an override -- the build-level
/// shared slot matching its kind is selected. An override can want EIV even
/// when *neither* shared slot is selected at all.
fn components_wanting_installation_eiv(
    command: &PreviewBuildPlanCommand,
    resolutions: &[ComponentResolution],
    expansion: &ComponentExpansion,
) -> Vec<(i64, RecipeSelection, u64)> {
    let resolutions_by_type: BTreeMap<i64, &ComponentResolution> = resolutions
        .iter()
        .map(|resolution| (resolution.type_id, resolution))
        .collect();
    expansion
        .components
        .iter()
        .filter_map(|component| {
            let ComponentResolutionOutcome::Build { recipe, runs, .. } = &component.resolution
            else {
                return None;
            };
            let has_override = resolutions_by_type
                .get(&component.type_id)
                .is_some_and(|resolution| resolution.facility_override.is_some());
            let wants_shared_slot = match recipe {
                RecipeSelection::Manufacturing { .. } => command.manufacturing_facility.is_some(),
                RecipeSelection::Reaction { .. } => command.reaction_facility.is_some(),
            };
            (has_override || wants_shared_slot).then_some((component.type_id, *recipe, *runs))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iskworks_core::{
        ComponentFacilityOverride, Contribution, ContributionSource, FacilityPreviewCommand,
        FacilityProfileId, ResolvedComponent,
    };

    fn manufacturing(blueprint_type_id: i64) -> RecipeSelection {
        RecipeSelection::Manufacturing { blueprint_type_id }
    }

    fn reaction(reaction_formula_type_id: i64) -> RecipeSelection {
        RecipeSelection::Reaction {
            reaction_formula_type_id,
        }
    }

    fn build_resolved_component(
        type_id: i64,
        total_quantity: u64,
        recipe: RecipeSelection,
    ) -> ResolvedComponent {
        ResolvedComponent {
            type_id,
            type_name: format!("Type {type_id}"),
            total_quantity,
            contributions: vec![Contribution {
                source: ContributionSource::Root,
                quantity: total_quantity,
            }],
            resolution: ComponentResolutionOutcome::Build {
                recipe,
                runs: 1,
                produced_quantity: total_quantity,
                surplus: 0,
                duration_seconds: None,
            },
        }
    }

    fn buy_component(type_id: i64, total_quantity: u64) -> ResolvedComponent {
        ResolvedComponent {
            type_id,
            type_name: format!("Type {type_id}"),
            total_quantity,
            contributions: vec![Contribution {
                source: ContributionSource::Root,
                quantity: total_quantity,
            }],
            resolution: ComponentResolutionOutcome::Buy,
        }
    }

    fn base_command(component_resolutions: Vec<ComponentResolution>) -> PreviewBuildPlanCommand {
        PreviewBuildPlanCommand {
            recipe: manufacturing(1),
            runs: 1,
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            manufacturing_facility: None,
            reaction_facility: None,
            blueprint_selection: None,
            component_resolutions,
            fulfillment_scopes: Vec::new(),
            build_id: None,
            market_evidence: Vec::new(),
        }
    }

    #[test]
    fn components_wanting_installation_eiv_includes_an_override_only_row_even_with_no_shared_slot_selected(
    ) {
        let command = base_command(vec![ComponentResolution {
            type_id: 90_001,
            recipe: reaction(200),
            facility_override: Some(ComponentFacilityOverride {
                facility_profile_id: FacilityProfileId::new(),
            }),
            blueprint_selection: None,
        }]);
        let expansion = ComponentExpansion {
            components: vec![build_resolved_component(90_001, 10, reaction(200))],
        };

        let wanted = components_wanting_installation_eiv(
            &command,
            &command.component_resolutions,
            &expansion,
        );

        assert_eq!(wanted, vec![(90_001, reaction(200), 1)]);
    }

    #[test]
    fn components_wanting_installation_eiv_still_requires_a_row_to_be_build_resolved() {
        let command = base_command(vec![ComponentResolution {
            type_id: 90_001,
            recipe: reaction(200),
            facility_override: Some(ComponentFacilityOverride {
                facility_profile_id: FacilityProfileId::new(),
            }),
            blueprint_selection: None,
        }]);
        // The row's own resolution outcome is Buy -- an override on it is
        // inert, matching how a Buy row never wants EIV today regardless of
        // the shared slots.
        let expansion = ComponentExpansion {
            components: vec![buy_component(90_001, 10)],
        };

        let wanted = components_wanting_installation_eiv(
            &command,
            &command.component_resolutions,
            &expansion,
        );

        assert!(wanted.is_empty());
    }

    #[test]
    fn components_wanting_installation_eiv_still_honors_the_shared_slot_without_an_override() {
        let mut command = base_command(vec![ComponentResolution {
            type_id: 90_001,
            recipe: manufacturing(200),
            facility_override: None,
            blueprint_selection: None,
        }]);
        command.manufacturing_facility = Some(FacilityPreviewCommand {
            facility_profile_id: FacilityProfileId::new(),
            blueprint_me: 0,
            blueprint_te: 0,
            estimated_item_value: None,
        });
        let expansion = ComponentExpansion {
            components: vec![build_resolved_component(90_001, 10, manufacturing(200))],
        };

        let wanted = components_wanting_installation_eiv(
            &command,
            &command.component_resolutions,
            &expansion,
        );

        assert_eq!(wanted, vec![(90_001, manufacturing(200), 1)]);
    }

    #[test]
    fn components_wanting_installation_eiv_excludes_a_row_with_neither_override_nor_shared_slot() {
        let command = base_command(vec![ComponentResolution {
            type_id: 90_001,
            recipe: manufacturing(200),
            facility_override: None,
            blueprint_selection: None,
        }]);
        let expansion = ComponentExpansion {
            components: vec![build_resolved_component(90_001, 10, manufacturing(200))],
        };

        let wanted = components_wanting_installation_eiv(
            &command,
            &command.component_resolutions,
            &expansion,
        );

        assert!(wanted.is_empty());
    }
}
