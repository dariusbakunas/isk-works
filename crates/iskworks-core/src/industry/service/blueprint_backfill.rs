use super::*;

/// One legacy `ObservedAsset` selection
/// [`IndustryService::backfill_observed_blueprint_configurations`] could not
/// resolve -- its observation lookup failed (not found, owner/type
/// mismatch, or an invalid ME/TE/kind on the observation itself). The
/// selection is left exactly as it was: still the "not yet captured"
/// sentinel, never fabricated, never switched to Buy or Manual. `reason` is
/// the `IndustryError`'s display text -- safe to log, no ESI/user-sensitive
/// payload, just the classified failure kind.
#[derive(Debug, Clone)]
pub struct UnresolvedObservedBlueprint {
    pub build_id: BuildId,
    pub observation_id: Uuid,
    pub reason: String,
}

/// Per-Build scratch tally for
/// [`IndustryService::backfill_selection`], folded into the whole-workspace
/// [`ObservedBlueprintBackfillReport`] once that Build's draft is processed.
#[derive(Debug, Default)]
pub(super) struct BackfillTally {
    captured: usize,
    unresolved: Vec<UnresolvedObservedBlueprint>,
}

/// Outcome of [`IndustryService::backfill_observed_blueprint_configurations`].
#[derive(Debug, Default, Clone)]
pub struct ObservedBlueprintBackfillReport {
    pub builds_scanned: usize,
    pub builds_updated: usize,
    pub selections_captured: usize,
    pub selections_unresolved: Vec<UnresolvedObservedBlueprint>,
    /// Builds skipped this run because a concurrent edit moved their
    /// revision on -- safe, and expected to be picked up by the next run.
    pub revision_conflicts: Vec<BuildId>,
}
impl IndustryService {
    /// Resolves every not-yet-captured `ObservedAsset` selection in a
    /// draft-planning input -- the root's own `blueprint_selection` and
    /// every `component_resolutions[].blueprint_selection` -- into its
    /// durable effective kind/ME/TE, exactly once, before the draft is
    /// normalized and persisted (see the invariant doc on
    /// `BlueprintSelection::ObservedAsset`). An already-captured selection
    /// (`kind != Unknown`) is left untouched -- re-saving a draft whose
    /// blueprint choice didn't change must never re-trigger a live
    /// observation lookup, or the exact fragility kept out of preview
    /// would just reappear on save instead.
    pub(super) async fn capture_effective_draft_blueprints(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        recipe: &BuildRecipe,
        mut input: DraftPlanningInput,
    ) -> Result<DraftPlanningInput, IndustryError> {
        if let Some(selection) = input.blueprint_selection.take() {
            input.blueprint_selection = Some(match recipe.blueprint_type_id() {
                Some(blueprint_type_id) => {
                    self.capture_effective_blueprint_selection(
                        workspace_id,
                        owner_id,
                        blueprint_type_id,
                        selection,
                    )
                    .await?
                }
                // A Reaction root with a blueprint assumption is already an
                // error `capture_blueprint_snapshot_for_preview` rejects at
                // preview time -- leave it as-is and let that existing
                // check surface it, rather than duplicating it here.
                None => selection,
            });
        }
        let mut resolutions = Vec::with_capacity(input.component_resolutions.len());
        for mut resolution in input.component_resolutions {
            if let Some(selection) = resolution.blueprint_selection.take() {
                resolution.blueprint_selection = Some(match resolution.recipe {
                    RecipeSelection::Manufacturing { blueprint_type_id } => {
                        self.capture_effective_blueprint_selection(
                            workspace_id,
                            owner_id,
                            blueprint_type_id,
                            selection,
                        )
                        .await?
                    }
                    RecipeSelection::Reaction { .. } => selection,
                });
            }
            resolutions.push(resolution);
        }
        input.component_resolutions = resolutions;
        Ok(input)
    }

    /// Resolves an `ObservedAsset` selection's live observation into its
    /// durable effective planning configuration -- validating ownership,
    /// blueprint-type match, and ME/TE bounds via `blueprint::validate` --
    /// then freezes kind/ME/TE onto the returned selection. A `Manual`
    /// selection, or an `ObservedAsset` selection that's already captured
    /// (`kind != Unknown`), passes through untouched; this only ever
    /// resolves live for the not-yet-captured sentinel.
    ///
    /// `requested_runs: 1` is passed to `blueprint::validate`, deliberately:
    /// a BPC's licensed-run count is execution-readiness evidence, never a
    /// planning-validity gate -- selecting a blueprint must never fail just
    /// because its licensed runs happen to be fewer than this Build's
    /// current run count.
    pub(super) async fn capture_effective_blueprint_selection(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        blueprint_type_id: i64,
        selection: crate::BlueprintSelection,
    ) -> Result<crate::BlueprintSelection, IndustryError> {
        let observation_id = match &selection {
            crate::BlueprintSelection::ObservedAsset {
                observation_id,
                kind,
                ..
            } if *kind == crate::BlueprintKind::Unknown => *observation_id,
            _ => return Ok(selection),
        };
        let observation = self
            .repository
            .get_blueprint_observation(workspace_id, observation_id)
            .await?;
        if observation.owner_id != owner_id {
            return Err(IndustryError::Blueprint(
                crate::BlueprintError::ObservationOwnerMismatch,
            ));
        }
        if observation.blueprint_type_id != blueprint_type_id {
            return Err(IndustryError::Blueprint(
                crate::BlueprintError::ObservationTypeMismatch,
            ));
        }
        crate::blueprint::validate(
            observation.kind,
            observation.material_efficiency,
            observation.time_efficiency,
            observation.licensed_runs,
            1,
            false,
        )?;
        Ok(crate::BlueprintSelection::ObservedAsset {
            observation_id,
            kind: observation.kind,
            material_efficiency: observation.material_efficiency,
            time_efficiency: observation.time_efficiency,
            licensed_runs: observation.licensed_runs,
        })
    }

    /// One-time (idempotent, safe to rerun) maintenance pass over every
    /// Build in a workspace -- every root `list_builds` returns, **plus**
    /// every other Build of its plan (`load_root_plan`'s producers,
    /// including retained/inactive ones: a dormant Build->Buy'd child still
    /// deserves captured config in case it's switched back to Build later)
    /// -- capturing durable effective kind/ME/TE for any `ObservedAsset`
    /// selection still carrying the legacy "not yet captured" sentinel --
    /// persisted rows saved before this field existed, which only ever
    /// recorded `observation_id`. Never invoked from a request path; run it
    /// via the `backfill_observed_blueprints` binary against a workspace.
    ///
    /// For each such selection: resolve its observation, validate it
    /// exactly as a fresh selection would be, and persist the captured
    /// value. A selection whose observation can no longer be resolved is
    /// left completely untouched -- still the sentinel, recorded in
    /// [`ObservedBlueprintBackfillReport::selections_unresolved`] -- never
    /// fabricated, deleted, or silently switched to Buy/Manual (that Build
    /// still needs a human to re-select its blueprint; this pass only
    /// closes the *resolvable* backlog). A Build a concurrent edit moved on
    /// (`RevisionConflict`) is skipped for this run; rerunning the backfill
    /// is always safe and picks it up later.
    ///
    /// Idempotent by construction: a selection that's already captured
    /// (`kind != Unknown`) is never touched, so a Build with nothing left
    /// to migrate does zero repository writes on a repeat run.
    pub async fn backfill_observed_blueprint_configurations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<ObservedBlueprintBackfillReport, IndustryError> {
        let roots = self.repository.list_builds(workspace_id).await?;
        let mut report = ObservedBlueprintBackfillReport::default();
        for root in roots {
            let descendants = self
                .repository
                .load_root_plan(workspace_id, root.id)
                .await?
                .producers;
            report.builds_scanned += 1 + descendants.len();
            self.backfill_build(workspace_id, &root, &mut report)
                .await?;
            for descendant in &descendants {
                self.backfill_build(workspace_id, descendant, &mut report)
                    .await?;
            }
        }
        Ok(report)
    }

    /// One Build's contribution to
    /// [`Self::backfill_observed_blueprint_configurations`]: capture every
    /// not-yet-captured `ObservedAsset` selection on `build`'s own draft
    /// (its root `blueprint_selection` and every
    /// `component_resolutions[].blueprint_selection`), and persist iff at
    /// least one was actually captured.
    async fn backfill_build(
        &self,
        workspace_id: WorkspaceId,
        build: &Build,
        report: &mut ObservedBlueprintBackfillReport,
    ) -> Result<(), IndustryError> {
        let Some(draft) = build.draft_planning.as_ref() else {
            return Ok(());
        };
        let mut input = draft.input.clone();
        let mut tally = BackfillTally::default();

        if let Some(selection) = input.blueprint_selection.take() {
            let resolved = match build.recipe.blueprint_type_id() {
                Some(blueprint_type_id) => {
                    self.backfill_selection(
                        workspace_id,
                        build.owner_id,
                        build.id,
                        blueprint_type_id,
                        selection,
                        &mut tally,
                    )
                    .await
                }
                None => selection,
            };
            input.blueprint_selection = Some(resolved);
        }

        let mut resolutions = Vec::with_capacity(input.component_resolutions.len());
        for mut resolution in input.component_resolutions {
            if let Some(selection) = resolution.blueprint_selection.take() {
                let resolved = match resolution.recipe {
                    RecipeSelection::Manufacturing { blueprint_type_id } => {
                        self.backfill_selection(
                            workspace_id,
                            build.owner_id,
                            build.id,
                            blueprint_type_id,
                            selection,
                            &mut tally,
                        )
                        .await
                    }
                    RecipeSelection::Reaction { .. } => selection,
                };
                resolution.blueprint_selection = Some(resolved);
            }
            resolutions.push(resolution);
        }
        input.component_resolutions = resolutions;
        report.selections_unresolved.append(&mut tally.unresolved);

        if tally.captured == 0 {
            return Ok(());
        }

        match self
            .repository
            .update_draft(
                workspace_id,
                build.id,
                DraftUpdate {
                    expected_revision: build.revision,
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
        {
            Ok(_) => {
                report.builds_updated += 1;
                report.selections_captured += tally.captured;
                Ok(())
            }
            // Skipped for this run -- a concurrent edit moved the build
            // on. Not logged here: `iskworks-core` is pure domain logic
            // with no logging dependency; the caller (the
            // `backfill_observed_blueprints` CLI) reports this from the
            // returned `ObservedBlueprintBackfillReport`.
            Err(IndustryError::RevisionConflict) => {
                report.revision_conflicts.push(build.id);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// One backfill candidate: an already-captured selection (or `Manual`)
    /// passes through with no I/O and no report entry. A sentinel
    /// `ObservedAsset` selection is resolved via
    /// [`Self::capture_effective_blueprint_selection`]; success increments
    /// `tally.captured` and returns the captured selection, failure appends
    /// to `tally.unresolved` and returns the original, untouched selection.
    async fn backfill_selection(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        blueprint_type_id: i64,
        selection: crate::BlueprintSelection,
        tally: &mut BackfillTally,
    ) -> crate::BlueprintSelection {
        let observation_id = match &selection {
            crate::BlueprintSelection::ObservedAsset {
                observation_id,
                kind,
                ..
            } if *kind == crate::BlueprintKind::Unknown => *observation_id,
            crate::BlueprintSelection::ObservedAsset {
                kind: crate::BlueprintKind::Copy,
                licensed_runs: None,
                ..
            } => {
                return self
                    .backfill_licensed_runs(
                        workspace_id,
                        owner_id,
                        build_id,
                        blueprint_type_id,
                        selection,
                        tally,
                    )
                    .await;
            }
            _ => return selection,
        };
        match self
            .capture_effective_blueprint_selection(
                workspace_id,
                owner_id,
                blueprint_type_id,
                selection.clone(),
            )
            .await
        {
            Ok(resolved) => {
                tally.captured += 1;
                resolved
            }
            Err(error) => {
                tally.unresolved.push(UnresolvedObservedBlueprint {
                    build_id,
                    observation_id,
                    reason: error.to_string(),
                });
                selection
            }
        }
    }

    /// A copy captured before licensed runs were frozen on the selection:
    /// borrow them from its observation, but only while that observation
    /// still matches the frozen kind/ME/TE -- a re-researched or replaced
    /// copy's runs describe a different blueprint. Otherwise the selection
    /// is left untouched (planned as one job) and reported as unresolved.
    async fn backfill_licensed_runs(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        blueprint_type_id: i64,
        selection: crate::BlueprintSelection,
        tally: &mut BackfillTally,
    ) -> crate::BlueprintSelection {
        let crate::BlueprintSelection::ObservedAsset {
            observation_id,
            kind,
            material_efficiency,
            time_efficiency,
            ..
        } = selection
        else {
            return selection;
        };
        let unresolved = |reason: String| UnresolvedObservedBlueprint {
            build_id,
            observation_id,
            reason,
        };
        let observation = match self
            .repository
            .get_blueprint_observation(workspace_id, observation_id)
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                tally.unresolved.push(unresolved(error.to_string()));
                return selection;
            }
        };
        let matches = observation.owner_id == owner_id
            && observation.blueprint_type_id == blueprint_type_id
            && observation.kind == kind
            && observation.material_efficiency == material_efficiency
            && observation.time_efficiency == time_efficiency;
        match observation.licensed_runs.filter(|_| matches) {
            Some(licensed_runs) => {
                tally.captured += 1;
                crate::BlueprintSelection::ObservedAsset {
                    observation_id,
                    kind,
                    material_efficiency,
                    time_efficiency,
                    licensed_runs: Some(licensed_runs),
                }
            }
            None => {
                tally.unresolved.push(unresolved(
                    "observed copy no longer matches the captured configuration".into(),
                ));
                selection
            }
        }
    }
}
