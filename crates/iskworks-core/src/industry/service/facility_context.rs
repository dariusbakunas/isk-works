use super::*;

impl IndustryService {
    /// Resolves the current facility profile by id, for both the root's own
    /// facility resolution and resolving a sub-component's facility profile
    /// (which needs the profile itself but not a full `FacilityPlanPreview`
    /// -- there is no root-job-shaped recipe to preview it against).
    ///
    /// A live Build references its facility purely by id and always
    /// calculates against the profile's *current* settings -- there is no
    /// recorded revision to consult. Revision-based optimistic concurrency
    /// still guards edits to the `FacilityProfile` itself (its PATCH /
    /// archive / delete in `iskworks-storage`), which is why
    /// `FacilityError::RevisionConflict` remains a variant.
    pub(super) async fn resolve_facility_profile(
        &self,
        workspace_id: WorkspaceId,
        facility_profile_id: FacilityProfileId,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        self.repository
            .get_facility_profile(workspace_id, facility_profile_id)
            .await
    }

    /// Resolves and validates a per-sub-component facility override: the
    /// current profile by id, plus role-match and not-archived checks that
    /// the shared build-level slots already get from
    /// `preview_facility`/`preview_reaction_facility` but this standalone
    /// lookup doesn't inherit for free. `pub` (not just used internally by
    /// `preview_plan`) so the route layer can pre-validate an override
    /// before deciding whether it's worth an EIV fetch -- an override
    /// that's about to fail this check shouldn't trigger an external ESI
    /// round trip first, which would otherwise mask the real validation
    /// error behind an unrelated ESI failure.
    pub async fn resolve_override_profile(
        &self,
        workspace_id: WorkspaceId,
        override_: &crate::ComponentFacilityOverride,
        expected_role: FacilityRole,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        self.resolve_facility_context(workspace_id, override_.facility_profile_id, expected_role)
            .await
    }

    pub async fn resolve_facility_context(
        &self,
        workspace_id: WorkspaceId,
        facility_profile_id: FacilityProfileId,
        expected_role: FacilityRole,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        let profile = self
            .resolve_facility_profile(workspace_id, facility_profile_id)
            .await?;
        if profile.archived_at.is_some() {
            return Err(IndustryError::Facility(FacilityError::Archived));
        }
        if profile.role != expected_role {
            return Err(IndustryError::Facility(FacilityError::Validation(
                "This facility profile's role does not match the requested recipe kind."
                    .to_string(),
            )));
        }
        Ok(profile)
    }

    /// Resolves a per-sub-component blueprint selection (manual entry, or
    /// an owned/observed blueprint asset) into a plain `(material_efficiency,
    /// time_efficiency)` pair -- manufacturing-only, callers reject a
    /// selection on a reaction resolution before ever calling this.
    ///
    /// For `ObservedAsset`, kind/ME/TE are the durable effective planning
    /// configuration captured once at selection time (see the invariant doc
    /// on `BlueprintSelection::ObservedAsset`) -- read directly, with zero
    /// I/O and no live dependency on `observation_id` remaining resolvable.
    /// Only a not-yet-captured selection (the sentinel `kind ==
    /// BlueprintKind::Unknown` -- a fresh client pick, or an unmigrated
    /// legacy row) still resolves live,
    /// with `requested_runs: 1` passed to `blueprint::validate` as a
    /// placeholder (a sub-component's own `runs` isn't known until deep
    /// inside `ComponentExpansionService::expand`'s demand-cascade loop,
    /// well after this resolution has to happen) -- licensed-runs
    /// sufficiency is never enforced here, deliberately: a BPC's licensed
    /// -run count is execution-readiness evidence, never a planning
    /// -validity gate.
    ///
    /// `pub` for the same reason `resolve_override_profile` is: the route
    /// layer's own EIV preflight expansion needs the same resolved map this
    /// method feeds, to keep its `runs` figures consistent with the final
    /// preview (a sub-component's ME-reduced upstream demand cascades into
    /// downstream `runs`, unlike a facility override, which never affects
    /// `expand`'s own math).
    pub async fn resolve_component_blueprint_efficiency(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        blueprint_type_id: i64,
        selection: &crate::BlueprintSelection,
    ) -> Result<(u8, u8), IndustryError> {
        match selection {
            crate::BlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                ..
            } => {
                crate::blueprint::validate(
                    *kind,
                    *material_efficiency,
                    *time_efficiency,
                    *licensed_runs,
                    1,
                    true,
                )?;
                Ok((*material_efficiency, *time_efficiency))
            }
            crate::BlueprintSelection::ObservedAsset {
                kind,
                material_efficiency,
                time_efficiency,
                ..
            } if *kind != crate::BlueprintKind::Unknown => {
                Ok((*material_efficiency, *time_efficiency))
            }
            crate::BlueprintSelection::ObservedAsset { observation_id, .. } => {
                let observation = self
                    .repository
                    .get_blueprint_observation(workspace_id, *observation_id)
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
                Ok((observation.material_efficiency, observation.time_efficiency))
            }
        }
    }

    /// Resolves the root job's own facility preview from whichever of the
    /// two slots matches `recipe`'s kind. The other slot may still hold a
    /// selection (e.g. for a build-resolved sub-component of the other
    /// kind), but that is not the root job, so it is simply not previewed
    /// here.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn facility_preview(
        &self,
        workspace_id: WorkspaceId,
        recipe: &BuildRecipe,
        runs: u64,
        blueprint_me: u8,
        blueprint_te: u8,
        max_runs_per_job: Option<u64>,
        manufacturing: Option<&FacilityPreviewCommand>,
        reaction: Option<&ReactionFacilityPreviewCommand>,
    ) -> Result<Option<FacilityPlanPreview>, IndustryError> {
        Ok(Some(match recipe {
            BuildRecipe::Manufacturing(recipe) => {
                let Some(command) = manufacturing else {
                    return Ok(None);
                };
                let profile = self
                    .resolve_facility_profile(workspace_id, command.facility_profile_id)
                    .await?;
                let eiv = command
                    .estimated_item_value
                    .as_deref()
                    .map(Money::parse)
                    .transpose()?;
                let product = self
                    .resolve_product_classification(recipe.primary_product().type_id)
                    .await?;
                preview_facility(
                    recipe,
                    runs,
                    profile,
                    product,
                    blueprint_me,
                    blueprint_te,
                    eiv,
                    max_runs_per_job,
                )?
            }
            BuildRecipe::Reaction(formula) => {
                let Some(command) = reaction else {
                    return Ok(None);
                };
                let profile = self
                    .resolve_facility_profile(workspace_id, command.facility_profile_id)
                    .await?;
                let eiv = command
                    .estimated_item_value
                    .as_deref()
                    .map(Money::parse)
                    .transpose()?;
                let product = self
                    .resolve_product_classification(formula.primary_product().type_id)
                    .await?;
                crate::preview_reaction_facility(formula, runs, profile, product, eiv)?.into()
            }
        }))
    }

    /// The job product's SDE `(category_id, group_id)` -- the only input a
    /// facility rig's target filter is matched against. An unknown product
    /// (no active SDE, or absent from it) yields the default, under which a
    /// `Restricted` rig contributes nothing.
    async fn resolve_product_classification(
        &self,
        product_type_id: i64,
    ) -> Result<crate::ProductClassification, IndustryError> {
        let classifications = self
            .sde_repository
            .type_classifications(&[product_type_id])
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?;
        Ok(classifications
            .get(&product_type_id)
            .map(|classification| crate::ProductClassification {
                category_id: classification.category_id,
                group_id: classification.group_id,
            })
            .unwrap_or_default())
    }
}
