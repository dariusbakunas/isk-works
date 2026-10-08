use super::*;

impl IndustryService {
    /// `component_eivs` is pre-resolved by the caller (route layer, since
    /// it requires ESI/HTTP -- see `IndustryService`'s I/O-free-core
    /// contract) for each build-resolved component whose kind matches a
    /// selected facility slot. Missing a key means that component's
    /// installation cost simply can't be computed yet.
    ///
    /// `material_coverage` is likewise pre-resolved by the route layer
    /// (inventory access needs a `ProductionRepository`, which
    /// `IndustryService` doesn't have -- same I/O-free-core reason as
    /// `component_eivs`), keyed by type_id for *every* material on the
    /// worksheet, not just `Missing`-scoped ones -- filtered down to just
    /// the `Missing`-scoped rows below before it ever reaches
    /// `ComponentExpansionService::expand` or `apply_component_expansion`,
    /// which both only ever act on what they're told to.
    pub async fn preview_plan(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
        component_eivs: &BTreeMap<i64, Money>,
        material_coverage: &BTreeMap<i64, MaterialCoverageSummary>,
        market_evidence: Option<&GraphMarketEvidence>,
    ) -> Result<BuildPlanRevision, IndustryError> {
        self.preview_plan_inner(
            workspace_id,
            owner_id,
            command,
            component_eivs,
            material_coverage,
            market_evidence,
        )
        .await
    }

    /// `preview_plan`'s implementation, also called directly by the canonical
    /// planner to preview each producer at its sized runs.
    pub(in crate::industry) async fn preview_plan_inner(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
        component_eivs: &BTreeMap<i64, Money>,
        material_coverage: &BTreeMap<i64, MaterialCoverageSummary>,
        market_evidence: Option<&GraphMarketEvidence>,
    ) -> Result<BuildPlanRevision, IndustryError> {
        self.counters
            .previews
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        validate_runs(command.runs)?;
        let recipe = self.capture_recipe(&command.recipe).await?;
        // The manual price list is only an optional fallback for items with
        // no market coverage -- market scope pricing is always primary, so
        // an unconfigured list is not an error.
        let manual_price_list = match command.manual_price_list_id {
            Some(id) => {
                let source = self.repository.get_price_source(workspace_id, id).await?;
                if Some(source.revision) != command.expected_manual_price_list_revision {
                    return Err(IndustryError::RevisionConflict);
                }
                Some(source)
            }
            None => None,
        };
        let now = Utc::now();
        let build = Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Planning preview".to_string(),
            recipe,
            runs: command.runs,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: None,
            recipe_currency: RecipeCurrency::Current,
            active_sde_version: None,
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        };
        let mut blueprint = self
            .capture_blueprint_snapshot_for_preview(
                &build,
                command.blueprint_selection.as_ref(),
                command.manufacturing_facility.as_ref(),
            )
            .await?;
        let (blueprint_me, blueprint_te) = blueprint_efficiencies(blueprint.as_ref());
        let max_runs_per_job = blueprint
            .as_ref()
            .and_then(crate::BlueprintSnapshot::max_runs_per_job);
        let facility = self
            .facility_preview(
                workspace_id,
                &build.recipe,
                build.runs,
                blueprint_me,
                blueprint_te,
                max_runs_per_job,
                command.manufacturing_facility.as_ref(),
                command.reaction_facility.as_ref(),
            )
            .await?;
        // The root's own preview above only resolves the slot matching its
        // kind; a build-resolved sub-component of the *other* kind still
        // needs that slot's raw profile to compute its own installation
        // cost, so resolve it here if it wasn't already resolved as root's.
        let manufacturing_profile = match (&build.recipe, &facility) {
            (BuildRecipe::Manufacturing(_), Some(facility)) => Some(facility.profile.clone()),
            _ => match command.manufacturing_facility.as_ref() {
                Some(command) => Some(
                    self.resolve_facility_profile(workspace_id, command.facility_profile_id)
                        .await?,
                ),
                None => None,
            },
        };
        let reaction_profile = match (&build.recipe, &facility) {
            (BuildRecipe::Reaction(_), Some(facility)) => Some(facility.profile.clone()),
            _ => match command.reaction_facility.as_ref() {
                Some(command) => Some(
                    self.resolve_facility_profile(workspace_id, command.facility_profile_id)
                        .await?,
                ),
                None => None,
            },
        };
        // Per-row overrides, resolved once up front so `apply_component_expansion`
        // stays a pure lookup -- keyed by type_id, same shape as `component_eivs`.
        let mut component_facility_overrides: BTreeMap<i64, IndustryFacilityProfile> =
            BTreeMap::new();
        for resolution in &command.component_resolutions {
            if let Some(override_) = resolution.facility_override.as_ref() {
                let expected_role = match resolution.recipe {
                    RecipeSelection::Manufacturing { .. } => FacilityRole::Manufacturing,
                    RecipeSelection::Reaction { .. } => FacilityRole::Reaction,
                };
                let profile = self
                    .resolve_override_profile(workspace_id, override_, expected_role)
                    .await?;
                component_facility_overrides.insert(resolution.type_id, profile);
            }
        }
        // `material_coverage` is already scoped by the caller -- it holds
        // an entry for every type_id that should net against inventory
        // (`Missing`, the default), excluding any explicitly `Full`-scoped
        // ones. Same precedent as `component_eivs`: the route layer
        // resolves I/O-backed data once, `preview_plan` just consumes it.
        let available_quantities: BTreeMap<i64, u64> = material_coverage
            .iter()
            .map(|(type_id, summary)| (*type_id, summary.available_to_this_build))
            .collect();
        let effective_root_requirements = derive_effective_root_requirements(
            &build.recipe,
            build.runs,
            blueprint_me,
            blueprint_te,
            max_runs_per_job,
            facility.as_ref(),
        )?;
        let expansion = self
            .expand_component_tree(
                workspace_id,
                owner_id,
                command.recipe,
                build.runs,
                &command.component_resolutions,
                ComponentExpansionQuantities {
                    available: &available_quantities,
                    effective_root_requirements: &effective_root_requirements,
                },
            )
            .await?;
        let pricing_selections = crate::normalize_pricing_selections(command.pricing_selections)?;
        let (price_inputs, item_pricing_policies) =
            resolve_planner_pricing(&build, pricing_selections.clone())?;
        let overrides = self.parse_price_items(price_inputs).await?;
        let material_pricing_policy = command.material_pricing_policy;
        let output_pricing_policy = command.output_pricing_policy;
        let (material_requests, output_requests) = market_price_requests(
            &build,
            facility.as_ref(),
            material_pricing_policy,
            output_pricing_policy,
            &item_pricing_policies,
            expansion.as_ref(),
        )?;
        // One coherent market-evidence snapshot per scope when a multi-node
        // valuation supplied one (Build Graph): every node on this scope
        // then prices from the same batch, whatever refreshes land while the
        // projection runs. `None` => a live read.
        let material_evidence =
            market_evidence.and_then(|evidence| evidence.get(command.material_scope));
        let output_evidence =
            market_evidence.and_then(|evidence| evidence.get(command.output_scope));
        let mut market_items = self
            .repository
            .derive_market_price_items(
                workspace_id,
                command.material_scope,
                material_requests.clone(),
                material_evidence,
            )
            .await?;
        market_items.extend(
            self.repository
                .derive_market_price_items(
                    workspace_id,
                    command.output_scope,
                    output_requests.clone(),
                    output_evidence,
                )
                .await?,
        );
        if let Some(manual_source) = manual_price_list.as_ref() {
            let resolved: BTreeSet<i64> = market_items.iter().map(|item| item.type_id).collect();
            let manual_items: BTreeMap<i64, &PriceSourceItem> = manual_source
                .items
                .iter()
                .map(|item| (item.type_id, item))
                .collect();
            for request in material_requests.iter().chain(output_requests.iter()) {
                if resolved.contains(&request.type_id) {
                    continue;
                }
                if let Some(manual_item) = manual_items.get(&request.type_id) {
                    market_items.push(PriceSourceItem {
                        type_id: request.type_id,
                        type_name: request.type_name.clone(),
                        price: manual_item.price,
                        note: format!("manual price list fallback: {}", manual_item.note),
                        updated_at: manual_item.updated_at,
                    });
                }
            }
        }
        let pricing_policies = build_pricing_policies(
            &build,
            material_pricing_policy,
            output_pricing_policy,
            &item_pricing_policies,
        );
        // A produced row is priced by the plan's cost projection, which the
        // caller overlays (`BuildCostProjection::apply_to_revision`); this
        // pass leaves it unpriced.
        let linked_build_totals: BTreeMap<i64, Money> = BTreeMap::new();
        let (effective_requirements, planned_duration_seconds) = if let Some(facility) = &facility {
            (
                facility.requirements.clone(),
                facility.planned_duration_seconds,
            )
        } else {
            preview_recipe_effects(
                &build.recipe,
                build.runs,
                blueprint_me,
                blueprint_te,
                max_runs_per_job,
            )?
        };
        if let Some(blueprint) = blueprint.as_mut() {
            blueprint.planned_duration_seconds = planned_duration_seconds;
        }
        let mut calculated = calculate_transient_plan(TransientCalculationInput {
            build: &build,
            material_scope: command.material_scope,
            output_scope: command.output_scope,
            market_items: &market_items,
            manual_price_list: manual_price_list.as_ref(),
            overrides,
            pricing_policies: &pricing_policies,
            effective_requirements: &effective_requirements,
            root_facility: facility,
            blueprint: blueprint.map(Into::into),
            expansion: expansion.as_ref(),
            manufacturing_profile: manufacturing_profile.as_ref(),
            reaction_profile: reaction_profile.as_ref(),
            component_facility_overrides: &component_facility_overrides,
            component_eivs,
            linked_build_material_costs: &linked_build_totals,
            material_coverage,
        })?;
        apply_pricing_provenance(&mut calculated.price_lines, &pricing_selections)?;
        calculated.into_plan_revision(&build, 1, Utc::now())
    }

    /// Rebuild a `PreviewBuildPlanCommand` from `build.draft_planning`,
    /// live against the manual price list's current revision. `Ok(None)`
    /// when the build has no planning input yet (a soft state for the
    /// linked-build fold; `calculate_build_snapshot_with_coverage` turns it into an
    /// error for its own callers).
    pub async fn reconstruct_preview_command(
        &self,
        workspace_id: WorkspaceId,
        build: &Build,
    ) -> Result<Option<PreviewBuildPlanCommand>, IndustryError> {
        let Some(draft) = build.draft_planning.as_ref() else {
            return Ok(None);
        };
        let expected_manual_price_list_revision = match draft.input.manual_price_list_id {
            Some(id) => Some(
                self.repository
                    .get_price_source(workspace_id, id)
                    .await?
                    .revision,
            ),
            None => None,
        };
        Ok(Some(PreviewBuildPlanCommand {
            recipe: recipe_selection_of(&build.recipe),
            runs: build.runs,
            material_scope: draft.input.material_scope,
            output_scope: draft.input.output_scope,
            manual_price_list_id: draft.input.manual_price_list_id,
            expected_manual_price_list_revision,
            material_pricing_policy: draft.input.material_pricing_policy,
            output_pricing_policy: draft.input.output_pricing_policy,
            pricing_selections: draft.input.pricing_selections.clone(),
            manufacturing_facility: draft.input.manufacturing_facility.clone(),
            reaction_facility: draft.input.reaction_facility.clone(),
            blueprint_selection: draft.input.blueprint_selection.clone(),
            component_resolutions: draft.input.component_resolutions.clone(),
            fulfillment_scopes: draft.input.fulfillment_scopes.clone(),
            build_id: Some(build.id),
            // The graph fold passes evidence through the fn args, not the
            // reconstructed command; a stand-alone reconstruct carries none.
            market_evidence: Vec::new(),
        }))
    }

    // -- Build materials projection -----------------------------------------

    /// Resolves each component resolution's blueprint efficiency (if any)
    /// and runs the demand-cascade expansion, in one place -- shared by
    /// `preview_plan`, the linked-build lookups below, and `build_tree`'s
    /// walker, which all need the same resolved component tree.
    pub(in crate::industry) async fn expand_component_tree(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        recipe: RecipeSelection,
        runs: u64,
        component_resolutions: &[crate::ComponentResolution],
        quantities: ComponentExpansionQuantities<'_>,
    ) -> Result<Option<crate::ComponentExpansion>, IndustryError> {
        // A `Missing`-scoped row needs an expansion computed even when
        // nothing at all is Build-resolved (a plain "Buy Missing" row on
        // an otherwise fully-Buy build) -- `available_quantities` is
        // already filtered to `Missing`-scoped type_ids by the caller, so
        // non-empty here means there's real blending work for
        // `apply_component_expansion` to do, even with no resolutions.
        if component_resolutions.is_empty() && quantities.available.is_empty() {
            return Ok(None);
        }
        let mut component_blueprint_efficiencies: BTreeMap<i64, (u8, u8)> = BTreeMap::new();
        for resolution in component_resolutions {
            if let Some(selection) = resolution.blueprint_selection.as_ref() {
                let blueprint_type_id = match resolution.recipe {
                    RecipeSelection::Manufacturing { blueprint_type_id } => blueprint_type_id,
                    RecipeSelection::Reaction { .. } => {
                        return Err(IndustryError::Validation(
                            "Reaction sub-components do not have blueprint assumptions."
                                .to_string(),
                        ));
                    }
                };
                let efficiency = self
                    .resolve_component_blueprint_efficiency(
                        workspace_id,
                        owner_id,
                        blueprint_type_id,
                        selection,
                    )
                    .await?;
                component_blueprint_efficiencies.insert(resolution.type_id, efficiency);
            }
        }
        Ok(Some(
            crate::ComponentExpansionService::new(self.sde_repository.clone())
                .expand_with_root_requirements(
                    recipe,
                    runs,
                    component_resolutions,
                    &component_blueprint_efficiencies,
                    quantities.available,
                    quantities.effective_root_requirements,
                )
                .await
                .map_err(|error| IndustryError::Validation(error.to_string()))?,
        ))
    }
}
