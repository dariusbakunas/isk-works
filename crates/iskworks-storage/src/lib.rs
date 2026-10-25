use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{
    AppError, NewWorkspace, Owner, OwnerId, OwnerKind, Workspace, WorkspaceId, WorkspaceRepository,
    WorkspaceState,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, CandidateMarketGroup, CandidateProductClassification,
    CandidateRecipeIdentity, CandidateRecipeKind, ImportCounts, ImportPhase,
    ManufacturableCandidateRecipe, ManufacturableCandidateScope, ManufacturingRecipe, NewImport,
    NormalizedSde, ProgressEvent, ProgressReporter, ReactionFormulaRecipe,
    ReactionFormulaSearchResult, RecipeLine, SdeError, SdeImportStore, SdeInventoryTypeMetadata,
    SdeReadRepository, SdeTypeClassification, TypeSearchResult,
};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

mod admin;
mod assets;
mod auth;
mod blueprint_library;
mod cascade;
mod erase;
mod esi;
mod facility;
mod finance;
mod finance_analytics;
mod industry;
mod inventory;
mod invite;
mod market;
mod material_types;
mod order;
mod pool;
mod pool_metrics;
mod production;
pub use admin::PgAdminRepository;
pub use assets::PgAssetBrowserRepository;
pub use auth::{
    AuthPurgeOutcome, OwnerHashCheck, PendingLoginAuthorization, PgAuthMaintenance,
    PgSessionRepository, PgUserRepository,
};
pub use blueprint_library::PgBlueprintLibraryRepository;
pub use esi::{
    EsiObservationPruneOutcome, FacilityLabel, PendingAuthorization, PgEsiRepository,
    StoredRefreshToken,
};
pub use facility::{KnownStructure, PgFacilityRepository};
pub use finance::PgFinanceRepository;
pub use industry::PgIndustryRepository;
pub use inventory::PgInventoryRepository;
pub use invite::PgInviteRepository;
pub use iskworks_core::{InviteSummary, NewInvite};
pub use market::{MarketObservationPruneOutcome, PgMarketRepository};
pub use order::PgOrderRepository;
pub use pool::DbPoolConfig;
pub use pool_metrics::{
    db_pool_metrics_layer, init_metrics, spawn_pool_sampler, ACQUIRE_BUCKETS, ACQUIRE_DURATION,
};
pub use production::PgProductionRepository;

mod sde_categories;
mod sde_import;
mod sde_read;
mod skill_plans;
mod wallet_recording;
mod workspace;
pub use sde_import::PgSdeRepository;
pub use skill_plans::PgSkillPlanRepository;
pub use workspace::PgWorkspaceRepository;

#[cfg(test)]
mod tests;

/// The current time at the microsecond precision Postgres stores
/// `timestamptz` with. Every timestamp a repository generates goes through
/// this, so the value a write returns equals the value a later read returns.
/// Linux clocks have nanosecond precision (macOS only microsecond), so a raw
/// `Utc::now()` would read back rounded and differ only on Linux.
pub(crate) fn db_now() -> chrono::DateTime<chrono::Utc> {
    use chrono::SubsecRound;
    chrono::Utc::now().trunc_subsecs(6)
}
