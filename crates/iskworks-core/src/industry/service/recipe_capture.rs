use super::*;

impl IndustryService {
    /// `Ok(None)` for a reaction Build with no blueprint assumptions
    /// supplied -- reaction formulas have no blueprint at all in EVE, so
    /// there is nothing to snapshot. A reaction Build with a
    /// `selection` present is a caller error, not a silent no-op.
    pub(super) async fn capture_blueprint_snapshot_for_preview(
        &self,
        build: &Build,
        selection: Option<&crate::BlueprintSelection>,
        legacy_facility: Option<&FacilityPreviewCommand>,
    ) -> Result<Option<crate::BlueprintSnapshot>, IndustryError> {
        let recipe = match &build.recipe {
            BuildRecipe::Manufacturing(recipe) => recipe,
            BuildRecipe::Reaction(_) => {
                return if selection.is_some() {
                    Err(IndustryError::Validation(
                        "Reaction Builds do not have blueprint assumptions.".to_string(),
                    ))
                } else {
                    Ok(None)
                };
            }
        };
        let now = Utc::now();
        let snapshot = match selection {
            // `requested_runs: 1` -- never `build.runs` -- for the same
            // reason `resolve_component_blueprint_efficiency` already uses
            // it: a manual `licensed_runs` assumption is planning-time
            // *evidence*, not a gate the walker's dynamically-resized run
            // count can trip. `requested_runs` is set to the real run count
            // on the struct just below, purely for display/history.
            Some(crate::BlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                notes,
            }) => {
                let mut snapshot = crate::capture_manual_snapshot(
                    build.id,
                    recipe.blueprint_type_id,
                    &recipe.blueprint_name,
                    1,
                    *kind,
                    *material_efficiency,
                    *time_efficiency,
                    *licensed_runs,
                    notes,
                    now,
                )?;
                snapshot.requested_runs = build.runs;
                snapshot
            }
            // Already captured (see the invariant doc on
            // `BlueprintSelection::ObservedAsset`): kind/ME/TE are frozen
            // planning configuration, read directly with zero I/O. A
            // consumed/moved/desynced BPC must never block quantity/cost
            // projection, so the observation is only ever fetched here
            // best-effort, purely to refresh *display* provenance
            // (owner/location/eve-item) -- any failure at all (not found,
            // owner/type mismatch, repository error) just leaves that
            // provenance `None` rather than failing this preview.
            Some(crate::BlueprintSelection::ObservedAsset {
                observation_id,
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
            }) if *kind != crate::BlueprintKind::Unknown => {
                let provenance = self
                    .repository
                    .get_blueprint_observation(build.workspace_id, *observation_id)
                    .await
                    .ok()
                    .filter(|observation| {
                        observation.owner_id == build.owner_id
                            && observation.blueprint_type_id == recipe.blueprint_type_id
                    });
                crate::BlueprintSnapshot {
                    id: Uuid::new_v4(),
                    build_id: build.id,
                    source_mode: crate::BlueprintSourceMode::ObservedAsset,
                    blueprint_type_id: recipe.blueprint_type_id,
                    blueprint_name: provenance
                        .as_ref()
                        .map(|observation| observation.blueprint_name.clone())
                        .unwrap_or_else(|| recipe.blueprint_name.clone()),
                    kind: *kind,
                    material_efficiency: *material_efficiency,
                    time_efficiency: *time_efficiency,
                    // Frozen, like ME/TE: it sets the per-job run limit
                    // (`BlueprintSnapshot::max_runs_per_job`), so it must
                    // never follow the live asset.
                    licensed_runs: *licensed_runs,
                    requested_runs: build.runs,
                    source_observation_id: Some(*observation_id),
                    source_eve_item_id: provenance.as_ref().map(|o| o.eve_item_id),
                    source_owner_id: provenance.as_ref().map(|o| o.owner_id),
                    source_owner_name: provenance.as_ref().map(|o| o.owner_name.clone()),
                    source_location_id: provenance.as_ref().map(|o| o.location_id),
                    source_location_name: provenance.as_ref().and_then(|o| o.location_name.clone()),
                    observed_at: provenance.as_ref().map(|o| o.observed_at),
                    imported_at: provenance.as_ref().map(|o| o.imported_at),
                    manual_notes: None,
                    planned_duration_seconds: None,
                    formula_version: "blueprint-snapshot-v2".into(),
                    captured_at: now,
                }
            }
            // Not yet captured -- a fresh, unsaved pick (the client named
            // only `observation_id`) or an unmigrated legacy row (see
            // `IndustryService::backfill_observed_blueprint_configurations`).
            // This is the one remaining boundary where a live lookup is
            // load-bearing for *this* preview: resolve once, right now,
            // without persisting anything. `requested_runs: 1`, same
            // reasoning as the Manual arm above -- a first-time capture is
            // never gated on licensed runs vs. the current run count.
            //
            // Deliberately no graceful fallback here: with no frozen
            // config to fall back on, fabricating ME0/TE0/kind-Unknown as
            // if it were real would misrepresent this Build as
            // mathematically valid when it plainly isn't (the sourcing
            // -model investigation's explicit "never fabricate" invariant)
            // -- every failure propagates, exactly as selecting this
            // observation for the first time would fail.
            Some(crate::BlueprintSelection::ObservedAsset { observation_id, .. }) => {
                let observation = self
                    .repository
                    .get_blueprint_observation(build.workspace_id, *observation_id)
                    .await?;
                let mut snapshot = crate::capture_observed_snapshot(
                    build.id,
                    build.owner_id,
                    recipe.blueprint_type_id,
                    1,
                    &observation,
                    now,
                )?;
                snapshot.requested_runs = build.runs;
                snapshot
            }
            None => {
                let (me, te) = legacy_facility
                    .map_or((0, 0), |value| (value.blueprint_me, value.blueprint_te));
                crate::BlueprintSnapshot {
                    id: Uuid::new_v4(),
                    build_id: build.id,
                    source_mode: crate::BlueprintSourceMode::LegacyMigration,
                    blueprint_type_id: recipe.blueprint_type_id,
                    blueprint_name: recipe.blueprint_name.clone(),
                    kind: crate::BlueprintKind::Unknown,
                    material_efficiency: me,
                    time_efficiency: te,
                    licensed_runs: None,
                    requested_runs: build.runs,
                    source_observation_id: None,
                    source_eve_item_id: None,
                    source_owner_id: None,
                    source_owner_name: None,
                    source_location_id: None,
                    source_location_name: None,
                    observed_at: None,
                    imported_at: None,
                    manual_notes: None,
                    planned_duration_seconds: None,
                    formula_version: "legacy-blueprint-snapshot-v1".into(),
                    captured_at: now,
                }
            }
        };
        Ok(Some(snapshot))
    }

    pub async fn capture_recipe(
        &self,
        selection: &RecipeSelection,
    ) -> Result<BuildRecipe, IndustryError> {
        match selection {
            RecipeSelection::Manufacturing { blueprint_type_id } => Ok(BuildRecipe::Manufacturing(
                self.capture_active_recipe(*blueprint_type_id).await?,
            )),
            RecipeSelection::Reaction {
                reaction_formula_type_id,
            } => Ok(BuildRecipe::Reaction(
                self.capture_active_reaction_formula(*reaction_formula_type_id)
                    .await?,
            )),
        }
    }

    async fn capture_active_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<CapturedRecipe, IndustryError> {
        let active = self
            .sde_repository
            .active_sde()
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
            .ok_or(IndustryError::NoActiveSde)?;
        let recipe = self
            .sde_repository
            .manufacturing_recipe(blueprint_type_id)
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
            .ok_or(IndustryError::BlueprintNotFound)?;
        CapturedRecipe::capture(active.import_id, active.source_version, recipe)
    }

    pub async fn capture_active_reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<CapturedReactionFormula, IndustryError> {
        let active = self
            .sde_repository
            .active_sde()
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
            .ok_or(IndustryError::NoActiveSde)?;
        let formula = self
            .sde_repository
            .reaction_formula(reaction_formula_type_id)
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
            .ok_or(IndustryError::ReactionFormulaNotFound)?;
        CapturedReactionFormula::capture(active.import_id, active.source_version, formula)
    }
}
