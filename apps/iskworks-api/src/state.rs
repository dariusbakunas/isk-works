//! `AppState` — the dependency container every route receives through
//! `axum`'s `State` extractor — its `with_*` builder methods and its
//! service/repository accessors. Each accessor's "unavailable" fallback
//! error is part of the API's observable behavior (status, code and
//! message), so it must not drift.
//! `UnavailableSdeRepository` is the null `SdeReadRepository` used until a
//! real one is wired in.

use std::env;
use std::sync::Arc;

use iskworks_core::order::{OrderError, OrderRepository};
use iskworks_core::{
    AdjustedPriceRepository, BuildPlanningService, ComponentExpansionService, FacilityError,
    FinanceError, IndustryError, IndustryRepository, IndustryService, InventoryError,
    InventoryRepository, InventoryService, MarketError, MarketRepository, MarketService,
    OpportunityQueryService, ProductionError, ProductionRepository, ReactionPlanningService,
    WorkspaceId, WorkspaceRepository, WorkspaceService,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, SdeError, SdeReadRepository, TypeSearchResult,
};

use iskworks_app::{
    BuildGraphCoordinator, BuildMaterialsCoordinator, BuildPlanningDeps, BuildWorksheetCoordinator,
    CalendarService, CharacterRosterService, CharacterSyncService,
    DescendantProductionConfigurationCoordinator, EsiApplicationError, EsiApplicationService,
    ExecutionPlanCoordinator, OrderPlanCoordinator, PlanetaryCharacterSource, PlanetaryService,
    PublicMarketService,
};

use crate::error::ApiError;
use crate::readiness::ReadinessCheck;
use crate::{AuthApplicationError, AuthService};

#[derive(Clone)]
pub struct AppState {
    repository: Arc<dyn WorkspaceRepository>,
    pub(crate) sde_repository: Arc<dyn SdeReadRepository>,
    pub(crate) industry_repository: Option<Arc<dyn IndustryRepository>>,
    inventory_repository: Option<Arc<dyn InventoryRepository>>,
    production_repository: Option<Arc<dyn ProductionRepository>>,
    order_repository: Option<Arc<dyn OrderRepository>>,
    esi_repository: Option<Arc<iskworks_storage::PgEsiRepository>>,
    pub(crate) esi_service: Option<Arc<EsiApplicationService>>,
    character_sync_service: Option<Arc<CharacterSyncService>>,
    character_roster_service: Option<Arc<CharacterRosterService>>,
    calendar_service: Option<Arc<CalendarService>>,
    planetary_characters: Option<Arc<dyn PlanetaryCharacterSource>>,
    planetary_preferences:
        Option<Arc<dyn iskworks_core::planetary::PlanetaryPreferencesRepository>>,
    facility_repository: Option<Arc<iskworks_storage::PgFacilityRepository>>,
    finance_repository: Option<Arc<iskworks_storage::PgFinanceRepository>>,
    finance_analytics_repository: Option<Arc<dyn iskworks_core::FinanceAnalyticsRepository>>,
    asset_browser_repository: Option<Arc<iskworks_storage::PgAssetBrowserRepository>>,
    pub(crate) market_repository: Option<Arc<dyn MarketRepository>>,
    adjusted_price_repository: Option<Arc<dyn AdjustedPriceRepository>>,
    public_market_service: Option<Arc<PublicMarketService>>,
    pub(crate) auth_service: Option<Arc<AuthService>>,
    pub(crate) admin_config: iskworks_core::AdminConfig,
    invite_admin_repository: Option<Arc<dyn iskworks_core::InviteAdminRepository>>,
    admin_users_repository: Option<Arc<dyn iskworks_core::AdminUsersRepository>>,
    pub(crate) invite_cipher: Option<iskworks_esi::SecretCipher>,
    pub(crate) web_app_origin: String,
    readiness_check: Option<Arc<dyn ReadinessCheck>>,
    /// Answers "is ESI in its daily downtime?" for the web app's badge.
    esi_status: Option<iskworks_esi::HttpEsiTransport>,
}

impl AppState {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self {
            repository,
            sde_repository: Arc::new(UnavailableSdeRepository),
            industry_repository: None,
            inventory_repository: None,
            production_repository: None,
            order_repository: None,
            esi_repository: None,
            esi_service: None,
            character_sync_service: None,
            character_roster_service: None,
            calendar_service: None,
            planetary_characters: None,
            planetary_preferences: None,
            facility_repository: None,
            finance_repository: None,
            finance_analytics_repository: None,
            asset_browser_repository: None,
            market_repository: None,
            adjusted_price_repository: None,
            public_market_service: None,
            auth_service: None,
            admin_config: iskworks_core::AdminConfig::default(),
            invite_admin_repository: None,
            admin_users_repository: None,
            invite_cipher: None,
            web_app_origin: "http://127.0.0.1:5173".to_string(),
            readiness_check: None,
            esi_status: None,
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

    /// The public ESI transport `/api/esi/status` asks about downtime.
    /// Unset, ESI is always reported available.
    #[must_use]
    pub fn with_esi_status(mut self, transport: iskworks_esi::HttpEsiTransport) -> Self {
        self.esi_status = Some(transport);
        self
    }

    pub(crate) fn esi_status(&self) -> Option<&iskworks_esi::HttpEsiTransport> {
        self.esi_status.as_ref()
    }

    #[must_use]
    pub fn with_sde_repository(mut self, repository: Arc<dyn SdeReadRepository>) -> Self {
        self.sde_repository = repository;
        self
    }

    #[must_use]
    pub fn with_industry_repository(mut self, repository: Arc<dyn IndustryRepository>) -> Self {
        self.industry_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_inventory_repository(mut self, repository: Arc<dyn InventoryRepository>) -> Self {
        self.inventory_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_production_repository(mut self, repository: Arc<dyn ProductionRepository>) -> Self {
        self.production_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_order_repository(mut self, repository: Arc<dyn OrderRepository>) -> Self {
        self.order_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_esi(
        mut self,
        repository: Arc<iskworks_storage::PgEsiRepository>,
        service: Option<EsiApplicationService>,
    ) -> Self {
        self.adjusted_price_repository = Some(repository.clone());
        self.esi_repository = Some(repository.clone());
        let character_roster_service = Arc::new(CharacterRosterService::new(
            repository.clone(),
            self.sde_repository.clone(),
        ));
        self.calendar_service = Some(Arc::new(CalendarService::new(
            character_roster_service.clone(),
        )));
        self.character_roster_service = Some(character_roster_service);
        self.planetary_characters = Some(repository.clone());
        self.planetary_preferences = Some(repository.clone());
        let service = service.map(Arc::new);
        self.character_sync_service = service.as_ref().map(|service| {
            Arc::new(CharacterSyncService::new(
                repository,
                service.transport(),
                Arc::clone(service),
                Arc::clone(service),
            ))
        });
        self.esi_service = service;
        self
    }

    /// Planetary Interaction data sources, normally wired by [`Self::with_esi`]
    /// from the Postgres ESI repository; tests substitute fakes.
    #[must_use]
    pub fn with_planetary_repositories(
        mut self,
        characters: Arc<dyn PlanetaryCharacterSource>,
        preferences: Arc<dyn iskworks_core::planetary::PlanetaryPreferencesRepository>,
    ) -> Self {
        self.planetary_characters = Some(characters);
        self.planetary_preferences = Some(preferences);
        self
    }

    #[must_use]
    pub fn with_facility_repository(
        mut self,
        repository: Arc<iskworks_storage::PgFacilityRepository>,
    ) -> Self {
        self.facility_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_finance_repository(
        mut self,
        repository: Arc<iskworks_storage::PgFinanceRepository>,
    ) -> Self {
        self.finance_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_finance_analytics_repository(
        mut self,
        repository: Arc<dyn iskworks_core::FinanceAnalyticsRepository>,
    ) -> Self {
        self.finance_analytics_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_asset_browser_repository(
        mut self,
        repository: Arc<iskworks_storage::PgAssetBrowserRepository>,
    ) -> Self {
        self.asset_browser_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_market_repository(mut self, repository: Arc<dyn MarketRepository>) -> Self {
        self.market_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_adjusted_price_repository(
        mut self,
        repository: Arc<dyn AdjustedPriceRepository>,
    ) -> Self {
        self.adjusted_price_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_public_market_service(mut self, service: PublicMarketService) -> Self {
        self.public_market_service = Some(Arc::new(service));
        self
    }

    #[must_use]
    pub fn with_auth(mut self, service: Option<AuthService>) -> Self {
        self.auth_service = service.map(Arc::new);
        self
    }

    #[must_use]
    pub fn with_admin_config(mut self, config: iskworks_core::AdminConfig) -> Self {
        self.admin_config = config;
        self
    }

    #[must_use]
    pub fn with_invite_admin_repository(
        mut self,
        repository: Arc<dyn iskworks_core::InviteAdminRepository>,
    ) -> Self {
        self.invite_admin_repository = Some(repository);
        self
    }

    /// Enables storing and revealing invite codes (encrypted at rest).
    /// Without it, newly created invites cannot be revealed later.
    #[must_use]
    pub fn with_invite_cipher(mut self, cipher: iskworks_esi::SecretCipher) -> Self {
        self.invite_cipher = Some(cipher);
        self
    }

    #[must_use]
    pub fn with_admin_users_repository(
        mut self,
        repository: Arc<dyn iskworks_core::AdminUsersRepository>,
    ) -> Self {
        self.admin_users_repository = Some(repository);
        self
    }

    pub(crate) fn admin_users_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::AdminUsersRepository>, ApiError> {
        self.admin_users_repository.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "User administration is not available.".to_string(),
            ))
        })
    }

    pub(crate) fn invite_admin_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::InviteAdminRepository>, ApiError> {
        self.invite_admin_repository.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "Invite administration is not available.".to_string(),
            ))
        })
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

    pub(crate) fn industry_repository(&self) -> Result<Arc<dyn IndustryRepository>, ApiError> {
        self.industry_repository.clone().ok_or_else(|| {
            ApiError::Industry(IndustryError::Persistence(
                "Industry persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn industry_service(&self) -> Result<IndustryService, ApiError> {
        Ok(IndustryService::new(
            self.industry_repository()?,
            self.sde_repository.clone(),
        ))
    }

    /// The collaborators every Build-planning coordinator is built from.
    /// Only industry persistence is required here; the optional ones pass
    /// through for each coordinator to degrade on.
    fn build_planning_deps(&self) -> Result<BuildPlanningDeps, ApiError> {
        Ok(BuildPlanningDeps {
            industry_repository: self.industry_repository()?,
            sde_repository: self.sde_repository.clone(),
            production_repository: self.production_repository.clone(),
            inventory_repository: self.inventory_repository.clone(),
            market_repository: self.market_repository.clone(),
            public_market_service: self.public_market_service.clone(),
            esi_service: self.esi_service.clone(),
            adjusted_price_repository: self.adjusted_price_repository.clone(),
        })
    }

    /// The Build Graph coordinator. It reuses the preview's I/O
    /// prelude as-is before composing the hierarchy folds.
    pub(crate) fn build_graph_coordinator(&self) -> Result<BuildGraphCoordinator, ApiError> {
        Ok(BuildGraphCoordinator::new(self.build_planning_deps()?))
    }

    /// The Execution Plan coordinator. Same
    /// collaborator set as `build_graph_coordinator` -- it reuses the exact
    /// same materials/cost prelude, never a second planning walk.
    pub(crate) fn execution_plan_coordinator(&self) -> Result<ExecutionPlanCoordinator, ApiError> {
        Ok(ExecutionPlanCoordinator::new(self.build_planning_deps()?))
    }

    /// Read-only Worksheet coordinator. It shares the authoritative
    /// allocation/cost prelude with Graph and Execution Plan and adds one
    /// bulk SDE type-reference read for row classification.
    pub(crate) fn build_worksheet_coordinator(
        &self,
    ) -> Result<BuildWorksheetCoordinator, ApiError> {
        Ok(BuildWorksheetCoordinator::new(self.build_planning_deps()?))
    }

    /// The Build Materials coordinator (whole-tree inventory allocator).
    /// Same collaborator set as the graph/preview coordinators -- it
    /// reuses their prelude and the shared per-node revision fold, then adds
    /// one `list_balances` read + the pure aggregate allocator.
    pub(crate) fn build_materials_coordinator(
        &self,
    ) -> Result<BuildMaterialsCoordinator, ApiError> {
        Ok(BuildMaterialsCoordinator::new(self.build_planning_deps()?))
    }

    /// The Stages descendant-production-configuration coordinator. Same
    /// collaborator set as the graph/materials coordinators -- it reuses
    /// `BuildMaterialsCoordinator::materials` as-is to re-validate the
    /// live plan immediately before an atomic multi-Build facility/blueprint
    /// mutation.
    pub(crate) fn descendant_production_configuration_coordinator(
        &self,
    ) -> Result<DescendantProductionConfigurationCoordinator, ApiError> {
        Ok(DescendantProductionConfigurationCoordinator::new(
            self.build_planning_deps()?,
        ))
    }

    /// The whole-tree Epic-freeze coordinator. Same
    /// collaborator set as the graph/materials coordinators -- it reuses
    /// `materials_with_planning_cost_for_overlay` as-is, then reduces
    /// that one walk into the frozen plan.
    pub(crate) fn order_plan_coordinator(&self) -> Result<OrderPlanCoordinator, ApiError> {
        Ok(OrderPlanCoordinator::new(self.build_planning_deps()?))
    }

    pub(crate) fn inventory_repository(&self) -> Result<Arc<dyn InventoryRepository>, ApiError> {
        self.inventory_repository.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Inventory persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn inventory_service(&self) -> Result<InventoryService, ApiError> {
        Ok(InventoryService::new(self.inventory_repository()?))
    }

    pub(crate) fn production_repository(&self) -> Result<Arc<dyn ProductionRepository>, ApiError> {
        self.production_repository.clone().ok_or_else(|| {
            ApiError::Production(ProductionError::Persistence(
                "Production persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn order_repository(&self) -> Result<Arc<dyn OrderRepository>, ApiError> {
        self.order_repository.clone().ok_or_else(|| {
            ApiError::Order(OrderError::Persistence(
                "Order persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) async fn reserved_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        type_id: i64,
    ) -> Result<u64, ApiError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .reserved_quantity(workspace_id, owner_id, type_id)
                .await?),
            None => Ok(0),
        }
    }

    /// `reserved_quantity` for every listed type in one read; a type
    /// absent from the map has nothing reserved (0).
    pub(crate) async fn reserved_quantities(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, u64>, ApiError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .reserved_quantities(workspace_id, owner_id, type_ids)
                .await?),
            None => Ok(std::collections::BTreeMap::new()),
        }
    }

    pub(crate) async fn list_reservations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        type_id: i64,
    ) -> Result<Vec<iskworks_core::InventoryReservation>, ApiError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .list_reservations(workspace_id, owner_id, type_id)
                .await?),
            None => Ok(Vec::new()),
        }
    }

    pub(crate) async fn list_esi_observations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
    ) -> Result<std::collections::BTreeMap<i64, iskworks_core::EsiObservation>, ApiError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .list_esi_observations(workspace_id, owner_id)
                .await?),
            None => Ok(std::collections::BTreeMap::new()),
        }
    }

    pub(crate) async fn esi_holdings(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        type_id: i64,
    ) -> Result<iskworks_core::EsiHoldings, ApiError> {
        match &self.production_repository {
            Some(repository) => Ok(repository
                .esi_holdings(workspace_id, owner_id, type_id)
                .await?),
            None => Ok(iskworks_core::EsiHoldings {
                type_id,
                observed_quantity: 0,
                ignored_quantity: 0,
                included_quantity: 0,
                observed_at: None,
                contributors: Vec::new(),
            }),
        }
    }

    pub(crate) async fn set_esi_holding_reconciliation_inclusion(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        command: iskworks_core::SetEsiHoldingReconciliationInclusion,
    ) -> Result<iskworks_core::EsiHoldings, ApiError> {
        Ok(self
            .production_repository
            .as_ref()
            .ok_or_else(|| {
                ApiError::Integration(iskworks_app::EsiApplicationError::Configuration(
                    "ESI persistence is unavailable.".into(),
                ))
            })?
            .set_esi_holding_reconciliation_inclusion(workspace_id, owner_id, command)
            .await?)
    }

    pub(crate) fn esi_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgEsiRepository>, ApiError> {
        self.esi_repository.clone().ok_or_else(|| {
            ApiError::Integration(EsiApplicationError::Configuration(
                "ESI persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn esi_service(&self) -> Result<Arc<EsiApplicationService>, ApiError> {
        self.esi_service.clone().ok_or_else(esi_not_configured)
    }

    pub(crate) fn character_sync_service(&self) -> Result<Arc<CharacterSyncService>, ApiError> {
        self.character_sync_service
            .clone()
            .ok_or_else(esi_not_configured)
    }

    pub(crate) fn character_roster_service(&self) -> Result<Arc<CharacterRosterService>, ApiError> {
        self.character_roster_service
            .clone()
            .ok_or_else(esi_not_configured)
    }

    pub(crate) fn planetary_service(&self) -> Result<PlanetaryService, ApiError> {
        Ok(PlanetaryService::new(
            self.planetary_characters
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.planetary_preferences
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.sde_repository.clone(),
            self.market_repository()?,
        ))
    }

    /// Calendar milestones, plus Planetary Interaction timers whenever the PI
    /// read model is available (it is optional: without it the calendar
    /// still serves industry and skill milestones).
    pub(crate) fn calendar_service(&self) -> Result<Arc<CalendarService>, ApiError> {
        let service = self.calendar_service.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Calendar persistence is unavailable.".to_string(),
            ))
        })?;
        Ok(match self.planetary_service() {
            Ok(planetary) => Arc::new((*service).clone().with_planetary(Arc::new(planetary))),
            Err(_) => service,
        })
    }

    pub(crate) fn facility_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgFacilityRepository>, ApiError> {
        self.facility_repository.clone().ok_or_else(|| {
            ApiError::Facility(FacilityError::Persistence(
                "Facility persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn finance_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgFinanceRepository>, ApiError> {
        self.finance_repository.clone().ok_or_else(|| {
            ApiError::Finance(FinanceError::Persistence(
                "Finance persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn finance_analytics_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::FinanceAnalyticsRepository>, ApiError> {
        self.finance_analytics_repository.clone().ok_or_else(|| {
            ApiError::Finance(FinanceError::Persistence(
                "Finance analytics persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn asset_browser_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgAssetBrowserRepository>, ApiError> {
        self.asset_browser_repository.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Asset browser persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn market_repository(&self) -> Result<Arc<dyn MarketRepository>, ApiError> {
        self.market_repository.clone().ok_or_else(|| {
            ApiError::Market(MarketError::Persistence(
                "Market persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn market_service(&self) -> Result<MarketService, ApiError> {
        Ok(MarketService::new(self.market_repository()?))
    }

    pub(crate) fn adjusted_price_repository(
        &self,
    ) -> Result<Arc<dyn AdjustedPriceRepository>, ApiError> {
        self.adjusted_price_repository.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Adjusted-price persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn opportunity_service(&self) -> Result<OpportunityQueryService, ApiError> {
        Ok(OpportunityQueryService::new(
            self.industry_repository()?,
            self.sde_repository.clone(),
            self.market_repository()?,
            self.adjusted_price_repository()?,
        )
        .with_freshness(iskworks_core::OpportunityEvidenceFreshness {
            market: evidence_freshness("ISKWORKS_WORKER_MARKET_FRESHNESS_SECONDS", 900),
            adjusted_prices: evidence_freshness(
                "ISKWORKS_WORKER_ADJUSTED_PRICE_FRESHNESS_SECONDS",
                21_600,
            ),
            system_index: evidence_freshness(
                "ISKWORKS_WORKER_SYSTEM_INDEX_FRESHNESS_SECONDS",
                3_600,
            ),
        }))
    }

    pub(crate) fn public_market_service(&self) -> Result<Arc<PublicMarketService>, ApiError> {
        self.public_market_service.clone().ok_or_else(|| {
            ApiError::Market(MarketError::Persistence(
                "Public ESI market pricing is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn auth_service(&self) -> Result<Arc<AuthService>, ApiError> {
        self.auth_service.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "EVE SSO login is not configured. Set EVE_SSO_CLIENT_ID.".to_string(),
            ))
        })
    }
}

fn evidence_freshness(name: &str, default_seconds: i64) -> chrono::Duration {
    let seconds = env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(default_seconds);
    chrono::Duration::seconds(seconds)
}

pub(crate) struct UnavailableSdeRepository;

#[async_trait::async_trait]
impl SdeReadRepository for UnavailableSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        _blueprint_type_id: i64,
    ) -> Result<Option<iskworks_sde::ManufacturingRecipe>, SdeError> {
        Ok(None)
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }
}

/// The error every ESI-backed accessor returns when EVE SSO isn't configured.
fn esi_not_configured() -> ApiError {
    ApiError::Integration(EsiApplicationError::Configuration(
        "EVE integration is not configured. Set EVE SSO credentials or enable explicit fixture mode."
            .to_string(),
    ))
}
