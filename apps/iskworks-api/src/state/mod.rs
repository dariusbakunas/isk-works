//! `AppState` — the dependency container every route receives through
//! `axum`'s `State` extractor — its `with_*` builder methods and its
//! service/repository accessors. Each accessor's "unavailable" fallback
//! error is part of the API's observable behavior (status, code and
//! message), so it must not drift.
//!
//! The dependencies are grouped by domain ([`IndustryDeps`], [`MarketDeps`], [`EsiDeps`], [`CharacterViewDeps`], [`AdminDeps`]), one
//! submodule each with its own builders and accessors.

mod admin;
mod character_views;
mod esi;
mod industry;
mod market;

use crate::error::ApiError;
use crate::readiness::ReadinessCheck;
use crate::unavailable_sde::UnavailableSdeRepository;
use admin::AdminDeps;
use character_views::CharacterViewDeps;
use esi::EsiDeps;
use industry::IndustryDeps;
use iskworks_app::EsiApplicationError;
use iskworks_core::{
    BuildPlanningService, ComponentExpansionService, ReactionPlanningService, WorkspaceRepository,
    WorkspaceService,
};
use iskworks_sde::SdeReadRepository;
use market::MarketDeps;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    repository: Arc<dyn WorkspaceRepository>,
    pub(crate) sde_repository: Arc<dyn SdeReadRepository>,
    pub(crate) web_app_origin: String,
    readiness_check: Option<Arc<dyn ReadinessCheck>>,
    pub(crate) industry: IndustryDeps,
    pub(crate) market: MarketDeps,
    pub(crate) esi: EsiDeps,
    pub(crate) character_views: CharacterViewDeps,
    pub(crate) admin: AdminDeps,
}

impl AppState {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self {
            repository,
            sde_repository: Arc::new(UnavailableSdeRepository),
            web_app_origin: "http://127.0.0.1:5173".to_string(),
            readiness_check: None,
            industry: IndustryDeps::default(),
            market: MarketDeps::default(),
            esi: EsiDeps::default(),
            character_views: CharacterViewDeps::default(),
            admin: AdminDeps::default(),
        }
    }

    /// The single allowed CORS origin — the web app's own origin, since
    /// login uses a cookie and `Access-Control-Allow-Origin: *` is rejected
    /// by browsers alongside credentialed requests anyway. Defaults to the
    /// local Vite dev server; `main.rs` overrides it from `WEB_APP_URL` in
    /// every real deployment.
    #[must_use]
    pub fn with_web_app_origin(mut self, origin: String) -> Self {
        self.web_app_origin = origin;
        self
    }

    /// The dependency `/api/ready` probes — the Postgres pool in `main.rs`.
    /// Unset, `/api/ready` reports ready unconditionally.
    #[must_use]
    pub fn with_readiness_check(mut self, check: Arc<dyn ReadinessCheck>) -> Self {
        self.readiness_check = Some(check);
        self
    }

    pub(crate) fn readiness_check(&self) -> Option<&Arc<dyn ReadinessCheck>> {
        self.readiness_check.as_ref()
    }

    #[must_use]
    pub fn with_sde_repository(mut self, repository: Arc<dyn SdeReadRepository>) -> Self {
        self.sde_repository = repository;
        self
    }

    pub(crate) fn workspace_service(&self) -> WorkspaceService<Arc<dyn WorkspaceRepository>> {
        WorkspaceService::new(self.repository.clone())
    }

    /// The workspace's own default `MarketScope` (`Workspace::default_market_scope`,
    /// falling back to `DEFAULT_MARKET_SCOPE`/Jita when unset) -- used
    /// wherever a caller needs "the workspace's market" with no more
    /// specific selection, currently just Inventory's default valuation
    /// scope. A second lookup beyond `workspace_context`'s own (which
    /// discards the loaded `Workspace` after resolving id/owner) rather
    /// than threading a `Workspace` through every route signature.
    pub(crate) async fn workspace_default_market_scope(
        &self,
        workspace_id: iskworks_core::WorkspaceId,
    ) -> Result<iskworks_core::MarketScope, ApiError> {
        let state = self
            .workspace_service()
            .get_workspace_state_by_id(workspace_id)
            .await?;
        Ok(state
            .workspace
            .map(|workspace| workspace.default_market_scope())
            .unwrap_or(iskworks_core::DEFAULT_MARKET_SCOPE))
    }

    pub(crate) fn build_planning_service(&self) -> BuildPlanningService {
        BuildPlanningService::new(self.sde_repository.clone())
    }

    pub(crate) fn reaction_planning_service(&self) -> ReactionPlanningService {
        ReactionPlanningService::new(self.sde_repository.clone())
    }

    pub(crate) fn component_expansion_service(&self) -> ComponentExpansionService {
        ComponentExpansionService::new(self.sde_repository.clone())
    }
}

/// The error every ESI-backed accessor returns when EVE SSO isn't configured.
fn esi_not_configured() -> ApiError {
    ApiError::Integration(EsiApplicationError::Configuration(
        "EVE integration is not configured. Set EVE SSO credentials or enable explicit fixture mode."
            .to_string(),
    ))
}
