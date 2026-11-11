//! `AppState`'s market dependencies: the [`MarketDeps`] group, its
//! `with_*` builders and accessors.

use super::AppState;
use crate::error::ApiError;
use iskworks_app::PublicMarketService;
use iskworks_core::{
    AdjustedPriceRepository, InventoryError, MarketError, MarketRepository, MarketService,
    OpportunityQueryService,
};
use std::sync::{Arc, OnceLock};

/// Market data: stored and public order books, adjusted prices and the Market Orders read model.
#[derive(Clone, Default)]
pub(crate) struct MarketDeps {
    pub(crate) market_repository: Option<Arc<dyn MarketRepository>>,
    pub(super) adjusted_price_repository: Option<Arc<dyn AdjustedPriceRepository>>,
    pub(super) public_market_service: Option<Arc<PublicMarketService>>,
    pub(super) market_orders_service: Option<Arc<iskworks_app::MarketOrdersService>>,
    pub(super) opportunity_freshness: iskworks_core::OpportunityEvidenceFreshness,
    /// Built on first use and shared by every clone of the state, so a
    /// request doesn't rebuild them. Builders must not run after the first
    /// request (they don't: `main.rs` finishes the state before serving).
    built_market_orders_service: Arc<OnceLock<Arc<iskworks_app::MarketOrdersService>>>,
    built_opportunity_service: Arc<OnceLock<Arc<OpportunityQueryService>>>,
}

impl AppState {
    /// For tests: the Market Orders read model with fake sources.
    #[must_use]
    pub fn with_market_orders_service(
        mut self,
        service: Arc<iskworks_app::MarketOrdersService>,
    ) -> Self {
        self.market.market_orders_service = Some(service);
        self
    }

    /// The Opportunities freshness windows, read once at startup
    /// (`ApiConfig::opportunity_freshness`). Unset, the core defaults apply.
    #[must_use]
    pub fn with_opportunity_freshness(
        mut self,
        freshness: iskworks_core::OpportunityEvidenceFreshness,
    ) -> Self {
        self.market.opportunity_freshness = freshness;
        self
    }

    #[must_use]
    pub fn with_market_repository(mut self, repository: Arc<dyn MarketRepository>) -> Self {
        self.market.market_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_adjusted_price_repository(
        mut self,
        repository: Arc<dyn AdjustedPriceRepository>,
    ) -> Self {
        self.market.adjusted_price_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_public_market_service(mut self, service: PublicMarketService) -> Self {
        self.market.public_market_service = Some(Arc::new(service));
        self
    }

    pub(crate) fn market_repository(&self) -> Result<Arc<dyn MarketRepository>, ApiError> {
        self.market.market_repository.clone().ok_or_else(|| {
            ApiError::Market(MarketError::Persistence(
                "Market persistence is unavailable.".to_string(),
            ))
        })
    }

    /// The Market Orders read model, built from the ESI, market and
    /// inventory stores (unless a test injected one). Structure resolution
    /// is added when EVE SSO is configured.
    pub(crate) fn market_orders_service(
        &self,
    ) -> Result<Arc<iskworks_app::MarketOrdersService>, ApiError> {
        if let Some(service) = &self.market.market_orders_service {
            return Ok(service.clone());
        }
        if let Some(service) = self.market.built_market_orders_service.get() {
            return Ok(service.clone());
        }
        let esi = self.esi.esi_repository.clone().ok_or_else(|| {
            ApiError::Inventory(InventoryError::Persistence(
                "Market order persistence is unavailable.".to_string(),
            ))
        })?;
        let market = self.market_repository()?;
        let mut service = iskworks_app::MarketOrdersService::new(
            esi.clone(),
            esi,
            market.clone(),
            self.sde_repository.clone(),
            self.inventory_repository()?,
        );
        if let Some(esi_service) = &self.esi.esi_service {
            service = service.with_structure_resolver(Arc::new(
                iskworks_app::EsiStructureLocationResolver::new(esi_service.clone(), market),
            ));
        }
        Ok(self
            .market
            .built_market_orders_service
            .get_or_init(|| Arc::new(service))
            .clone())
    }

    pub(crate) fn market_service(&self) -> Result<MarketService, ApiError> {
        Ok(MarketService::new(self.market_repository()?))
    }

    pub(crate) fn adjusted_price_repository(
        &self,
    ) -> Result<Arc<dyn AdjustedPriceRepository>, ApiError> {
        self.market
            .adjusted_price_repository
            .clone()
            .ok_or_else(|| {
                ApiError::Inventory(InventoryError::Persistence(
                    "Adjusted-price persistence is unavailable.".to_string(),
                ))
            })
    }

    pub(crate) fn opportunity_service(&self) -> Result<Arc<OpportunityQueryService>, ApiError> {
        if let Some(service) = self.market.built_opportunity_service.get() {
            return Ok(service.clone());
        }
        let service = OpportunityQueryService::new(
            self.industry_repository()?,
            self.sde_repository.clone(),
            self.market_repository()?,
            self.adjusted_price_repository()?,
        )
        .with_freshness(self.market.opportunity_freshness);
        Ok(self
            .market
            .built_opportunity_service
            .get_or_init(|| Arc::new(service))
            .clone())
    }

    pub(crate) fn public_market_service(&self) -> Result<Arc<PublicMarketService>, ApiError> {
        self.market.public_market_service.clone().ok_or_else(|| {
            ApiError::Market(MarketError::Persistence(
                "Public ESI market pricing is unavailable.".to_string(),
            ))
        })
    }
}
