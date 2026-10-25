//! Shared application-service layer consumed by both `iskworks-api` and
//! `iskworks-worker`.
//!
//! Axum-free by construction: this crate never depends on `iskworks-api`
//! or any HTTP framework. It holds the orchestration services that are not
//! HTTP-specific -- `PublicMarketService`, `EsiApplicationService`,
//! `CharacterSyncService`, `CharacterRosterService`, the build-planning
//! coordinators (`BuildPreviewCoordinator` and friends), and the ESI/market
//! ports they define.

mod build_graph;
mod build_materials;
mod build_preview;
mod build_worksheet;
mod calendar;
mod character_roster;
mod character_sync;
mod descendant_production_configuration;
mod esi_service;
mod execution_plan;
mod jobs;
mod manual_sync_gate;
mod market_orders;
mod operation_metrics;
mod order_plan;
mod planetary;
mod public_market;
mod skills_catalogue;
mod sync_metrics;
mod ticket_jobs;

pub use build_graph::{BuildGraphCoordinator, BuildGraphError};
pub use build_materials::{BuildMaterialsCoordinator, BuildMaterialsError, BuildMaterialsSummary};
pub use build_preview::{BuildPlanningDeps, BuildPreviewCoordinator, BuildPreviewError};
pub use build_worksheet::{BuildWorksheetCoordinator, BuildWorksheetError};
pub use calendar::{CalendarMilestone, CalendarRange, CalendarService};
pub use character_roster::{
    CharacterDetail, CharacterRosterEntry, CharacterRosterService, IndustryJobDto,
};
pub use character_sync::{AccessTokenProvider, CharacterSyncOutcome, CharacterSyncService};
pub use descendant_production_configuration::{
    DescendantProductionConfigurationCoordinator, DescendantProductionConfigurationError,
};
#[cfg(any(test, feature = "test-support"))]
pub use esi_service::FakeLinkTransport;
pub use esi_service::{
    AuthorizationStart, AutomaticEiv, EsiApplicationError, EsiApplicationService,
    EsiSyncDispatcher, MarketAccessResolution, MarketAccessResolver, SystemCostIndex,
};
pub use execution_plan::{ExecutionPlanCoordinator, ExecutionPlanError};
pub use jobs::{
    ActiveJobs, JobCharacter, JobCharacterSnapshot, JobHistoryView, JobSlots, JobView,
    JobsCharacterSource, JobsService, JobsSyncState, SlotUsage,
};
pub use market_orders::{
    BookUnavailable, DepthOrder, EsiStructureLocationResolver, MarketOrderLocation,
    MarketOrderLocationKind, MarketOrderRow, MarketOrdersError, MarketOrdersOverview,
    MarketOrdersService, MarketOrdersSummary, OrderDepth, StructureLocationResolver,
    TraderCharacter, TraderCharacterSource,
};
pub use order_plan::{OrderPlanCoordinator, OrderPlanError};
pub use planetary::{
    PlanetaryCharacter, PlanetaryCharacterSource, PlanetaryOverview, PlanetaryService,
};
pub use public_market::PublicMarketService;
pub use skills_catalogue::{
    SkillCatalogueCharacter, SkillCatalogueResponse, SkillCatalogueService, SkillsCharacter,
    SkillsCharacterSource,
};
pub use sync_metrics::init_metrics as init_sync_metrics;
pub use ticket_jobs::{IndustryJobsSynced, TicketJobError, TicketJobService};
