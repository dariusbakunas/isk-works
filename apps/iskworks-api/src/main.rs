use std::net::SocketAddr;
use std::sync::Arc;

use iskworks_api::{build_router, ApiConfig, AppState, AuthService};
use iskworks_app::{EsiApplicationService, MarketAccessResolver, PublicMarketService};
use iskworks_core::order::OrderRepository;
use iskworks_core::{
    IndustryRepository, InventoryRepository, MarketRepository, ProductionRepository,
    WorkspaceRepository,
};
use iskworks_esi::HttpEsiTransport;
use iskworks_sde::SdeReadRepository;
use iskworks_storage::{
    PgEsiRepository, PgFacilityRepository, PgFinanceRepository, PgIndustryRepository,
    PgInventoryRepository, PgMarketRepository, PgOrderRepository, PgProductionRepository,
    PgSdeRepository, PgWorkspaceRepository,
};
use sqlx::migrate::Migrator;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const DEFAULT_ESI_BASE_URL: &str = "https://esi.evetech.net";

fn configured_esi_base_url(configured: Option<String>) -> String {
    configured
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ESI_BASE_URL.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // RUST_LOG filters only the fmt layer. The pool metrics layer needs
    // sqlx's TRACE acquire events, which a global filter would drop.
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        ))
        .with(iskworks_storage::db_pool_metrics_layer())
        .init();

    let config = ApiConfig::from_env()?;
    if let Some(addr) = config.metrics_addr {
        iskworks_api::metrics_exporter::install(addr)?;
        tracing::info!("API metrics listening on {addr}");
    }
    if !iskworks_esi::contact_configured() {
        tracing::warn!(
            "ISKWORKS_ESI_CONTACT is not set; ESI requests won't say how to reach this \
             instance's operator. Set it to an email, Discord handle, or EVE character name."
        );
    }
    let repository =
        PgWorkspaceRepository::connect_with(&config.database_url, &config.db_pool).await?;
    MIGRATOR.run(repository.pool()).await?;
    if config.metrics_addr.is_some() {
        iskworks_storage::spawn_pool_sampler(repository.pool().clone());
    }

    let sde_repository = PgSdeRepository::new(repository.pool().clone());
    // Finance analytics categories are materialized from the SDE; an import
    // that predates them gets built here instead of needing a re-import.
    match sde_repository.ensure_active_type_categories().await {
        Ok(Some(rows)) => tracing::info!(rows, "built SDE type categories"),
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "could not build SDE type categories"),
    }
    let industry_repository = PgIndustryRepository::new(repository.pool().clone());
    let facility_repository = Arc::new(PgFacilityRepository::new(repository.pool().clone()));
    let finance_repository = Arc::new(PgFinanceRepository::new(repository.pool().clone()));
    let asset_browser_repository = Arc::new(iskworks_storage::PgAssetBrowserRepository::new(
        repository.pool().clone(),
    ));
    let blueprint_library_repository = Arc::new(
        iskworks_storage::PgBlueprintLibraryRepository::new(repository.pool().clone()),
    ) as Arc<dyn iskworks_core::BlueprintLibraryRepository>;
    let fit_repository = Arc::new(iskworks_storage::PgFitRepository::new(
        repository.pool().clone(),
    ));
    let skill_plan_repository = Arc::new(iskworks_storage::PgSkillPlanRepository::new(
        repository.pool().clone(),
    )) as Arc<dyn iskworks_core::skill_plans::SkillPlanRepository>;
    let pi_planner_repository = Arc::new(iskworks_storage::PgPiPlannerInputsRepository::new(
        repository.pool().clone(),
    ))
        as Arc<dyn iskworks_core::pi_planner::PiPlannerInputsRepository>;
    let inventory_repository = Arc::new(PgInventoryRepository::new(repository.pool().clone()));
    let inventory_trait = inventory_repository.clone() as Arc<dyn InventoryRepository>;
    let production_repository = Arc::new(PgProductionRepository::new(repository.pool().clone()))
        as Arc<dyn ProductionRepository>;
    let order_repository =
        Arc::new(PgOrderRepository::new(repository.pool().clone())) as Arc<dyn OrderRepository>;
    let ticket_job_repository = Arc::new(PgOrderRepository::new(repository.pool().clone()))
        as Arc<dyn iskworks_core::order::TicketJobRepository>;
    let inventory_runway_repository = Arc::new(PgOrderRepository::new(repository.pool().clone()))
        as Arc<dyn iskworks_core::InventoryRunwayRepository>;
    let esi_repository = Arc::new(PgEsiRepository::new(repository.pool().clone()));
    let pg_market_repository = Arc::new(PgMarketRepository::new(repository.pool().clone()));
    let market_repository = pg_market_repository.clone() as Arc<dyn MarketRepository>;
    let esi_service = EsiApplicationService::from_env(esi_repository.clone())?;
    // Upwell structure market orders need an authenticated ESI resolver --
    // the same `EsiApplicationService` character sync already uses -- or
    // every structure-scoped refresh fails immediately with "ESI is not
    // configured" (surfaced as a 502, since single-item requests are
    // synchronous). Mirrors how
    // `apps/iskworks-worker`'s `main.rs` wires the same resolver into its
    // own `PublicMarketService`.
    let market_access_resolver: Option<Arc<dyn MarketAccessResolver>> = esi_service
        .clone()
        .map(|service| Arc::new(service) as Arc<dyn MarketAccessResolver>);
    let public_esi_transport = HttpEsiTransport::public(configured_esi_base_url(
        std::env::var("EVE_ESI_BASE_URL").ok(),
    ));
    let public_market_service =
        PublicMarketService::new(pg_market_repository, Arc::new(public_esi_transport.clone()))
            .with_market_access(market_access_resolver);
    let user_repository = Arc::new(iskworks_storage::PgUserRepository::new(
        repository.pool().clone(),
    ));
    let session_repository = Arc::new(iskworks_storage::PgSessionRepository::new(
        repository.pool().clone(),
    ));
    let invite_repository = Arc::new(iskworks_storage::PgInviteRepository::new(
        repository.pool().clone(),
    ));
    let invite_admin_repository: Arc<dyn iskworks_core::InviteAdminRepository> =
        invite_repository.clone();
    let auth_service =
        AuthService::from_env(user_repository, session_repository, invite_repository)?;
    let admin_users_repository: Arc<dyn iskworks_core::AdminUsersRepository> = Arc::new(
        iskworks_storage::PgAdminRepository::new(repository.pool().clone()),
    );
    let admin_config = iskworks_api::admin_config_from_env()?;
    let invite_cipher = match std::env::var("TOKEN_ENCRYPTION_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(iskworks_esi::SecretCipher::from_base64(&key)?),
        _ => None,
    };
    // Fail closed: a production deploy sets ISKWORKS_AUTH_REQUIRED=true, and
    // the API must refuse to start (non-zero exit, clear error) rather than
    // boot with auth_service = None and serve the singleton workspace to
    // any unauthenticated caller.
    iskworks_api::ensure_auth_available(
        iskworks_api::auth_required_from_env(),
        auth_service.is_some(),
    )?;
    let readiness_pool = repository.pool().clone();
    let state = AppState::new(Arc::new(repository) as Arc<dyn WorkspaceRepository>)
        .with_readiness_check(Arc::new(readiness_pool))
        .with_sde_repository(Arc::new(sde_repository) as Arc<dyn SdeReadRepository>)
        .with_industry_repository(Arc::new(industry_repository) as Arc<dyn IndustryRepository>)
        .with_inventory_repository(inventory_trait)
        .with_production_repository(production_repository)
        .with_order_repository(order_repository)
        .with_ticket_job_repository(ticket_job_repository)
        .with_inventory_runway_repository(inventory_runway_repository)
        .with_inventory_consumption_repository(
            inventory_repository.clone() as Arc<dyn iskworks_core::InventoryConsumptionRepository>
        )
        .with_facility_repository(facility_repository.clone())
        .with_facility_defaults_repository(facility_repository)
        .with_finance_analytics_repository(
            finance_repository.clone() as Arc<dyn iskworks_core::FinanceAnalyticsRepository>
        )
        .with_finance_repository(finance_repository)
        .with_fit_asset_holdings(asset_browser_repository.clone()
            as Arc<dyn iskworks_core::fitting::AssetHoldingsRepository>)
        .with_asset_browser_repository(asset_browser_repository)
        .with_blueprint_library_repository(blueprint_library_repository)
        .with_skill_plan_repository(skill_plan_repository)
        .with_pi_planner_repository(pi_planner_repository)
        .with_fit_repository(fit_repository)
        .with_market_repository(market_repository)
        .with_public_market_service(public_market_service)
        .with_esi_status(public_esi_transport)
        .with_esi(esi_repository, esi_service)
        .with_auth(auth_service)
        .with_admin_config(admin_config)
        .with_invite_admin_repository(invite_admin_repository)
        .with_admin_users_repository(admin_users_repository)
        .with_web_app_origin(
            std::env::var("WEB_APP_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:5173".to_string()),
        );
    let state = match invite_cipher {
        Some(cipher) => state.with_invite_cipher(cipher),
        None => state,
    };
    let state = match load_fitting_engine() {
        Some(engine) => state.with_fitting_engine(engine),
        None => state,
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;

    tracing::info!("ISK Works API listening on {}", config.listen_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// The fitting engine and its data (`FITTING_SDE_PATH`, default
/// `data/fitting/sde.dat` from `make fitting-data`; the API image sets the
/// path). Missing or stale data only disables fit stats.
fn load_fitting_engine() -> Option<Arc<dyn iskworks_core::fitting::FittingEngine>> {
    let path = std::env::var("FITTING_SDE_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "data/fitting/sde.dat".to_string());
    match iskworks_fitting::EsfFittingEngine::load(std::path::Path::new(&path)) {
        Ok(engine) => {
            let info = engine.engine_info();
            tracing::info!(
                path,
                data_version = info.data_version,
                build = info.build_number,
                "loaded fitting engine data"
            );
            Some(Arc::new(engine))
        }
        Err(error) => {
            tracing::warn!(path, %error, "fit stats are unavailable");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::configured_esi_base_url;

    #[test]
    fn empty_exported_esi_base_url_uses_ccp_default() {
        assert_eq!(
            configured_esi_base_url(Some(String::new())),
            "https://esi.evetech.net"
        );
        assert_eq!(
            configured_esi_base_url(Some("   ".to_string())),
            "https://esi.evetech.net"
        );
    }
}
