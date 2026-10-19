use super::*;
use crate::facility_defaults::{
    default_candidates, target_filters_from_sde, FacilityDefault, TargetFilter,
};

/// A workspace's facility defaults plus the SDE's role-tagged target
/// filters, loaded once per write that may create Builds.
pub(in crate::industry) struct FacilityDefaultsContext {
    defaults: Vec<FacilityDefault>,
    filters: Vec<TargetFilter>,
}

impl IndustryService {
    /// Loads the defaults, skipping the SDE read when there are none.
    pub(in crate::industry) async fn load_facility_defaults(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<FacilityDefaultsContext, IndustryError> {
        let defaults = self.repository.facility_defaults(workspace_id).await?;
        let filters = if defaults
            .iter()
            .any(|default| default.target_filter_id.is_some())
        {
            target_filters_from_sde(
                self.sde_repository
                    .industry_target_filters_by_activity()
                    .await
                    .map_err(|error| IndustryError::StaticData(error.to_string()))?,
            )
        } else {
            Vec::new()
        };
        Ok(FacilityDefaultsContext { defaults, filters })
    }

    /// The workspace default facility for a new Build of `selection` making
    /// `product_type_id`: the most specific matching target's default, then
    /// the role's fallback. A default whose facility is gone, archived or of
    /// another role is skipped in favor of the next candidate.
    pub(in crate::industry) async fn default_facility(
        &self,
        workspace_id: WorkspaceId,
        context: &FacilityDefaultsContext,
        selection: RecipeSelection,
        product_type_id: i64,
    ) -> Result<Option<FacilityProfileId>, IndustryError> {
        let role = match selection {
            RecipeSelection::Manufacturing { .. } => FacilityRole::Manufacturing,
            RecipeSelection::Reaction { .. } => FacilityRole::Reaction,
        };
        if !context.defaults.iter().any(|default| default.role == role) {
            return Ok(None);
        }
        let product = self.resolve_product_classification(product_type_id).await?;
        for candidate in default_candidates(role, product, &context.filters, &context.defaults) {
            match self
                .resolve_facility_context(workspace_id, candidate.facility_profile_id, role)
                .await
            {
                Ok(_) => return Ok(Some(candidate.facility_profile_id)),
                Err(IndustryError::Facility(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// A new root Build created without a facility for its recipe kind gets
    /// the workspace default for its product (ME/TE 0, no EIV); a draft-less
    /// command gets a draft holding just that facility. An explicit facility
    /// -- e.g. the one an opportunity was priced against -- is never
    /// replaced.
    pub(in crate::industry) async fn with_default_root_facility(
        &self,
        workspace_id: WorkspaceId,
        selection: RecipeSelection,
        recipe: &BuildRecipe,
        draft: Option<DraftPlanningInput>,
    ) -> Result<Option<DraftPlanningInput>, IndustryError> {
        let has_facility = draft.as_ref().is_some_and(|input| match selection {
            RecipeSelection::Manufacturing { .. } => input.manufacturing_facility.is_some(),
            RecipeSelection::Reaction { .. } => input.reaction_facility.is_some(),
        });
        if has_facility {
            return Ok(draft);
        }
        let context = self.load_facility_defaults(workspace_id).await?;
        let product_type_id = recipe.primary_product().type_id;
        let Some(facility_profile_id) = self
            .default_facility(workspace_id, &context, selection, product_type_id)
            .await?
        else {
            return Ok(draft);
        };
        let Some(mut input) = draft else {
            let resolution = crate::ComponentResolution {
                type_id: product_type_id,
                recipe: selection,
                facility_override: None,
                blueprint_selection: None,
            };
            return Ok(super::prepopulated_draft_planning(
                &resolution,
                selection,
                None,
                Some(facility_profile_id),
            )
            .map(|snapshot| snapshot.input));
        };
        match selection {
            RecipeSelection::Manufacturing { .. } => {
                input.manufacturing_facility = Some(FacilityPreviewCommand {
                    facility_profile_id,
                    blueprint_me: 0,
                    blueprint_te: 0,
                    estimated_item_value: None,
                });
            }
            RecipeSelection::Reaction { .. } => {
                input.reaction_facility = Some(ReactionFacilityPreviewCommand {
                    facility_profile_id,
                    estimated_item_value: None,
                });
            }
        }
        Ok(Some(input))
    }
}
