use std::sync::Arc;

use iskworks_app::{CharacterSyncService, EsiApplicationService, MarketAccessResolver};
use iskworks_esi::HttpEsiTransport;
use iskworks_storage::{
    PgAuthMaintenance, PgEsiRepository, PgMarketRepository, PgWorkspaceRepository,
};
use iskworks_worker::{EvidenceWorker, WorkerConfig};
use sqlx::migrate::Migrator;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const DEFAULT_DATABASE_URL: &str = "postgres://iskworks:iskworks@127.0.0.1:5432/iskworks";
const DEFAULT_ESI_BASE_URL: &str = "https://esi.evetech.net";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = WorkerConfig::from_env()?;
    if !iskworks_esi::contact_configured() {
        tracing::warn!(
            "ISKWORKS_ESI_CONTACT is not set; ESI requests won't say how to reach this \
             instance's operator. Set it to an email, Discord handle, or EVE character name."
        );
    }
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
    let workspace_repository = PgWorkspaceRepository::connect(&database_url).await?;
    MIGRATOR.run(workspace_repository.pool()).await?;
    let market_repository = Arc::new(PgMarketRepository::new(workspace_repository.pool().clone()));
    let esi_repository = Arc::new(PgEsiRepository::new(workspace_repository.pool().clone()));

    let esi_service = EsiApplicationService::from_env(esi_repository.clone())?;
    let (character_sync_service, market_access_resolver) = match esi_service {
        Some(service) => {
            let service = Arc::new(service);
            let character_sync = CharacterSyncService::new(
                esi_repository.clone(),
                service.transport(),
                Arc::clone(&service),
                Arc::clone(&service),
            );
            let market_access: Arc<dyn MarketAccessResolver> = service;
            (Some(character_sync), Some(market_access))
        }
        None => {
            tracing::info!("character sync disabled: EVE SSO is not configured");
            (None, None)
        }
    };

    let esi_base_url = std::env::var("EVE_ESI_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ESI_BASE_URL.to_string());
    let transport = Arc::new(HttpEsiTransport::public(esi_base_url));
    let worker = Arc::new(
        EvidenceWorker::new(config.clone(), esi_repository, market_repository, transport)
            .with_character_sync(character_sync_service)
            .with_market_access(market_access_resolver)
            .with_auth_maintenance(Some(PgAuthMaintenance::new(
                workspace_repository.pool().clone(),
            ))),
    );

    iskworks_worker::run(worker, &config, shutdown_signal()).await;
    Ok(())
}

/// Resolves on the first SIGINT (Ctrl+C) or SIGTERM. Signal-handler
/// installation errors are logged rather than propagated -- a worker that
/// cannot observe SIGTERM should still run and still stop on Ctrl+C.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(error) => {
                    tracing::error!(%error, "failed to install SIGTERM handler; ctrl-c only");
                    let _ = tokio::signal::ctrl_c().await;
                    return;
                }
            };
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "ctrl-c handler error");
                }
            }
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "ctrl-c handler error");
        }
    }
}
