use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBuildPlanCommand {
    pub recipe: RecipeSelection,
    pub runs: u64,
    #[serde(default = "crate::default_market_scope")]
    pub material_scope: crate::MarketScope,
    #[serde(default = "crate::default_market_scope")]
    pub output_scope: crate::MarketScope,
    #[serde(default)]
    pub manual_price_list_id: Option<PriceSourceId>,
    #[serde(default)]
    pub expected_manual_price_list_revision: Option<u64>,
    #[serde(default = "default_material_pricing_policy")]
    pub material_pricing_policy: crate::MarketPricingPolicy,
    #[serde(default = "default_output_pricing_policy")]
    pub output_pricing_policy: crate::MarketPricingPolicy,
    #[serde(default)]
    pub pricing_selections: Vec<ItemPricingSelectionInput>,
    #[serde(default)]
    pub manufacturing_facility: Option<FacilityPreviewCommand>,
    #[serde(default)]
    pub reaction_facility: Option<ReactionFacilityPreviewCommand>,
    #[serde(default)]
    pub blueprint_selection: Option<crate::BlueprintSelection>,
    #[serde(default)]
    pub component_resolutions: Vec<crate::ComponentResolution>,
    #[serde(default)]
    pub fulfillment_scopes: Vec<crate::FulfillmentScopeOverride>,
    /// The build this preview is for, when previewing an already-saved
    /// build's edits (omitted while creating a brand new build, which
    /// can't have linked children yet). Used to look up a Build-resolved
    /// row's own linked build for its real cost.
    #[serde(default)]
    pub build_id: Option<BuildId>,
    /// Optional market-evidence pin: when a client re-requests a
    /// candidate-preview of a linked Build to reconcile it against a Build
    /// Graph, it passes back the graph response's `marketEvidence` here so
    /// this preview prices from the exact same batch snapshot. Absent =>
    /// a live read.
    #[serde(default)]
    pub market_evidence: Vec<crate::MarketScopeEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DraftUpdate {
    pub expected_revision: u64,
    pub name: String,
    pub runs: u64,
    pub notes: String,
    pub replacement_recipe: Option<BuildRecipe>,
    pub draft_planning: Option<DraftPlanningSnapshot>,
}

/// One member of an atomic [`IndustryRepository::update_draft_planning_batch`]
/// call -- the already-normalized new `draft_planning.input` for exactly one
/// Build, plus the revision it must still be at. Unlike [`DraftUpdate`], this
/// never touches `name`/`runs`/`notes`/`recipe` -- the Stages descendant-
/// configuration mutation only ever writes planning-input fields (facility,
/// blueprint/formula selection), never the Build's own identity or recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftPlanningBatchUpdate {
    pub build_id: BuildId,
    pub expected_revision: u64,
    pub input: DraftPlanningInput,
}

#[async_trait]
pub trait IndustryRepository: Send + Sync {
    async fn list_builds(&self, workspace_id: WorkspaceId) -> Result<Vec<Build>, IndustryError>;
    async fn get_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<Build, IndustryError>;
    async fn create_build(&self, new_build: NewBuild) -> Result<Build, IndustryError>;
    /// Creates a new root Build that is already a canonical plan: its own
    /// plan root, with canonical planner authority. Fixtures that don't model planner state can rely on the
    /// default, which is a plain `create_build`.
    async fn create_root_build(&self, new_build: NewBuild) -> Result<Build, IndustryError> {
        self.create_build(new_build).await
    }
    /// The whole root plan -- the root, every
    /// Build whose `plan_root_build_id` is the root (reachable or not), and
    /// every persisted `production_dependencies` edge of the plan -- in a
    /// bounded number of queries, independent of the plan's Build count.
    /// Read-only; no planner consumes it yet. The default errors so test
    /// fakes that never need it are unaffected.
    async fn load_root_plan(
        &self,
        _workspace_id: WorkspaceId,
        _root_build_id: BuildId,
    ) -> Result<crate::production_dependency::RootPlanRecords, IndustryError> {
        Err(IndustryError::Persistence(
            "load_root_plan is not supported by this repository".to_string(),
        ))
    }
    /// The root of the plan `build_id` belongs to. `Ok(None)` only when the
    /// repository has no plan model (test fakes).
    async fn plan_root_of(
        &self,
        _workspace_id: WorkspaceId,
        _build_id: BuildId,
    ) -> Result<Option<BuildId>, IndustryError> {
        Ok(None)
    }
    /// Apply one canonical consumer write atomically -- see
    /// [`crate::canonical_planner::CanonicalConsumerWrite`]. Returns the
    /// updated consumer.
    async fn apply_canonical_consumer_write(
        &self,
        _workspace_id: WorkspaceId,
        _write: crate::canonical_planner::CanonicalConsumerWrite,
    ) -> Result<Build, IndustryError> {
        Err(IndustryError::Persistence(
            "canonical writes are not supported by this repository".to_string(),
        ))
    }
    async fn update_draft(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        update: DraftUpdate,
    ) -> Result<Build, IndustryError>;
    /// Atomically persist a `draft_planning.input` change across **every**
    /// listed Build, in one transaction: either every member's revision
    /// check passes and every write lands, or none of them do. This is
    /// `update_draft` generalized over a list instead of one Build -- see
    /// [`DraftPlanningBatchUpdate`]'s own doc comment for the narrower scope
    /// (planning input only, never name/runs/notes/recipe).
    ///
    /// Used by the Stages descendant-configuration mutation to apply one
    /// facility/blueprint edit to every canonical member Build a shared
    /// production operation represents, so the operation is never observed
    /// half-migrated. Default: unsupported (a repository that never needs
    /// this -- most test fakes -- need not implement it).
    async fn update_draft_planning_batch(
        &self,
        _workspace_id: WorkspaceId,
        _updates: Vec<DraftPlanningBatchUpdate>,
    ) -> Result<Vec<Build>, IndustryError> {
        Err(IndustryError::Persistence(
            "atomic multi-Build configuration updates are not supported by this repository"
                .to_string(),
        ))
    }
    async fn rename_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        name: String,
    ) -> Result<Build, IndustryError>;
    /// Delete a build and its linked-child planning subtree. Epics, board
    /// tickets and their historical inventory facts survive with nullable
    /// source provenance. `force` is retained for wire compatibility only
    /// and never broadens deletion scope.
    async fn delete_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        expected_revision: u64,
        force: bool,
    ) -> Result<(), IndustryError>;
    async fn list_price_sources(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PriceSource>, IndustryError>;
    async fn get_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError>;
    async fn create_price_source(&self, source: PriceSource) -> Result<PriceSource, IndustryError>;
    async fn update_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        command: UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError>;
    async fn upsert_price_items(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        expected_revision: u64,
        items: Vec<PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError>;
    async fn remove_price_item(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        expected_revision: u64,
    ) -> Result<PriceSource, IndustryError>;
    async fn delete_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        expected_revision: u64,
    ) -> Result<(), IndustryError>;
    /// Resolves market prices for `requests` against `scope` directly
    /// -- no `PriceSource` involved. Reads through the same
    /// scope-based order-book merge (ESI + import data) the Market Browser
    /// uses, generalized in Slices 6/7.
    ///
    /// When `evidence` is `Some`, every order-book read and the
    /// freshness/staleness clock are pinned to that resolved evidence rather
    /// than "latest, as of `Utc::now()`" -- so a multi-node valuation (the
    /// Build Graph) prices every node on one scope from one coherent
    /// snapshot even if a refresh completes mid-request. `None` reads
    /// live.
    async fn derive_market_price_items(
        &self,
        _workspace_id: WorkspaceId,
        _scope: crate::MarketScope,
        _requests: Vec<crate::MarketPriceRequest>,
        _evidence: Option<&crate::MarketScopeEvidence>,
    ) -> Result<Vec<PriceSourceItem>, IndustryError> {
        Err(IndustryError::Persistence(
            "Market pricing is unavailable.".to_string(),
        ))
    }

    /// Resolve the current market-evidence identity for `scope` -- the
    /// completed ESI observation batches and latest import batch pointed at
    /// right now, plus a frozen `as_of` clock. Called once per distinct
    /// scope at the start of a Build Graph request; the result is then
    /// passed back into every `derive_market_price_items` for that scope.
    ///
    /// Default: an un-pinned evidence carrying only `as_of` -- pricing then
    /// behaves exactly as a live read, so a repository that has no batch
    /// model still gives every node one shared clock.
    async fn resolve_market_evidence(
        &self,
        _workspace_id: WorkspaceId,
        scope: crate::MarketScope,
    ) -> Result<crate::MarketScopeEvidence, IndustryError> {
        Ok(crate::MarketScopeEvidence::unpinned(
            scope,
            chrono::Utc::now(),
        ))
    }
    async fn get_facility_profile(
        &self,
        _workspace_id: WorkspaceId,
        _facility_id: crate::FacilityProfileId,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        Err(IndustryError::Persistence(
            "Facility persistence is unavailable.".to_string(),
        ))
    }

    async fn get_blueprint_observation(
        &self,
        _workspace_id: WorkspaceId,
        _observation_id: Uuid,
    ) -> Result<crate::BlueprintObservation, IndustryError> {
        Err(IndustryError::Blueprint(
            crate::BlueprintError::ObservationNotFound,
        ))
    }

    async fn list_blueprint_observations(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        _blueprint_type_id: i64,
    ) -> Result<Vec<crate::BlueprintObservation>, IndustryError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Error)]
pub enum IndustryError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("money must be a non-negative decimal with at most four fractional digits")]
    InvalidMoney,
    #[error("money calculation exceeds the supported range")]
    MoneyOverflow,
    #[error("recipe is invalid")]
    InvalidRecipe,
    #[error("manufacturing blueprint was not found")]
    BlueprintNotFound,
    #[error("reaction formula was not found")]
    ReactionFormulaNotFound,
    #[error("no active SDE is available")]
    NoActiveSde,
    #[error("build was not found")]
    BuildNotFound,
    /// Deleting this Build would remove a producer that other Builds'
    /// active production dependencies still reference. Switch those
    /// consumers to Buy (or delete them) first.
    #[error("this build still produces for {} other build(s); switch them to Buy first", consumers.len())]
    ProducerInUse {
        producer: BuildId,
        consumers: Vec<BuildId>,
    },
    #[error(transparent)]
    CanonicalWrite(#[from] crate::canonical_planner::CanonicalWriteError),
    #[error("price source was not found")]
    PriceSourceNotFound,
    #[error("record changed since it was loaded")]
    RevisionConflict,
    #[error("static data lookup failed: {0}")]
    StaticData(String),
    #[error("persistence failed: {0}")]
    Persistence(String),
    #[error(transparent)]
    Market(#[from] crate::MarketError),
    #[error(transparent)]
    Facility(#[from] FacilityError),
    #[error(transparent)]
    Blueprint(#[from] crate::BlueprintError),
}
