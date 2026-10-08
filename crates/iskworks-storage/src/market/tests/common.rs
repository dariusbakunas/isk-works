//! Shared fixtures for the split `market` persistence tests.

pub(super) use std::sync::Arc;

pub(super) use chrono::{DateTime, TimeZone, Utc};
pub(super) use iskworks_core::{
    EsiMarketOrder, IndustryRepository, MarketCoverageRegistration, MarketError,
    MarketObservationBatchId, MarketOrderSide, MarketPriceRequest, MarketPricingPolicy,
    MarketRefreshState, MarketService, MarketUpload, Money, PriceSourceId,
    PublicMarketObservationBatch, WorkspaceId,
};
pub(super) use sqlx::PgPool;
pub(super) use uuid::Uuid;

pub(super) use crate::market::coverage::load_coverage;
pub(super) use crate::market::*;
pub(super) use crate::PgIndustryRepository;

pub(super) async fn fixture(pool: &PgPool) -> (WorkspaceId, Arc<PgMarketRepository>) {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Market Test',$2,$3,$3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Market Test',true,$3,$3)",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','market-fixture','active',true,$2,$2)",
    )
    .bind(import_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES ($1,34,'Tritanium',true)",
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (
        WorkspaceId(workspace_id),
        Arc::new(PgMarketRepository::new(pool.clone())),
    )
}

pub(super) fn upload() -> MarketUpload {
    MarketUpload {
        filename: "Insmother-Tritanium-2026.07.26 192639.txt".to_string(),
        content: include_bytes!(
            "../../../../iskworks-core/tests/fixtures/market/Insmother-Tritanium-2026.07.26 192639.txt"
        )
        .to_vec(),
        user_observed_at: None,
    }
}

pub(super) async fn esi_source(pool: &PgPool, workspace_id: WorkspaceId) -> PriceSourceId {
    esi_source_at(pool, workspace_id, 60_003_760, 30_000_142, 10_000_002).await
}

/// `esi_source` with an explicit `(location_id, solar_system_id, region_id)`
/// scope -- used to exercise a structure-scoped `esi_market_orders` source
/// (a large positive `location_id` that is not an NPC station) through the
/// same completion repository path.
pub(super) async fn esi_source_at(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    location_id: i64,
    solar_system_id: i64,
    region_id: i64,
) -> PriceSourceId {
    let source_id = PriceSourceId::new();
    let now = crate::db_now();
    sqlx::query(
        r#"
        INSERT INTO price_sources (
          id,workspace_id,display_name,description,source_kind,
          revision,created_at,updated_at
        ) VALUES ($1,$2,'Jita 4-4','','esi_market_orders',1,$3,$3)
        "#,
    )
    .bind(source_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_price_source_configs (
          price_source_id,workspace_id,location_id,solar_system_id,region_id,
          location_alias,source_kind,pricing_policy,coverage_policy,observation_mode,
          pinned_batch_id,fresh_after_hours,stale_after_hours
        ) VALUES (
          $1,$2,$3,$4,$5,'Scope','esi_market_orders',
          'acquire_quantity_from_sell_orders','allow_partial_with_warning',
          'latest_compatible_import',NULL,1,24
        )
        "#,
    )
    .bind(source_id.0)
    .bind(workspace_id.0)
    .bind(location_id)
    .bind(solar_system_id)
    .bind(region_id)
    .execute(pool)
    .await
    .unwrap();
    source_id
}
