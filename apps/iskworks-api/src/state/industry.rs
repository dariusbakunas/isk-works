//! `AppState`'s industry dependencies: the [`IndustryDeps`] group, its
//! `with_*` builders and accessors.

use super::AppState;
use crate::error::ApiError;
use iskworks_app::{
    BuildGraphCoordinator, BuildMaterialsCoordinator, BuildPlanningDeps, BuildWorksheetCoordinator,
    DescendantProductionConfigurationCoordinator, ExecutionPlanCoordinator, OrderPlanCoordinator,
};
use iskworks_core::order::{OrderError, OrderRepository, TicketJobRepository};
use iskworks_core::{
    FacilityError, FinanceError, IndustryError, IndustryRepository, IndustryService,
    InventoryError, InventoryRepository, InventoryService, ProductionError, ProductionRepository,
    WorkspaceId,
};
use std::sync::Arc;

/// Industry persistence: Builds and planning, inventory and production, Epics and tickets, facilities, finance and the blueprint library.
#[derive(Clone, Default)]
pub(crate) struct IndustryDeps {
    pub(crate) industry_repository: Option<Arc<dyn IndustryRepository>>,
    pub(super) inventory_repository: Option<Arc<dyn InventoryRepository>>,
    pub(super) production_repository: Option<Arc<dyn ProductionRepository>>,
    pub(super) order_repository: Option<Arc<dyn OrderRepository>>,
    pub(super) ticket_job_repository: Option<Arc<dyn TicketJobRepository>>,
    pub(super) facility_repository: Option<Arc<iskworks_storage::PgFacilityRepository>>,
    pub(super) facility_defaults_repository:
        Option<Arc<dyn iskworks_core::facility_defaults::FacilityDefaultsRepository>>,
    pub(super) finance_repository: Option<Arc<iskworks_storage::PgFinanceRepository>>,
    pub(super) finance_analytics_repository:
        Option<Arc<dyn iskworks_core::FinanceAnalyticsRepository>>,
    pub(super) inventory_runway_repository:
        Option<Arc<dyn iskworks_core::InventoryRunwayRepository>>,
    pub(super) inventory_consumption_repository:
        Option<Arc<dyn iskworks_core::InventoryConsumptionRepository>>,
    pub(super) blueprint_library_repository:
        Option<Arc<dyn iskworks_core::BlueprintLibraryRepository>>,
}

impl AppState {
    #[must_use]
    pub fn with_industry_repository(mut self, repository: Arc<dyn IndustryRepository>) -> Self {
        self.industry.industry_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_inventory_repository(mut self, repository: Arc<dyn InventoryRepository>) -> Self {
        self.industry.inventory_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_production_repository(mut self, repository: Arc<dyn ProductionRepository>) -> Self {
        self.industry.production_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_order_repository(mut self, repository: Arc<dyn OrderRepository>) -> Self {
        self.industry.order_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_ticket_job_repository(mut self, repository: Arc<dyn TicketJobRepository>) -> Self {
        self.industry.ticket_job_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_facility_repository(
        mut self,
        repository: Arc<iskworks_storage::PgFacilityRepository>,
    ) -> Self {
        self.industry.facility_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_facility_defaults_repository(
        mut self,
        repository: Arc<dyn iskworks_core::facility_defaults::FacilityDefaultsRepository>,
    ) -> Self {
        self.industry.facility_defaults_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_finance_repository(
        mut self,
        repository: Arc<iskworks_storage::PgFinanceRepository>,
    ) -> Self {
        self.industry.finance_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_finance_analytics_repository(
        mut self,
        repository: Arc<dyn iskworks_core::FinanceAnalyticsRepository>,
    ) -> Self {
        self.industry.finance_analytics_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_inventory_runway_repository(
        mut self,
        repository: Arc<dyn iskworks_core::InventoryRunwayRepository>,
    ) -> Self {
        self.industry.inventory_runway_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_inventory_consumption_repository(
        mut self,
        repository: Arc<dyn iskworks_core::InventoryConsumptionRepository>,
    ) -> Self {
        self.industry.inventory_consumption_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_blueprint_library_repository(
        mut self,
        repository: Arc<dyn iskworks_core::BlueprintLibraryRepository>,
    ) -> Self {
        self.industry.blueprint_library_repository = Some(repository);
        self
    }

    pub(crate) fn industry_repository(&self) -> Result<Arc<dyn IndustryRepository>, ApiError> {
        self.industry.industry_repository.clone().ok_or_else(|| {
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
            production_repository: self.industry.production_repository.clone(),
            inventory_repository: self.industry.inventory_repository.clone(),
            market_repository: self.market.market_repository.clone(),
            public_market_service: self.market.public_market_service.clone(),
            esi_service: self.esi.esi_service.clone(),
            adjusted_price_repository: self.market.adjusted_price_repository.clone(),
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
        self.industry.inventory_repository.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Inventory persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn inventory_service(&self) -> Result<InventoryService, ApiError> {
        Ok(InventoryService::new(self.inventory_repository()?))
    }

    pub(crate) fn production_repository(&self) -> Result<Arc<dyn ProductionRepository>, ApiError> {
        self.industry.production_repository.clone().ok_or_else(|| {
            ApiError::Production(ProductionError::Persistence(
                "Production persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn order_repository(&self) -> Result<Arc<dyn OrderRepository>, ApiError> {
        self.industry.order_repository.clone().ok_or_else(|| {
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
        match &self.industry.production_repository {
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
        match &self.industry.production_repository {
            Some(repository) => Ok(repository
                .reserved_quantities(workspace_id, owner_id, type_ids)
                .await?),
            None => Ok(std::collections::BTreeMap::new()),
        }
    }

    pub(crate) async fn in_job_quantities(
        &self,
        workspace_id: WorkspaceId,
        owner_id: iskworks_core::OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, u64>, ApiError> {
        match &self.industry.production_repository {
            Some(repository) => Ok(repository
                .in_job_quantities(workspace_id, owner_id, type_ids)
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
        match &self.industry.production_repository {
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
        match &self.industry.production_repository {
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
        match &self.industry.production_repository {
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
            .industry
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

    pub(crate) fn facility_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgFacilityRepository>, ApiError> {
        self.industry.facility_repository.clone().ok_or_else(|| {
            ApiError::Facility(FacilityError::Persistence(
                "Facility persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn facility_defaults_service(
        &self,
    ) -> Result<iskworks_core::facility_defaults::FacilityDefaultsService, ApiError> {
        let repository = self
            .industry
            .facility_defaults_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Facility(FacilityError::Persistence(
                    "Facility persistence is unavailable.".to_string(),
                ))
            })?;
        Ok(
            iskworks_core::facility_defaults::FacilityDefaultsService::new(
                repository,
                self.sde_repository.clone(),
            ),
        )
    }

    pub(crate) fn finance_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgFinanceRepository>, ApiError> {
        self.industry.finance_repository.clone().ok_or_else(|| {
            ApiError::Finance(FinanceError::Persistence(
                "Finance persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn finance_analytics_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::FinanceAnalyticsRepository>, ApiError> {
        self.industry
            .finance_analytics_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Finance(FinanceError::Persistence(
                    "Finance analytics persistence is unavailable.".to_string(),
                ))
            })
    }

    pub(crate) fn inventory_runway_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::InventoryRunwayRepository>, ApiError> {
        self.industry
            .inventory_runway_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Inventory(InventoryError::Persistence(
                    "Inventory runway persistence is unavailable.".to_string(),
                ))
            })
    }

    pub(crate) fn inventory_consumption_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::InventoryConsumptionRepository>, ApiError> {
        self.industry
            .inventory_consumption_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Inventory(InventoryError::Persistence(
                    "Inventory consumption persistence is unavailable.".to_string(),
                ))
            })
    }

    pub(crate) fn blueprint_library_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::BlueprintLibraryRepository>, ApiError> {
        self.industry
            .blueprint_library_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Industry(IndustryError::Persistence(
                    "Blueprint library persistence is unavailable.".to_string(),
                ))
            })
    }
}
