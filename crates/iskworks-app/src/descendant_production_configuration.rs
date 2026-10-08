//! `DescendantProductionConfigurationCoordinator` -- the application
//! orchestration behind the Stages inspector's atomic, multi-Build
//! descendant-configuration edit.
//!
//! Stages shows the authoritative production plan (nodes/occurrences
//! from `crate::ExecutionPlanCoordinator`, which are themselves a pure
//! projection of `BuildMaterialsCoordinator`'s own `verification_operations`).
//! This coordinator lets the inspector *edit* one production operation's
//! facility or blueprint/formula selection in place, applying the identical
//! patch to every canonical member Build the operation currently represents
//! -- one shared Reaction operation pooling three consumer occurrences has
//! three real, persisted `Build` rows behind it, and a "change this
//! operation's facility" edit must move all three together, atomically, or
//! not at all.
//!
//! Never a second planner: like `ExecutionPlanCoordinator` /
//! `OrderPlanCoordinator`, this reuses `BuildMaterialsCoordinator::materials`
//! to re-derive the *current* plan (under the caller's live,
//! unsaved overlay) immediately before mutating, so a stale inspector can
//! never apply an edit to a membership set that no longer matches reality
//! (see [`update`](DescendantProductionConfigurationCoordinator::update)'s
//! own doc comment). The actual persistence is one atomic transaction via
//! [`iskworks_core::IndustryRepository::update_draft_planning_batch`] --
//! either every member Build's revision check passes and every write
//! lands, or none of them do.

use std::collections::HashSet;
use std::sync::Arc;

use iskworks_core::build_materials::MaterialActivity;
use iskworks_core::{
    BuildId, DescendantConfigurationMember, DescendantProductionConfigurationRequest,
    DraftPlanningBatchUpdate, FacilityRole, IndustryError, IndustryRepository, IndustryService,
    OwnerId, PreviewBuildPlanCommand, WorkspaceId,
};
use iskworks_sde::SdeReadRepository;

use crate::build_preview::BuildPlanningDeps;
use crate::{BuildMaterialsCoordinator, BuildMaterialsError};

/// Every error the descendant-configuration mutation can produce.
#[derive(Debug, thiserror::Error)]
pub enum DescendantProductionConfigurationError {
    #[error(transparent)]
    Materials(#[from] BuildMaterialsError),
    #[error(transparent)]
    Industry(#[from] IndustryError),
    #[error("at least one member Build must be specified")]
    NoMembers,
    #[error("the root Build's own configuration is edited from Worksheet, not Stages")]
    RootNotEditable,
    /// The requested member set no longer corresponds to exactly one
    /// current production operation under the live plan -- either the
    /// topology changed (a member split into its own operation, a new
    /// compatible sibling joined, the node moved), or the client supplied
    /// an arbitrary set of Build ids that was never one operation to begin
    /// with. See [`DescendantProductionConfigurationCoordinator::update`]'s
    /// own doc comment for exactly what is re-checked.
    #[error(
        "these production operations have changed since they were loaded -- refresh Stages and try again"
    )]
    StaleMembership,
}

/// Application orchestration for the Stages descendant-configuration
/// mutation. Constructed per request from `AppState` with exactly the
/// collaborator set `BuildMaterialsCoordinator` takes, plus its own
/// `industry_repository`/`sde_repository` copies for the facility-role
/// validation and per-member `IndustryService` calls (same convention
/// `OrderPlanCoordinator` already follows for its own extra `preview`
/// collaborator: each coordinator holds its own cheap, stateless copy
/// rather than reaching into a sibling coordinator's private fields).
pub struct DescendantProductionConfigurationCoordinator {
    materials: BuildMaterialsCoordinator,
    industry_repository: Arc<dyn IndustryRepository>,
    sde_repository: Arc<dyn SdeReadRepository>,
}

impl DescendantProductionConfigurationCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            industry_repository: deps.industry_repository.clone(),
            sde_repository: deps.sde_repository.clone(),
            materials: BuildMaterialsCoordinator::new(deps),
        }
    }

    fn industry_service(&self) -> IndustryService {
        IndustryService::new(
            self.industry_repository.clone(),
            self.sde_repository.clone(),
        )
    }

    /// Apply `request` to every Build in `members`, atomically, after
    /// re-validating that they still form exactly one current production
    /// operation under `command`'s live, unsaved overlay.
    ///
    /// `root_build_id` is authoritative (mirrors every other Stages/Graph/
    /// Materials entry point): it overwrites `command.build_id`. None of
    /// `members` may be the root itself -- Stages only ever edits
    /// *descendant* production configuration (see the module doc's own
    /// ownership split); root configuration stays owned by Worksheet.
    ///
    /// Staleness re-check: re-runs the exact same authoritative walk
    /// (`BuildMaterialsCoordinator::materials`, `capture_verification: true`)
    /// Stages/Graph/Materials already pay for, then:
    /// 1. Finds the current occurrence for any one requested member. If
    ///    none of the requested ids appear in the live plan at all, the
    ///    whole request is stale.
    /// 2. The *current* authoritative member set is that one occurrence's
    ///    producer Build.
    /// 3. The current member set must equal the requested set exactly (as
    ///    sets, order-independent) -- not a superset, not a subset. Any
    ///    mismatch (a member left the operation, a new compatible sibling
    ///    joined, the operation split) is reported as
    ///    [`DescendantProductionConfigurationError::StaleMembership`], and
    ///    the request is rejected before any Build is even read for
    ///    mutation, let alone written.
    ///
    /// Persistence is one atomic transaction
    /// (`IndustryRepository::update_draft_planning_batch`): every member's
    /// new `draft_planning.input` is prepared and revision-checked before
    /// the single transactional write, so a validation failure on any one
    /// member (a stale revision, an incompatible facility role) leaves
    /// every member's configuration untouched.
    pub async fn update(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        root_build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
        members: Vec<DescendantConfigurationMember>,
        request: DescendantProductionConfigurationRequest,
    ) -> Result<Vec<iskworks_core::Build>, DescendantProductionConfigurationError> {
        if members.is_empty() {
            return Err(DescendantProductionConfigurationError::NoMembers);
        }
        let requested_ids: HashSet<BuildId> =
            members.iter().map(|member| member.build_id).collect();
        if requested_ids.contains(&root_build_id) {
            return Err(DescendantProductionConfigurationError::RootNotEditable);
        }

        command.build_id = Some(root_build_id);
        let materials = self
            .materials
            .materials(workspace_id, owner_id, root_build_id, command, true)
            .await?;

        let anchor = materials
            .verification_operations
            .iter()
            .find(|operation| requested_ids.contains(&operation.build_id))
            .ok_or(DescendantProductionConfigurationError::StaleMembership)?;
        // One ProductionOperation is one producer Build.
        let current_ids: HashSet<BuildId> = std::iter::once(anchor.build_id).collect();
        if current_ids != requested_ids {
            return Err(DescendantProductionConfigurationError::StaleMembership);
        }

        // Facility-role validation, once for the whole operation.
        if let DescendantProductionConfigurationRequest::Facility {
            facility_profile_id: Some(facility_profile_id),
            ..
        } = &request
        {
            let expected_role = match anchor.activity {
                MaterialActivity::Reaction => FacilityRole::Reaction,
                MaterialActivity::Manufacturing => FacilityRole::Manufacturing,
            };
            self.industry_service()
                .resolve_facility_context(workspace_id, *facility_profile_id, expected_role)
                .await?;
        }

        let service = self.industry_service();
        let mut updates = Vec::with_capacity(members.len());
        for member in &members {
            let build = self
                .industry_repository
                .get_build(workspace_id, member.build_id)
                .await?;
            if build.revision != member.expected_revision {
                return Err(DescendantProductionConfigurationError::Industry(
                    IndustryError::RevisionConflict,
                ));
            }
            let input = service
                .prepare_descendant_configuration_input(workspace_id, &build, &request)
                .await?;
            updates.push(DraftPlanningBatchUpdate {
                build_id: member.build_id,
                expected_revision: member.expected_revision,
                input,
            });
        }

        Ok(self
            .industry_repository
            .update_draft_planning_batch(workspace_id, updates)
            .await?)
    }
}
