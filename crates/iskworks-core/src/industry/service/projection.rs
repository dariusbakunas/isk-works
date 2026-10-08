use super::*;

impl IndustryService {
    /// The whole-plan **planning inventory allocation**: what external inputs
    /// the projected Build's production plan still needs once `inventory` is
    /// allocated **once** per demand edge.
    ///
    /// Runs the canonical planner (`project_canonical_build_materials`, see
    /// `industry/canonical_projection.rs`), an **allocating traversal** of
    /// the persisted producer DAG: each demand edge allocates inventory
    /// first, and each producer is then sized once from its aggregate
    /// remaining demand at `ceil(remaining / output_per_run)` runs (a fresh
    /// authoritative `preview_plan_inner`). A demand edge fully covered by
    /// inventory adds no production demand.
    ///
    /// Read-only and coverage-free: no `ProductionRepository::coverage`, no
    /// EIVs, no persisted `Build` mutation, no linked-build creation. The
    /// caller seeds `inventory` from one coherent inventory observation.
    pub async fn project_build_materials(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: &PreviewBuildPlanCommand,
        inventory: &mut crate::build_materials::PlanningInventory,
        generated_at: DateTime<Utc>,
        capture_verification: bool,
    ) -> Result<BuildMaterialsProjection, IndustryError> {
        let start = self.counters.snapshot();
        let mut projection = Box::pin(self.project_build_materials_inner(
            workspace_id,
            owner_id,
            command,
            inventory,
            generated_at,
            capture_verification,
        ))
        .await?;
        projection.metrics = self.counters.snapshot().since(start);
        Ok(projection)
    }

    async fn project_build_materials_inner(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: &PreviewBuildPlanCommand,
        inventory: &mut crate::build_materials::PlanningInventory,
        _generated_at: DateTime<Utc>,
        capture_verification: bool,
    ) -> Result<BuildMaterialsProjection, IndustryError> {
        // One planner: every saved Build belongs to a canonical root plan.
        let projected = command.build_id.ok_or_else(|| {
            IndustryError::Validation("A graph requires a saved build.".to_string())
        })?;
        let Some(plan_root) = self
            .repository
            .plan_root_of(workspace_id, projected)
            .await?
        else {
            // A missing Build is reported as such (404), not as planless.
            self.repository.get_build(workspace_id, projected).await?;
            return Err(IndustryError::Validation(
                "This Build is not part of a production plan.".to_string(),
            ));
        };
        Box::pin(self.project_canonical_build_materials(
            workspace_id,
            owner_id,
            command,
            inventory,
            capture_verification,
            plan_root,
        ))
        .await
    }

    /// Reconstructs a `PreviewBuildPlanCommand` from `build.draft_planning`
    /// and calculates the resulting `BuildPlanRevision` snapshot, live
    /// against the price source's current revision. `component_eivs` is
    /// empty: ESI-derived EIVs aren't reachable from here without giving
    /// `IndustryService` an I/O dependency it deliberately doesn't have.
    ///
    /// Nets every `Missing`-scoped
    /// requirement against `material_coverage` -- the same plain-data
    /// contract `preview_plan` already uses (the app/route layer resolves it
    /// from `ProductionRepository::coverage`, keeping this crate I/O-free).
    /// `material_coverage` must already exclude explicitly `Full`-scoped
    /// rows (the caller does that from the Build's own `fulfillment_scopes`).
    ///
    /// Reading it is inventory-neutral: it feeds only quantity/cost blending
    /// in the returned `BuildPlanRevision`, never a reservation or event.
    /// `create_order` freezes the resulting per-line `reused_quantity` /
    /// `missing_quantity` / `reused_line_total` onto the Epic.
    pub async fn calculate_build_snapshot_with_coverage(
        &self,
        workspace_id: WorkspaceId,
        build: &Build,
        material_coverage: &BTreeMap<i64, MaterialCoverageSummary>,
    ) -> Result<BuildPlanRevision, IndustryError> {
        let command = self
            .reconstruct_preview_command(workspace_id, build)
            .await?
            .ok_or_else(|| {
                IndustryError::Validation("Build has no planning input yet.".to_string())
            })?;
        Box::pin(self.preview_plan(
            workspace_id,
            build.owner_id,
            command,
            &BTreeMap::new(),
            material_coverage,
            None,
        ))
        .await
    }
}
