use super::plan_calculation::*;
use super::*;
use crate::GraphMarketEvidence;

mod blueprint_backfill;
mod facility_context;
mod preview;
mod projection;
mod recipe_capture;
mod sourcing;

pub use blueprint_backfill::*;

fn not_in_production_plan() -> IndustryError {
    IndustryError::Validation("This Build is not part of a production plan.".to_string())
}

/// Starts a newly created linked build's own facility/blueprint from the
/// parent resolution's, and its market scopes/manual fallback from whatever
/// the parent build itself currently has configured, so the user doesn't
/// re-enter what they already specified once. `None` when there's no parent
/// draft and no facility/blueprint override to prepopulate from either.
pub(super) fn prepopulated_draft_planning(
    resolution: &crate::ComponentResolution,
    recipe_selection: RecipeSelection,
    parent_draft: Option<&DraftPlanningInput>,
) -> Option<DraftPlanningSnapshot> {
    // Prefer the row's own per-row facility override; absent that, fall
    // back to the parent's own facility for the matching kind, so a fresh
    // linked build isn't left with no facility selected just because the
    // user never overrode this particular row -- only its own facility id
    // carries over, never the parent's blueprint ME/TE or EIV, which
    // describe the parent's own job, not the child's.
    let facility_source = resolution
        .facility_override
        .as_ref()
        .map(|override_| override_.facility_profile_id)
        .or_else(|| {
            parent_draft.and_then(|draft| match recipe_selection {
                RecipeSelection::Manufacturing { .. } => draft
                    .manufacturing_facility
                    .as_ref()
                    .map(|facility| facility.facility_profile_id),
                RecipeSelection::Reaction { .. } => draft
                    .reaction_facility
                    .as_ref()
                    .map(|facility| facility.facility_profile_id),
            })
        });
    if facility_source.is_none()
        && resolution.blueprint_selection.is_none()
        && parent_draft.is_none()
    {
        return None;
    }
    let (manufacturing_facility, reaction_facility) = match (recipe_selection, facility_source) {
        (RecipeSelection::Manufacturing { .. }, Some(facility_profile_id)) => (
            Some(FacilityPreviewCommand {
                facility_profile_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            }),
            None,
        ),
        (RecipeSelection::Reaction { .. }, Some(facility_profile_id)) => (
            None,
            Some(ReactionFacilityPreviewCommand {
                facility_profile_id,
                estimated_item_value: None,
            }),
        ),
        _ => (None, None),
    };
    let (material_scope, output_scope, manual_price_list_id, expected_manual_price_list_revision) =
        match parent_draft {
            Some(draft) => (
                draft.material_scope,
                draft.output_scope,
                draft.manual_price_list_id,
                draft.expected_manual_price_list_revision,
            ),
            None => (
                crate::default_market_scope(),
                crate::default_market_scope(),
                None,
                None,
            ),
        };
    Some(DraftPlanningSnapshot {
        input: DraftPlanningInput {
            material_scope,
            output_scope,
            manual_price_list_id,
            expected_manual_price_list_revision,
            material_pricing_policy: default_material_pricing_policy(),
            output_pricing_policy: default_output_pricing_policy(),
            pricing_selections: Vec::new(),
            blueprint_selection: resolution.blueprint_selection.clone(),
            manufacturing_facility,
            reaction_facility,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: Utc::now(),
    })
}

fn validate_name(name: &str) -> Result<(), IndustryError> {
    if !(1..=160).contains(&name.trim().chars().count()) {
        return Err(IndustryError::Validation(
            "Build name must be between 1 and 160 characters.".to_string(),
        ));
    }
    Ok(())
}

fn validate_price_source_name(name: &str) -> Result<(), IndustryError> {
    if !(1..=120).contains(&name.trim().chars().count()) {
        return Err(IndustryError::Validation(
            "Price Source name must be between 1 and 120 characters.".to_string(),
        ));
    }
    Ok(())
}

/// The ME/facility-adjusted final requirement per **root direct-material**
/// `type_id` -- the `effective_root_requirements` map fed to
/// `expand_component_tree` for the root recipe. Shared by
/// `preview_plan_inner` and the materials projection so a Graph/Materials
/// projection and a preview of the same overlay expand the root
/// identically.
fn derive_effective_root_requirements(
    recipe: &BuildRecipe,
    runs: u64,
    blueprint_me: u8,
    blueprint_te: u8,
    max_runs_per_job: Option<u64>,
    facility: Option<&FacilityPlanPreview>,
) -> Result<BTreeMap<i64, u64>, IndustryError> {
    Ok(match facility {
        Some(facility) => facility
            .requirements
            .iter()
            .map(|line| (line.type_id, line.final_required_quantity))
            .collect(),
        None => preview_recipe_effects(recipe, runs, blueprint_me, blueprint_te, max_runs_per_job)?
            .0
            .into_iter()
            .map(|line| (line.type_id, line.final_required_quantity))
            .collect(),
    })
}

/// What [`IndustryService::project_build_materials`] produced.
#[derive(Debug, Clone)]
pub enum BuildMaterialsProjectionOutcome {
    /// A complete, authoritative whole-tree aggregate.
    Complete(crate::build_materials::BuildMaterialsAggregate),
    /// One or more walked linked children had no reconstructable draft /
    /// preview -- the aggregate would understate demand. The API turns this
    /// into a curated `build_materials_incomplete` 422; `missing_nodes` is the
    /// diagnostic id list (never crossed to the client).
    Incomplete { missing_nodes: Vec<BuildId> },
    /// A walked linked child does not share the root's owner -- impossible
    /// under current domain rules (a linked child inherits its parent's owner
    /// at creation and it is immutable), so this signals corrupt data.
    MixedOwnerTree { node: BuildId },
    /// A canonical root plan's persisted
    /// producer graph cannot be planned (a cycle, a retired or unknown
    /// producer referenced, two active producers of one identity, or a
    /// structural edge diagnostic). Never a silently partial plan; the API
    /// turns this into a curated conflict.
    CanonicalGraphInvalid {
        error: crate::canonical_planner::CanonicalGraphError,
    },
}

/// [`IndustryService::project_build_materials`] result: the resolved root id
/// plus the projection outcome.
#[derive(Debug, Clone)]
pub struct BuildMaterialsProjection {
    pub root_build_id: BuildId,
    pub outcome: BuildMaterialsProjectionOutcome,
    /// The market-evidence identity this overlay's own pricing (and
    /// Graph's) was valued against -- resolved once by this projection,
    /// never a second read. Lets a caller (`BuildGraphCoordinator`) expose it on its
    /// own response without re-resolving it.
    pub market_evidence: crate::GraphMarketEvidence,
    /// This projection's own work counters.
    pub metrics: PlannerMetrics,
}

#[derive(Clone)]
pub struct IndustryService {
    repository: Arc<dyn IndustryRepository>,
    sde_repository: Arc<dyn SdeReadRepository>,
    counters: Arc<PlannerCounters>,
}

/// Deterministic work counters of one [`IndustryService`] instance (one per
/// request in the application layer). A projection reports its own delta
/// as [`PlannerMetrics`].
#[derive(Debug, Default)]
pub(super) struct PlannerCounters {
    pub(super) previews: std::sync::atomic::AtomicU64,
    pub(super) structural_previews: std::sync::atomic::AtomicU64,
    pub(super) root_plan_loads: std::sync::atomic::AtomicU64,
    pub(super) builds_loaded: std::sync::atomic::AtomicU64,
}

impl PlannerCounters {
    fn snapshot(&self) -> PlannerMetrics {
        use std::sync::atomic::Ordering::Relaxed;
        let previews = self.previews.load(Relaxed);
        let structural = self.structural_previews.load(Relaxed);
        PlannerMetrics {
            exact_previews: previews.saturating_sub(structural),
            structural_discovery_previews: structural,
            root_plan_loads: self.root_plan_loads.load(Relaxed),
            builds_loaded: self.builds_loaded.load(Relaxed),
        }
    }
}

/// What one projection did -- previews run
/// (exact, and structural `runs = 1` discovery ones), bounded root-plan
/// loads, and Builds hydrated. Deterministic for a given plan; asserted by tests and logged by
/// the application layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerMetrics {
    pub exact_previews: u64,
    pub structural_discovery_previews: u64,
    pub root_plan_loads: u64,
    pub builds_loaded: u64,
}

impl PlannerMetrics {
    fn since(self, start: Self) -> Self {
        Self {
            exact_previews: self.exact_previews - start.exact_previews,
            structural_discovery_previews: self.structural_discovery_previews
                - start.structural_discovery_previews,
            root_plan_loads: self.root_plan_loads - start.root_plan_loads,
            builds_loaded: self.builds_loaded - start.builds_loaded,
        }
    }
}

/// The one field of a node `register_production_operation` reads.
struct BuildNodeRef<'a> {
    build: &'a Build,
}

/// The primitive-input verification evidence for one boundary of an
/// operation previewed as `revision` at `node_runs`.
pub(super) fn boundary_verification(
    revision: &BuildPlanRevision,
    activity: crate::build_materials::MaterialActivity,
    node_runs: u64,
    type_id: i64,
    starting_inventory: u64,
    parent_traversal_index: Option<u32>,
    op_index: u32,
) -> crate::build_materials::BoundaryVerification {
    let effective = revision
        .effective_requirements
        .iter()
        .find(|requirement| requirement.type_id == type_id);
    let unit_price = revision
        .material_lines
        .iter()
        .find(|line| line.type_id == type_id)
        .and_then(|line| line.unit_price);
    let snapshot_line = revision
        .snapshot
        .items
        .iter()
        .find(|line| line.item_role == PlannerItemRole::Material && line.type_id == type_id);
    crate::build_materials::BoundaryVerification {
        activity,
        node_runs,
        base_quantity_per_run: effective.map_or(0, |requirement| requirement.base_quantity_per_run),
        blueprint_me: revision
            .blueprint
            .as_ref()
            .map_or(0, |snapshot| snapshot.material_efficiency),
        facility_material_factor: effective.map_or(rust_decimal::Decimal::ONE, |requirement| {
            requirement.facility_material_factor
        }),
        starting_inventory,
        parent_traversal_index,
        op_index,
        fresh_unit_price: unit_price,
        fresh_price_selection: snapshot_line
            .map_or(PricingSelectionKind::Default, |line| line.selection_kind),
        fresh_pricing_policy: snapshot_line.and_then(|line| line.pricing_policy),
        fresh_price_note: snapshot_line.map_or_else(String::new, |line| line.source_note.clone()),
        fresh_price_stale: snapshot_line
            .is_some_and(|line| line.source_note.contains(crate::STALE_MARKET_PROVENANCE)),
        market_region_id: snapshot_line.and_then(|line| line.market_region_id),
        market_location_id: snapshot_line.and_then(|line| line.market_location_id),
        intended_recipe: None,
    }
}

/// Register one production operation (Graph/Verification/execution-plan
/// visibility) from its Build and that Build's real preview.
///
/// Returns the registered `op_index`, or `0` when `capture_verification` is
/// `false` (a no-op, matching `MaterialsAccumulator::record_operation`'s
/// own documented behavior on the ordinary Materials path).
#[allow(clippy::too_many_arguments)]
pub(super) fn register_production_operation(
    build: &Build,
    revision: &BuildPlanRevision,
    is_root: bool,
    node_runs: u64,
    parent_path: &[i64],
    incoming: Vec<crate::build_materials::OperationIncomingDemand>,
    capture_verification: bool,
    acc: &mut crate::build_materials::MaterialsAccumulator,
) -> u32 {
    use crate::build_materials::{MaterialActivity, VerificationOperationInput};

    if !capture_verification {
        return 0;
    }
    let node = BuildNodeRef { build };
    let build_id = node.build.id;
    let graph_node_id = if is_root {
        format!("root:{}", build_id.0)
    } else {
        format!("build:{}", build_id.0)
    };
    let node_activity = match node.build.recipe.kind() {
        BuildRecipeKind::Manufacturing => MaterialActivity::Manufacturing,
        BuildRecipeKind::Reaction => MaterialActivity::Reaction,
    };
    let node_material_factor = revision
        .effective_requirements
        .first()
        .map_or(rust_decimal::Decimal::ONE, |requirement| {
            requirement.facility_material_factor
        });
    let facility = revision.root_facility();
    let product = node.build.recipe.primary_product();
    let parent_op_index = incoming.first().map(|demand| demand.consumer_op_index);
    let parent_traversal_index = incoming.first().map(|demand| demand.traversal_index);
    acc.record_operation(VerificationOperationInput {
        op_index: 0, // overwritten by `record_operation`
        parent_op_index,
        parent_traversal_index,
        incoming,
        graph_node_id,
        build_id,
        revision: node.build.revision,
        tree_path: parent_path.to_vec(),
        activity: node_activity,
        product_type_id: product.type_id,
        product_name: product.type_name.clone(),
        output_per_run: product.quantity_per_run,
        base_material_count: u32::try_from(node.build.recipe.materials().len()).unwrap_or(u32::MAX),
        blueprint_or_formula_type_id: node
            .build
            .recipe
            .blueprint_type_id()
            .or_else(|| node.build.recipe.reaction_formula_type_id())
            .unwrap_or_default(),
        blueprint_or_formula_name: node.build.recipe.name().to_string(),
        node_runs,
        persisted_runs: node.build.runs,
        recipe_currency: node.build.recipe_currency,
        me: revision
            .blueprint
            .as_ref()
            .map(|snapshot| snapshot.material_efficiency),
        te: revision
            .blueprint
            .as_ref()
            .map(|snapshot| snapshot.time_efficiency),
        blueprint_selection: node
            .build
            .draft_planning
            .as_ref()
            .and_then(|snapshot| snapshot.input.blueprint_selection.clone()),
        facility_id: facility.map(|preview| preview.profile.id.0),
        facility_name: facility.map(|preview| preview.profile.name.clone()),
        structure_type: facility.map(|preview| preview.profile.structure_type_name.clone()),
        solar_system: facility.map(|preview| preview.profile.solar_system_name.clone()),
        structure_material_reduction_percent: facility
            .map_or(rust_decimal::Decimal::ZERO, |preview| {
                preview.profile.material_reduction_percent
            }),
        structure_time_reduction_percent: facility.map_or(rust_decimal::Decimal::ZERO, |preview| {
            preview.profile.time_reduction_percent
        }),
        effective_material_factor: node_material_factor,
        facility_profile_revision: facility.map(|preview| preview.profile.revision),
        system_cost_index: facility.and_then(|preview| preview.profile.manual_system_cost_index),
        job_cost_reduction_percent: facility.map_or(rust_decimal::Decimal::ZERO, |preview| {
            preview.profile.job_cost_reduction_percent
        }),
        facility_tax_percent: facility.map_or(rust_decimal::Decimal::ZERO, |preview| {
            preview.profile.facility_tax_percent
        }),
        scc_surcharge_percent: facility.map_or(rust_decimal::Decimal::ZERO, |preview| {
            preview.profile.scc_surcharge_percent
        }),
        alliance_surcharge_percent: facility.map_or(rust_decimal::Decimal::ZERO, |preview| {
            preview.profile.alliance_surcharge_percent
        }),
        fixed_supplemental_cost: facility.map_or(Money::zero(), |preview| {
            preview.profile.fixed_supplemental_cost
        }),
        job_count: crate::JobSplit::for_runs(
            node_runs,
            revision
                .blueprint
                .as_ref()
                .and_then(crate::BlueprintSnapshot::max_runs_per_job),
        )
        .job_count(),
        installation_formula_version: facility.map_or_else(String::new, |preview| {
            preview.installation_cost.formula_version.clone()
        }),
    })
}

pub(super) fn ensure_canonical_run_update(
    plan_root: BuildId,
    build_id: BuildId,
    persisted_runs: u64,
    requested_runs: u64,
) -> Result<(), IndustryError> {
    if build_id != plan_root && requested_runs != persisted_runs {
        return Err(IndustryError::Validation(
            "Runs are derived from the top-level Build plan for this production step.".to_string(),
        ));
    }
    Ok(())
}

impl IndustryService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn IndustryRepository>,
        sde_repository: Arc<dyn SdeReadRepository>,
    ) -> Self {
        Self {
            repository,
            sde_repository,
            counters: Arc::new(PlannerCounters::default()),
        }
    }

    pub(super) fn counters(&self) -> &PlannerCounters {
        &self.counters
    }

    /// The linked-build / build persistence repository, for sibling modules.
    pub(super) fn build_repository(&self) -> Arc<dyn IndustryRepository> {
        self.repository.clone()
    }

    pub async fn create_draft(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: CreateBuildCommand,
    ) -> Result<Build, IndustryError> {
        validate_name(&command.name)?;
        validate_runs(command.runs)?;
        let recipe = self.capture_recipe(&command.recipe).await?;
        let now = Utc::now();
        let draft_planning = match command.draft_planning {
            Some(input) => Some(
                self.capture_effective_draft_blueprints(workspace_id, owner_id, &recipe, input)
                    .await?,
            ),
            None => None,
        };
        let draft_planning = draft_planning
            .map(|input| crate::normalize_draft_planning(input, command.runs))
            .transpose()?
            .map(|input| DraftPlanningSnapshot {
                input,
                updated_at: now,
            });
        let build = self
            .repository
            .create_root_build(NewBuild {
                build: Build {
                    id: BuildId::new(),
                    workspace_id,
                    owner_id,
                    name: command.name.trim().to_string(),
                    recipe,
                    runs: command.runs,
                    notes: command.notes.trim().to_string(),
                    revision: 1,
                    created_at: now,
                    updated_at: now,
                    draft_planning,
                    recipe_currency: RecipeCurrency::Current,
                    active_sde_version: None,
                    product_category_name: None,
                    product_group_name: None,
                    selected_blueprint_origin: None,
                    has_owned_blueprint: false,
                },
            })
            .await?;
        // The repository writes a new root as a canonical plan whose demand
        // edges all start as Buy/Missing; a draft that already sources a
        // component (Build, or a Full scope) is applied through the canonical
        // write, like any later sourcing change.
        let sourced = build.draft_planning.as_ref().is_some_and(|snapshot| {
            !snapshot.input.component_resolutions.is_empty()
                || snapshot
                    .input
                    .fulfillment_scopes
                    .iter()
                    .any(|scope| scope.scope == crate::FulfillmentScope::Full)
        });
        if !sourced {
            return Ok(build);
        }
        self.write_canonical_consumer(
            workspace_id,
            build.id,
            build.id,
            DraftUpdate {
                expected_revision: build.revision,
                name: build.name.clone(),
                runs: build.runs,
                notes: build.notes.clone(),
                replacement_recipe: None,
                draft_planning: build.draft_planning.clone(),
            },
        )
        .await
    }

    pub async fn update_draft(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        command: UpdateBuildCommand,
    ) -> Result<Build, IndustryError> {
        validate_name(&command.name)?;
        validate_runs(command.runs)?;
        let current = self.repository.get_build(workspace_id, build_id).await?;
        let canonical_root = self.plan_root_of(workspace_id, build_id).await?;
        if let Some(root) = canonical_root {
            ensure_canonical_run_update(root, build_id, current.runs, command.runs)?;
        }
        let replacement_recipe = match (&current.recipe, &command.recipe) {
            (
                BuildRecipe::Manufacturing(recipe),
                RecipeSelection::Manufacturing { blueprint_type_id },
            ) if recipe.blueprint_type_id == *blueprint_type_id => None,
            (
                BuildRecipe::Reaction(formula),
                RecipeSelection::Reaction {
                    reaction_formula_type_id,
                },
            ) if formula.reaction_formula_type_id == *reaction_formula_type_id => None,
            _ => Some(self.capture_recipe(&command.recipe).await?),
        };
        let effective_recipe = replacement_recipe.as_ref().unwrap_or(&current.recipe);
        let draft_planning = match command.draft_planning {
            Some(input) => Some(
                self.capture_effective_draft_blueprints(
                    workspace_id,
                    current.owner_id,
                    effective_recipe,
                    input,
                )
                .await?,
            ),
            None => None,
        };
        let draft_planning = draft_planning
            .map(|input| crate::normalize_draft_planning(input, command.runs))
            .transpose()?
            .map(|input| DraftPlanningSnapshot {
                input,
                updated_at: Utc::now(),
            });
        let update = DraftUpdate {
            expected_revision: command.expected_revision,
            name: command.name.trim().to_string(),
            runs: command.runs,
            notes: command.notes.trim().to_string(),
            replacement_recipe,
            draft_planning,
        };
        // A plan's sourcing lives on its demand edges: the save and the edge
        // writes it implies are one transaction.
        let root = canonical_root.ok_or_else(not_in_production_plan)?;
        self.write_canonical_consumer(workspace_id, root, build_id, update)
            .await
    }

    pub async fn rename_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        command: RenameBuildCommand,
    ) -> Result<Build, IndustryError> {
        validate_name(&command.name)?;
        self.repository
            .rename_build(workspace_id, build_id, command.name.trim().to_string())
            .await
    }

    pub async fn create_price_source(
        &self,
        workspace_id: WorkspaceId,
        command: CreatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        validate_price_source_name(&command.name)?;
        let now = Utc::now();
        self.repository
            .create_price_source(PriceSource {
                id: PriceSourceId::new(),
                workspace_id,
                name: command.name.trim().to_string(),
                description: command.description.trim().to_string(),
                kind: PriceSourceKind::Manual,
                revision: 1,
                item_count: 0,
                recent_build_count: 0,
                items: Vec::new(),
                created_at: now,
                updated_at: now,
            })
            .await
    }

    pub async fn parse_price_items(
        &self,
        inputs: Vec<PriceInput>,
    ) -> Result<Vec<PriceSourceItem>, IndustryError> {
        let now = Utc::now();
        let mut seen = BTreeSet::new();
        inputs
            .into_iter()
            .map(|input| {
                if input.type_id <= 0
                    || input.type_name.trim().is_empty()
                    || !seen.insert(input.type_id)
                {
                    return Err(IndustryError::Validation(
                        "Each price must identify one unique EVE item.".to_string(),
                    ));
                }
                Ok(PriceSourceItem {
                    type_id: input.type_id,
                    type_name: input.type_name.trim().to_string(),
                    price: Money::parse(&input.price)?,
                    note: input.note.trim().to_string(),
                    updated_at: now,
                })
            })
            .collect()
    }
}
