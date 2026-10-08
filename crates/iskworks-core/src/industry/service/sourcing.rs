use super::*;

impl IndustryService {
    /// The producer Build that fulfils `component_type_id` for the consumer
    /// `parent_build_id`: the one its demand edge references, resolved
    /// (reused, or created exactly once) when the edge is Produce but has
    /// none yet. Never a per-consumer child.
    pub async fn create_or_reuse_linked_build(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        parent_build_id: BuildId,
        component_type_id: i64,
    ) -> Result<Build, IndustryError> {
        let parent = self
            .repository
            .get_build(workspace_id, parent_build_id)
            .await?;
        if parent.owner_id != owner_id {
            return Err(IndustryError::BuildNotFound);
        }
        let root = self
            .plan_root_of(workspace_id, parent_build_id)
            .await?
            .ok_or_else(not_in_production_plan)?;
        self.canonical_linked_producer(workspace_id, root, parent_build_id, component_type_id)
            .await
    }

    /// Set (add or replace) one component's sourcing resolution on an
    /// **arbitrary** Build -- the Build-ID-addressable mutation the Graph
    /// uses to switch a nested acquisition dependency BUY -> BUILD, always
    /// targeting the Build that *owns* that requirement (`parent_build_id`
    /// on the acquisition node), never the root. It reuses the exact
    /// persistence path the worksheet editor uses (`update_draft` +
    /// `normalize_component_resolutions`); the linked-Build lifecycle
    /// (create-or-reuse) is the caller's next, separate step.
    pub async fn set_component_resolution(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        component_type_id: i64,
        recipe: RecipeSelection,
        expected_revision: u64,
    ) -> Result<Build, IndustryError> {
        self.mutate_component_resolutions(
            workspace_id,
            build_id,
            expected_revision,
            |resolutions| {
                resolutions.retain(|resolution| resolution.type_id != component_type_id);
                resolutions.push(crate::ComponentResolution {
                    type_id: component_type_id,
                    recipe,
                    facility_override: None,
                    blueprint_selection: None,
                });
            },
        )
        .await
    }

    /// Clear a component's Build resolution on an arbitrary Build
    /// (BUILD -> BUY). The retained linked Build is **not** deleted -- it
    /// stays inactive and is reused if the component is switched back to
    /// BUILD (existing create-or-reuse lifecycle). No-op if the component
    /// wasn't Build-resolved.
    pub async fn clear_component_resolution(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        component_type_id: i64,
        expected_revision: u64,
    ) -> Result<Build, IndustryError> {
        self.mutate_component_resolutions(
            workspace_id,
            build_id,
            expected_revision,
            |resolutions| {
                resolutions.retain(|resolution| resolution.type_id != component_type_id);
            },
        )
        .await
    }

    async fn mutate_component_resolutions(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        expected_revision: u64,
        mutate: impl FnOnce(&mut Vec<crate::ComponentResolution>),
    ) -> Result<Build, IndustryError> {
        let build = self.repository.get_build(workspace_id, build_id).await?;
        if build.revision != expected_revision {
            return Err(IndustryError::RevisionConflict);
        }
        let mut input = build
            .draft_planning
            .as_ref()
            .map(|snapshot| snapshot.input.clone())
            .ok_or_else(|| {
                IndustryError::Validation("Build has no planning input yet.".to_string())
            })?;
        mutate(&mut input.component_resolutions);
        input.component_resolutions = crate::normalize_component_resolutions(std::mem::take(
            &mut input.component_resolutions,
        ))?;
        let update = DraftUpdate {
            expected_revision,
            name: build.name.clone(),
            runs: build.runs,
            notes: build.notes.clone(),
            replacement_recipe: None,
            draft_planning: Some(DraftPlanningSnapshot {
                input,
                updated_at: Utc::now(),
            }),
        };
        // Buy -> Build references (or creates once) the plan's producer and
        // Build -> Buy changes only this edge, in the same transaction as the
        // draft.
        let root = self
            .plan_root_of(workspace_id, build_id)
            .await?
            .ok_or_else(not_in_production_plan)?;
        self.write_canonical_consumer(workspace_id, root, build_id, update)
            .await
    }

    /// Replace an arbitrary Build's blueprint selection (its own ME/TE, kind,
    /// or observed-asset choice). `None` clears it back to the unresearched
    /// default. Build-ID-addressable -- the unified inspector edits a *linked*
    /// Build's blueprint in place without going through the worksheet editor
    /// or its `/builds/:linkedId` page.
    pub async fn set_build_blueprint_selection(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        patch: BuildBlueprintSettingsPatch,
    ) -> Result<Build, IndustryError> {
        // Capture a not-yet-captured `ObservedAsset` pick's effective
        // kind/ME/TE here, at the exact moment the user selects it (see the
        // invariant doc on `BlueprintSelection::ObservedAsset`) -- this is
        // the atomic Build-settings path the unified inspector actually
        // uses to pick an observed blueprint, so it must capture just as
        // `create_draft`/`update_draft` do, not merely persist
        // `observation_id` and defer resolution to every future preview.
        let blueprint_selection = match patch.blueprint_selection {
            Some(selection) => {
                let build = self.repository.get_build(workspace_id, build_id).await?;
                let captured = match build.recipe.blueprint_type_id() {
                    Some(blueprint_type_id) => {
                        self.capture_effective_blueprint_selection(
                            workspace_id,
                            build.owner_id,
                            blueprint_type_id,
                            selection,
                        )
                        .await?
                    }
                    None => selection,
                };
                Some(captured)
            }
            None => None,
        };
        self.mutate_draft_planning(
            workspace_id,
            build_id,
            patch.expected_revision,
            move |input, _| {
                input.blueprint_selection = blueprint_selection;
            },
        )
        .await
    }

    /// Set (or clear, with `facility_profile_id: None`) an arbitrary Build's
    /// own facility for its recipe kind -- the manufacturing slot for a
    /// manufacturing Build, the reaction slot for a reaction Build. The other
    /// kind's slot is never touched, and blueprint ME/TE already recorded on
    /// the manufacturing command are preserved (they are recomputed from the
    /// blueprint selection at preview time regardless).
    pub async fn set_build_facility(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        patch: BuildFacilitySettingsPatch,
    ) -> Result<Build, IndustryError> {
        self.mutate_draft_planning(
            workspace_id,
            build_id,
            patch.expected_revision,
            |input, build| {
                let is_reaction = matches!(build.recipe, BuildRecipe::Reaction(_));
                let Some(facility_profile_id) = patch.facility_profile_id else {
                    if is_reaction {
                        input.reaction_facility = None;
                    } else {
                        input.manufacturing_facility = None;
                    }
                    return;
                };
                if is_reaction {
                    let estimated_item_value = patch.estimated_item_value.clone().or_else(|| {
                        input
                            .reaction_facility
                            .as_ref()
                            .and_then(|command| command.estimated_item_value.clone())
                    });
                    input.reaction_facility = Some(ReactionFacilityPreviewCommand {
                        facility_profile_id,
                        estimated_item_value,
                    });
                } else {
                    let previous = input.manufacturing_facility.as_ref();
                    let estimated_item_value = patch.estimated_item_value.clone().or_else(|| {
                        previous.and_then(|command| command.estimated_item_value.clone())
                    });
                    input.manufacturing_facility = Some(FacilityPreviewCommand {
                        facility_profile_id,
                        blueprint_me: previous
                            .map(|command| command.blueprint_me)
                            .unwrap_or_default(),
                        blueprint_te: previous
                            .map(|command| command.blueprint_te)
                            .unwrap_or_default(),
                        estimated_item_value,
                    });
                }
            },
        )
        .await
    }

    /// Replace an arbitrary Build's pricing configuration -- material/output
    /// market scope and strategy, the optional manual-price-list fallback,
    /// and the manual-EIV toggle. Row-level `pricing_selections` are left
    /// untouched (those are edited per material through the item inspector).
    pub async fn set_build_pricing(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        patch: BuildPricingSettingsPatch,
    ) -> Result<Build, IndustryError> {
        self.mutate_draft_planning(
            workspace_id,
            build_id,
            patch.expected_revision,
            |input, _| {
                input.material_scope = patch.material_scope;
                input.output_scope = patch.output_scope;
                input.material_pricing_policy = patch.material_pricing_policy;
                input.output_pricing_policy = patch.output_pricing_policy;
                input.manual_price_list_id = patch.manual_price_list_id;
                input.expected_manual_price_list_revision =
                    patch.expected_manual_price_list_revision;
                input.facility_eiv_manual = patch.facility_eiv_manual;
            },
        )
        .await
    }

    /// Applies one [`DescendantProductionConfigurationRequest`] to `build`'s
    /// own current `draft_planning.input`, returning the normalized result
    /// -- **never persisted here**. The per-member step of the Stages atomic
    /// multi-Build descendant-configuration mutation
    /// (`crates/iskworks-app`'s `DescendantProductionConfigurationCoordinator`):
    /// the coordinator calls this once per canonical member Build a
    /// production operation represents, collects every resulting
    /// [`DraftPlanningBatchUpdate`], and persists all of them in one
    /// transaction via [`IndustryRepository::update_draft_planning_batch`].
    ///
    /// Deliberately mirrors `set_build_facility` / `set_build_blueprint_selection`'s
    /// own field-by-field semantics (same "which slot", same "preserve
    /// whatever ME/TE was already there", same effective-blueprint capture
    /// for a not-yet-captured `ObservedAsset` pick) rather than routing
    /// through `mutate_draft_planning`, since that helper's synchronous
    /// `mutate` closure can't await the blueprint-capture step and also
    /// persists immediately -- exactly what a batch caller must defer until
    /// every member has been prepared and revision-checked.
    pub async fn prepare_descendant_configuration_input(
        &self,
        workspace_id: WorkspaceId,
        build: &Build,
        request: &DescendantProductionConfigurationRequest,
    ) -> Result<DraftPlanningInput, IndustryError> {
        let mut input = build
            .draft_planning
            .as_ref()
            .map(|snapshot| snapshot.input.clone())
            .ok_or_else(|| {
                IndustryError::Validation("Build has no planning input yet.".to_string())
            })?;
        match request {
            DescendantProductionConfigurationRequest::Facility {
                facility_profile_id,
                estimated_item_value,
            } => {
                let is_reaction = matches!(build.recipe, BuildRecipe::Reaction(_));
                match facility_profile_id {
                    None => {
                        if is_reaction {
                            input.reaction_facility = None;
                        } else {
                            input.manufacturing_facility = None;
                        }
                    }
                    Some(facility_profile_id) => {
                        if is_reaction {
                            let estimated_item_value = estimated_item_value.clone().or_else(|| {
                                input
                                    .reaction_facility
                                    .as_ref()
                                    .and_then(|command| command.estimated_item_value.clone())
                            });
                            input.reaction_facility = Some(ReactionFacilityPreviewCommand {
                                facility_profile_id: *facility_profile_id,
                                estimated_item_value,
                            });
                        } else {
                            let previous = input.manufacturing_facility.as_ref();
                            let estimated_item_value = estimated_item_value.clone().or_else(|| {
                                previous.and_then(|command| command.estimated_item_value.clone())
                            });
                            input.manufacturing_facility = Some(FacilityPreviewCommand {
                                facility_profile_id: *facility_profile_id,
                                blueprint_me: previous
                                    .map(|command| command.blueprint_me)
                                    .unwrap_or_default(),
                                blueprint_te: previous
                                    .map(|command| command.blueprint_te)
                                    .unwrap_or_default(),
                                estimated_item_value,
                            });
                        }
                    }
                }
            }
            DescendantProductionConfigurationRequest::BlueprintSelection {
                blueprint_selection,
            } => {
                let blueprint_selection = match blueprint_selection.clone() {
                    Some(selection) => {
                        let captured = match build.recipe.blueprint_type_id() {
                            Some(blueprint_type_id) => {
                                self.capture_effective_blueprint_selection(
                                    workspace_id,
                                    build.owner_id,
                                    blueprint_type_id,
                                    selection,
                                )
                                .await?
                            }
                            None => selection,
                        };
                        Some(captured)
                    }
                    None => None,
                };
                input.blueprint_selection = blueprint_selection;
            }
        }
        crate::normalize_draft_planning(input, build.runs)
    }

    /// Read one Build, revision-check it, apply `mutate` to a clone of its
    /// `draft_planning.input`, re-run the same `normalize_draft_planning` the
    /// worksheet editor's save path uses, and persist through `update_draft`.
    /// The sibling of `mutate_component_resolutions`, generalised to the
    /// whole planning input for the "common settings" patches.
    async fn mutate_draft_planning(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        expected_revision: u64,
        mutate: impl FnOnce(&mut DraftPlanningInput, &Build),
    ) -> Result<Build, IndustryError> {
        let build = self.repository.get_build(workspace_id, build_id).await?;
        if build.revision != expected_revision {
            return Err(IndustryError::RevisionConflict);
        }
        let mut input = build
            .draft_planning
            .as_ref()
            .map(|snapshot| snapshot.input.clone())
            .ok_or_else(|| {
                IndustryError::Validation("Build has no planning input yet.".to_string())
            })?;
        mutate(&mut input, &build);
        let input = crate::normalize_draft_planning(input, build.runs)?;
        self.repository
            .update_draft(
                workspace_id,
                build_id,
                DraftUpdate {
                    expected_revision,
                    name: build.name.clone(),
                    runs: build.runs,
                    notes: build.notes.clone(),
                    replacement_recipe: None,
                    draft_planning: Some(DraftPlanningSnapshot {
                        input,
                        updated_at: Utc::now(),
                    }),
                },
            )
            .await
    }
}
