//! `ApiError` — the crate's single HTTP error type — with every
//! `From<DomainError>` conversion and the exhaustive `IntoResponse` mapping
//! that turns a domain error into an HTTP status plus JSON problem body.
//! Status codes, error codes, `retryable` flags, messages, and the response
//! shape are the API's contract, defined here and nowhere else.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use iskworks_core::order::OrderError;
use iskworks_core::{
    AppError, AuthError, BlueprintError, BuildPlanningError, ComponentExpansionError,
    FacilityError, FinanceError, IndustryError, InventoryError, MarketError, OpportunityError,
    ProductionError, ReactionPlanningError,
};
use iskworks_sde::SdeError;
use serde::Serialize;
use uuid::Uuid;

use iskworks_app::{
    BuildGraphError, BuildMaterialsError, BuildPreviewError, BuildWorksheetError,
    EsiApplicationError, ExecutionPlanError, OrderPlanError,
};

use crate::AuthApplicationError;

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<std::collections::BTreeMap<String, String>>,
    pub retryable: bool,
    /// Present only on responses whose `message` has been redacted to a
    /// curated string because the real error was an ISK Works
    /// implementation/infrastructure failure. It ties this response to the
    /// full diagnostic logged server-side (`tracing` target
    /// `iskworks_api::internal_error`). Never set for curated domain errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

pub enum ApiError {
    Application(AppError),
    StaticData(SdeError),
    StaticDataEntryNotFound,
    BuildPlanning(BuildPlanningError),
    ReactionPlanning(ReactionPlanningError),
    ComponentExpansion(ComponentExpansionError),
    Industry(IndustryError),
    Inventory(InventoryError),
    Production(ProductionError),
    Order(OrderError),
    Facility(FacilityError),
    Finance(FinanceError),
    Market(MarketError),
    Opportunity(OpportunityError),
    Integration(EsiApplicationError),
    Auth(AuthApplicationError),
    Unauthenticated,
    /// Signed in, but not an app admin (`ISKWORKS_ADMIN_CHARACTER_IDS`).
    Forbidden,
    /// A generic 404 for admin-addressed records that do not exist.
    NotFound,
    /// The request is well-formed but not allowed in the current state;
    /// the message is curated and safe to show.
    Conflict(&'static str),
    /// The Build Materials aggregate could not be projected: authoritative
    /// per-node quantities were unavailable for one or more Build-tree nodes.
    /// A curated 422 with no partial payload; the diagnostic build ids are
    /// logged at the `iskworks_app::BuildMaterialsError` -> `ApiError`
    /// boundary, never returned.
    BuildMaterialsIncomplete,
    /// A canonical root plan's persisted
    /// producer graph cannot be planned (never a partial plan). The detail
    /// is logged, never crossed to the client.
    CanonicalGraphInvalid,
    /// Stages descendant-configuration mutation: the requested member Build
    /// set no longer corresponds to exactly one current production
    /// operation under the live plan. A curated 409 -- the client should
    /// refetch the plan and let the user retry against the current
    /// operation, never silently apply the edit to a different member set.
    DescendantOperationMembershipStale,
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        Self::Application(value)
    }
}

impl From<SdeError> for ApiError {
    fn from(value: SdeError) -> Self {
        Self::StaticData(value)
    }
}

impl From<BuildPlanningError> for ApiError {
    fn from(value: BuildPlanningError) -> Self {
        Self::BuildPlanning(value)
    }
}

impl From<ReactionPlanningError> for ApiError {
    fn from(value: ReactionPlanningError) -> Self {
        Self::ReactionPlanning(value)
    }
}

impl From<ComponentExpansionError> for ApiError {
    fn from(value: ComponentExpansionError) -> Self {
        Self::ComponentExpansion(value)
    }
}

impl From<IndustryError> for ApiError {
    fn from(value: IndustryError) -> Self {
        Self::Industry(value)
    }
}

impl From<InventoryError> for ApiError {
    fn from(value: InventoryError) -> Self {
        Self::Inventory(value)
    }
}

impl From<OrderError> for ApiError {
    fn from(value: OrderError) -> Self {
        Self::Order(value)
    }
}

impl From<ProductionError> for ApiError {
    fn from(value: ProductionError) -> Self {
        Self::Production(value)
    }
}

impl From<FacilityError> for ApiError {
    fn from(value: FacilityError) -> Self {
        Self::Facility(value)
    }
}

impl From<FinanceError> for ApiError {
    fn from(value: FinanceError) -> Self {
        Self::Finance(value)
    }
}

impl From<MarketError> for ApiError {
    fn from(value: MarketError) -> Self {
        Self::Market(value)
    }
}

impl From<OpportunityError> for ApiError {
    fn from(value: OpportunityError) -> Self {
        Self::Opportunity(value)
    }
}

impl From<EsiApplicationError> for ApiError {
    fn from(value: EsiApplicationError) -> Self {
        Self::Integration(value)
    }
}

impl From<AuthApplicationError> for ApiError {
    fn from(value: AuthApplicationError) -> Self {
        Self::Auth(value)
    }
}

/// `BuildPreviewError` (application layer, `iskworks-app`) carries no HTTP
/// or message policy. This fan-out maps it to the `ApiError` the API
/// contract expects: the `#[from]` arms pass their domain error straight
/// through, and the three `*Unavailable` markers rebuild the same
/// `Persistence(..)` / `Configuration(..)` value -- including its message
/// string -- that `AppState`'s accessors produce.
impl From<BuildPreviewError> for ApiError {
    fn from(value: BuildPreviewError) -> Self {
        match value {
            BuildPreviewError::Industry(error) => Self::Industry(error),
            BuildPreviewError::ComponentExpansion(error) => Self::ComponentExpansion(error),
            BuildPreviewError::Production(error) => Self::Production(error),
            BuildPreviewError::Inventory(error) => Self::Inventory(error),
            BuildPreviewError::Esi(error) => Self::Integration(error),
            BuildPreviewError::StaticData(error) => Self::StaticData(error),
            BuildPreviewError::ProductionUnavailable => Self::Production(ProductionError::Persistence(
                "Production persistence is unavailable.".to_string(),
            )),
            BuildPreviewError::InventoryUnavailable => Self::Inventory(InventoryError::Persistence(
                "Inventory persistence is unavailable.".to_string(),
            )),
            BuildPreviewError::EsiUnavailable => Self::Integration(EsiApplicationError::Configuration(
                "EVE integration is not configured. Set EVE SSO credentials or enable explicit fixture mode.".to_string(),
            )),
        }
    }
}

/// `BuildGraphError` is a thin wrapper over exactly the
/// same `BuildMaterialsError` a `/materials` request can produce (Graph
/// shares its whole prelude with Materials -- the shared allocation-aware
/// walk, the shared cost projection). Fans out to the identical `ApiError`s.
impl From<BuildGraphError> for ApiError {
    fn from(value: BuildGraphError) -> Self {
        match value {
            BuildGraphError::Materials(error) => error.into(),
        }
    }
}

/// `ExecutionPlanError` is likewise a thin
/// wrapper over the same `BuildMaterialsError` `/materials`/`/graph` can
/// produce (the Execution Plan endpoint shares their whole prelude -- the
/// same allocation-aware walk, the same cost projection). Fans out to the
/// identical `ApiError`s -- ordinary incomplete cost evidence is never an
/// HTTP failure here, only a genuine `BuildMaterialsError` is.
impl From<ExecutionPlanError> for ApiError {
    fn from(value: ExecutionPlanError) -> Self {
        match value {
            ExecutionPlanError::Materials(error) => error.into(),
        }
    }
}

impl From<BuildWorksheetError> for ApiError {
    fn from(value: BuildWorksheetError) -> Self {
        match value {
            BuildWorksheetError::Materials(error) => error.into(),
            BuildWorksheetError::Projection(error) => match error {
                iskworks_core::build_worksheet::BuildWorksheetProjectionError::UnknownFocus => {
                    Self::Industry(IndustryError::Validation(
                        "the focused producer does not belong to this root plan".to_string(),
                    ))
                }
                iskworks_core::build_worksheet::BuildWorksheetProjectionError::MissingRoot => {
                    Self::Industry(IndustryError::Persistence(
                        "worksheet projection has no root operation".to_string(),
                    ))
                }
                iskworks_core::build_worksheet::BuildWorksheetProjectionError::QuantityOverflow
                | iskworks_core::build_worksheet::BuildWorksheetProjectionError::MoneyOverflow => {
                    Self::Industry(IndustryError::MoneyOverflow)
                }
            },
        }
    }
}

/// `BuildMaterialsError` (application layer, whole-tree inventory allocator).
/// `Preview` / `Industry` fan out to the same `ApiError`s a build-plan
/// preview / the Build Graph already produce. The two materials-specific
/// arms are curated: `MixedOwnerTree` is corrupt data (a linked child's
/// owner is immutable and inherited, so a spanning tree is impossible) and
/// maps to a redacted persistence failure; `ProjectionUnavailable` is a
/// deliberate 422 (`build_materials_incomplete`) whose diagnostic build ids
/// are logged here and never crossed to the client.
impl From<BuildMaterialsError> for ApiError {
    fn from(value: BuildMaterialsError) -> Self {
        match value {
            BuildMaterialsError::Preview(error) => error.into(),
            BuildMaterialsError::Industry(error) => Self::Industry(error),
            BuildMaterialsError::MixedOwnerTree(build_id) => {
                Self::Industry(IndustryError::Persistence(format!(
                    "build materials: linked tree node {} does not share the root owner/workspace",
                    build_id.0
                )))
            }
            BuildMaterialsError::ProjectionUnavailable(build_ids) => {
                tracing::warn!(
                    target: "iskworks_api::build_materials",
                    missing_build_ids = ?build_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                    "materials projection unavailable: authoritative per-node quantities missing"
                );
                Self::BuildMaterialsIncomplete
            }
            BuildMaterialsError::CanonicalGraphInvalid(error) => {
                tracing::error!(
                    target: "iskworks_api::build_materials",
                    error = %error,
                    "canonical production graph invalid"
                );
                Self::CanonicalGraphInvalid
            }
        }
    }
}

/// `OrderPlanError` (application layer, whole-tree Epic
/// freeze). `Materials` fans out to the same `ApiError`s a `/materials`
/// request already produces.
impl From<OrderPlanError> for ApiError {
    fn from(value: OrderPlanError) -> Self {
        match value {
            OrderPlanError::Materials(error) => error.into(),
            OrderPlanError::Preview(error) => error.into(),
            OrderPlanError::Freeze(error) => error.into(),
        }
    }
}

/// The Stages descendant-configuration mutation's own coordinator error.
/// `Materials`/`Industry` fan out to the same `ApiError`s a `/materials`
/// request or a single-Build settings patch already produce (including
/// `RevisionConflict` -> the existing 409 `revision_conflict`); the three
/// remaining arms are curated: `NoMembers`/`RootNotEditable` are ordinary
/// 400s (nothing was ever attempted), `StaleMembership` is the dedicated
/// 409 `descendant_operation_membership_stale` so the Stages inspector can
/// react to it specifically (refetch, do not retry the same edit blindly).
impl From<iskworks_app::DescendantProductionConfigurationError> for ApiError {
    fn from(value: iskworks_app::DescendantProductionConfigurationError) -> Self {
        use iskworks_app::DescendantProductionConfigurationError as Error;
        match value {
            Error::Materials(error) => error.into(),
            Error::Industry(error) => Self::Industry(error),
            Error::NoMembers => Self::Industry(IndustryError::Validation(
                "At least one member Build must be specified.".to_string(),
            )),
            Error::RootNotEditable => Self::Industry(IndustryError::Validation(
                "The root Build's own configuration is edited from Worksheet, not Stages."
                    .to_string(),
            )),
            Error::StaleMembership => Self::DescendantOperationMembershipStale,
        }
    }
}

fn facility_error_status(error: &FacilityError) -> (StatusCode, &'static str) {
    match error {
        FacilityError::NotFound => (StatusCode::NOT_FOUND, "facility_not_found"),
        FacilityError::Archived => (StatusCode::CONFLICT, "facility_archived"),
        FacilityError::RevisionConflict => (StatusCode::CONFLICT, "facility_revision_conflict"),
        FacilityError::Validation(_) | FacilityError::ArithmeticOverflow => {
            (StatusCode::BAD_REQUEST, "facility_calculation_failed")
        }
        FacilityError::Persistence(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "persistence_unavailable")
        }
    }
}

/// A public category for an error that represents an ISK Works
/// implementation or infrastructure failure rather than a deliberately
/// authored, actionable domain outcome. When `ApiError::into_response`
/// classifies an error into one of these, the client sees only
/// [`public_code`](InternalFailureCategory::public_code) /
/// [`public_message`](InternalFailureCategory::public_message) plus a
/// `correlationId`; the real error text is logged server-side against that
/// id and never crosses the wire.
///
/// There is intentionally one category today. `sqlx`/Postgres errors,
/// stored-row decode failures, and SDE read failures all surface as
/// `String`-carrying `Persistence` / `StaticData` variants whose `Display`
/// embeds raw driver text (`relation "..."`, `constraint "..."`), so they
/// share one contract. Add another variant here only when a genuinely
/// different public code/status/retryable tuple is warranted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalFailureCategory {
    /// Storage failure: database driver error, transaction failure, a
    /// stored row that no longer decodes, or an SDE read that hit the
    /// database. Retryable — a later attempt may land once the pool or
    /// database recovers.
    Persistence,
}

impl InternalFailureCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Persistence => "persistence",
        }
    }

    /// Stable public `code` substituted into the response body.
    fn public_code(self) -> &'static str {
        match self {
            Self::Persistence => "persistence_unavailable",
        }
    }

    /// Fixed, non-sensitive public `message`. Carries no SQL, schema,
    /// connection, or driver detail.
    fn public_message(self) -> &'static str {
        match self {
            Self::Persistence => "A storage error occurred. Please try again.",
        }
    }

    fn retryable(self) -> bool {
        match self {
            Self::Persistence => true,
        }
    }
}

fn persistence_failure(detail: impl Into<String>) -> Option<(InternalFailureCategory, String)> {
    Some((InternalFailureCategory::Persistence, detail.into()))
}

/// `IndustryError` reached through several `ApiError` arms (`Industry`,
/// `Opportunity`, `BuildGraph`). Classify its storage-failure variants once.
fn industry_internal_failure(error: &IndustryError) -> Option<(InternalFailureCategory, String)> {
    match error {
        IndustryError::Persistence(detail) => {
            persistence_failure(format!("industry persistence: {detail}"))
        }
        IndustryError::StaticData(detail) => {
            persistence_failure(format!("industry static-data read: {detail}"))
        }
        IndustryError::Market(MarketError::Persistence(detail)) => {
            persistence_failure(format!("industry market persistence: {detail}"))
        }
        IndustryError::Facility(FacilityError::Persistence(detail)) => {
            persistence_failure(format!("industry facility persistence: {detail}"))
        }
        _ => None,
    }
}

fn opportunity_internal_failure(
    error: &OpportunityError,
) -> Option<(InternalFailureCategory, String)> {
    match error {
        OpportunityError::StaticData(detail) => {
            persistence_failure(format!("opportunity static-data read: {detail}"))
        }
        OpportunityError::AdjustedPrices(detail) => {
            persistence_failure(format!("opportunity adjusted-price read: {detail}"))
        }
        OpportunityError::Market(MarketError::Persistence(detail)) => {
            persistence_failure(format!("opportunity market persistence: {detail}"))
        }
        OpportunityError::Facility(FacilityError::Persistence(detail)) => {
            persistence_failure(format!("opportunity facility persistence: {detail}"))
        }
        OpportunityError::Industry(inner) => industry_internal_failure(inner),
        _ => None,
    }
}

/// `EsiApplicationError::Persistence` wraps an `InventoryError`. Three of its
/// variants have deliberate cross-cutting HTTP mappings
/// (`revision_conflict`, `integration_record_not_found`, `validation_failed`)
/// and stay curated; every other value is a raw storage failure.
fn integration_internal_failure(
    error: &InventoryError,
) -> Option<(InternalFailureCategory, String)> {
    match error {
        InventoryError::RevisionConflict
        | InventoryError::ItemNotFound
        | InventoryError::Validation(_) => None,
        other => persistence_failure(format!("integration persistence: {other}")),
    }
}

impl ApiError {
    /// Classify this error as an internal implementation/infrastructure
    /// failure whose diagnostic text must not reach the client. `Some`
    /// means `into_response` will redact `code`/`message`/`retryable`,
    /// attach a `correlationId`, and log the returned detail string.
    /// `None` means the error is a curated domain outcome and passes
    /// through unchanged.
    fn internal_failure(&self) -> Option<(InternalFailureCategory, String)> {
        match self {
            Self::Application(AppError::Persistence(detail)) => {
                persistence_failure(format!("application persistence: {detail}"))
            }
            Self::StaticData(error) => persistence_failure(format!("static-data read: {error}")),
            Self::BuildPlanning(BuildPlanningError::StaticData(detail)) => {
                persistence_failure(format!("build-planning static-data read: {detail}"))
            }
            Self::ReactionPlanning(ReactionPlanningError::StaticData(detail)) => {
                persistence_failure(format!("reaction-planning static-data read: {detail}"))
            }
            Self::ComponentExpansion(ComponentExpansionError::StaticData(detail)) => {
                persistence_failure(format!("component-expansion static-data read: {detail}"))
            }
            Self::Industry(error) => industry_internal_failure(error),
            Self::Inventory(InventoryError::Persistence(detail)) => {
                persistence_failure(format!("inventory persistence: {detail}"))
            }
            Self::Production(ProductionError::Persistence(detail)) => {
                persistence_failure(format!("production persistence: {detail}"))
            }
            Self::Order(OrderError::Persistence(detail)) => {
                persistence_failure(format!("order persistence: {detail}"))
            }
            Self::Facility(FacilityError::Persistence(detail)) => {
                persistence_failure(format!("facility persistence: {detail}"))
            }
            Self::Finance(FinanceError::Persistence(detail)) => {
                persistence_failure(format!("finance persistence: {detail}"))
            }
            Self::Market(MarketError::Persistence(detail)) => {
                persistence_failure(format!("market persistence: {detail}"))
            }
            Self::Opportunity(error) => opportunity_internal_failure(error),
            Self::Integration(EsiApplicationError::Persistence(inner)) => {
                integration_internal_failure(inner)
            }
            Self::Auth(AuthApplicationError::Persistence(AuthError::Persistence(detail))) => {
                persistence_failure(format!("auth persistence: {detail}"))
            }
            _ => None,
        }
    }
}

/// Emit the single server-side diagnostic log for a redacted internal
/// failure and return the fresh correlation id shared with the client. The
/// id is created here exactly once per failing response, so the value in
/// the log line and the value in the response body cannot diverge.
///
/// Deliberately logs only `detail` (the domain error's own text, already
/// free of secrets — repositories build it from `sqlx::Error::to_string()`,
/// which never contains the connection string, password, or token key) plus
/// the safe category/status. No headers, cookies, tokens, or request body.
fn log_internal_failure(
    category: InternalFailureCategory,
    http_status: StatusCode,
    detail: &str,
) -> Uuid {
    let correlation_id = Uuid::new_v4();
    tracing::error!(
        target: "iskworks_api::internal_error",
        correlation_id = %correlation_id,
        category = category.as_str(),
        http_status = http_status.as_u16(),
        detail = %detail,
        "internal error redacted from API response"
    );
    correlation_id
}

/// Sanitize a domain error for inline reporting inside a `2xx` bulk-outcome
/// DTO — the per-item `error` / `message` fields of sync and import
/// responses, which never pass through `ApiError::into_response`. A curated
/// domain error keeps its authored text; an internal/persistence failure
/// collapses to the fixed opaque message with an `Error ID:` suffix and is
/// logged server-side, exactly like the `ApiError` response boundary.
pub fn public_outcome_message<E>(error: E) -> String
where
    E: std::fmt::Display + Into<ApiError>,
{
    let rendered = error.to_string();
    match error.into().internal_failure() {
        Some((category, detail)) => {
            let correlation_id =
                log_internal_failure(category, StatusCode::SERVICE_UNAVAILABLE, &detail);
            format!("{} (Error ID: {correlation_id})", category.public_message())
        }
        None => rendered,
    }
}

mod industry;
mod integration;
mod inventory;
mod market;
mod opportunity;
mod order;
use industry::*;
use integration::*;
use inventory::*;
use market::*;
use opportunity::*;
use order::*;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Classify before `self` is moved into the match: an internal
        // implementation failure must never ship its diagnostic text.
        let internal = self.internal_failure();
        let (status, mut body) = match self {
            Self::Application(AppError::Validation(errors)) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "validation_failed",
                    message: "Validation failed.".to_string(),
                    fields: Some(errors.fields),
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::Application(AppError::WorkspaceAlreadyConfigured) => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "workspace_already_configured",
                    message: "Workspace is already configured.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::Application(AppError::Persistence(message)) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "persistence_unavailable",
                    message,
                    fields: None,
                    retryable: true,
                    correlation_id: None,
                },
            ),
            Self::StaticData(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "static_data_unavailable",
                    message: error.to_string(),
                    fields: None,
                    retryable: true,
                    correlation_id: None,
                },
            ),
            Self::StaticDataEntryNotFound => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "static_data_entry_not_found",
                    message: "Manufacturing modifiers were not found in the active SDE."
                        .to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::BuildPlanning(BuildPlanningError::InvalidRuns) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "invalid_runs",
                    message: "Runs must be between 1 and 1,000,000.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::BuildPlanning(BuildPlanningError::BlueprintNotFound(_)) => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "blueprint_not_found",
                    message: "Manufacturing blueprint was not found in the active SDE.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::BuildPlanning(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "build_planning_unavailable",
                    message: error.to_string(),
                    fields: None,
                    retryable: true,
                    correlation_id: None,
                },
            ),
            Self::ReactionPlanning(ReactionPlanningError::InvalidRuns) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "invalid_runs",
                    message: "Runs must be between 1 and 1,000,000.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::ReactionPlanning(ReactionPlanningError::ReactionFormulaNotFound(_)) => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "reaction_formula_not_found",
                    message: "Reaction formula was not found in the active SDE.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::ReactionPlanning(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "reaction_planning_unavailable",
                    message: error.to_string(),
                    fields: None,
                    retryable: true,
                    correlation_id: None,
                },
            ),
            Self::ComponentExpansion(ComponentExpansionError::InvalidRuns) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "invalid_runs",
                    message: "Runs must be between 1 and 1,000,000.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::ComponentExpansion(ComponentExpansionError::RecipeNotFound) => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "component_recipe_not_found",
                    message: "No active recipe was found for a requested component resolution."
                        .to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::ComponentExpansion(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "component_expansion_unavailable",
                    message: error.to_string(),
                    fields: None,
                    retryable: true,
                    correlation_id: None,
                },
            ),
            Self::Industry(error) => industry_response(error),
            Self::Inventory(error) => inventory_response(error),
            Self::Production(error) => production_response(error),
            Self::Order(error) => order_response(error),
            Self::Facility(error) => facility_response(error),
            Self::Opportunity(error) => opportunity_response(error),
            Self::Finance(error) => finance_response(error),
            Self::Market(error) => market_response(error),
            Self::Integration(error) => integration_response(error),
            Self::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                ErrorBody {
                    code: "unauthenticated",
                    message: "Sign in with EVE Online to continue.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::Conflict(message) => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "conflict",
                    message: message.to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "not_found",
                    message: "Not found.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                ErrorBody {
                    code: "forbidden",
                    message: "You do not have access to this area.".to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::BuildMaterialsIncomplete => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorBody {
                    code: "build_materials_incomplete",
                    message: "The materials breakdown is unavailable for this build right now \
                              because one or more linked builds need attention. Open the \
                              affected linked builds, then try again."
                        .to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::CanonicalGraphInvalid => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "canonical_graph_invalid",
                    message: "This plan's production graph is inconsistent (a cycle, a retired \
                              producer, or duplicate producers) and cannot be calculated."
                        .to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::DescendantOperationMembershipStale => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "descendant_operation_membership_stale",
                    message: "This production operation has changed since it was loaded. \
                              Refresh Stages and try the edit again."
                        .to_string(),
                    fields: None,
                    retryable: false,
                    correlation_id: None,
                },
            ),
            Self::Auth(error) => auth_response(error),
        };

        // A curated domain error passes through untouched. An internal
        // failure keeps its HTTP status (still 5xx) but its body is
        // replaced with the fixed public contract plus a correlation id
        // that ties it to one server-side log line.
        if let Some((category, detail)) = internal {
            let correlation_id = log_internal_failure(category, status, &detail);
            body.code = category.public_code();
            body.message = category.public_message().to_string();
            body.fields = None;
            body.retryable = category.retryable();
            body.correlation_id = Some(correlation_id.to_string());
        }

        (status, Json(ErrorEnvelope { error: body })).into_response()
    }
}

#[cfg(test)]
mod tests;
