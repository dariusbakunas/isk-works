use super::*;
use axum::body::to_bytes;

/// Serializes the tests that emit an internal-failure `tracing` event.
/// `the_body_correlation_id_matches_the_single_server_side_log_line`
/// installs a *scoped* subscriber and asserts on captured output, but
/// `tracing`'s per-callsite interest cache is process-global: a
/// concurrent emit from one of the sibling tests (which run with no
/// subscriber) can cache `Interest::never()` for that callsite and make
/// the scoped capture miss its own event. Holding this lock for each such
/// test removes the race. (Pre-existing flake, unrelated to any one
/// feature — surfaces whenever the test binary gains more parallel work.)
static INTERNAL_LOG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The guard is intentionally held for the whole test body (which
/// `.await`s only on inert response-body readers) to keep the emit
/// serialized against the sibling tests.
macro_rules! hold_internal_log_lock {
    () => {
        let _log_guard = INTERNAL_LOG_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    };
}

#[tokio::test]
async fn opportunity_errors_preserve_facility_and_blueprint_http_semantics() {
    let archived = ApiError::Opportunity(OpportunityError::Industry(IndustryError::Facility(
        FacilityError::Archived,
    )))
    .into_response();
    assert_eq!(archived.status(), StatusCode::CONFLICT);
    let body = to_bytes(archived.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
        "facility_archived"
    );

    let invalid_me = ApiError::Opportunity(OpportunityError::Blueprint(
        BlueprintError::InvalidMaterialEfficiency,
    ))
    .into_response();
    assert_eq!(invalid_me.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(invalid_me.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
        "validation_failed"
    );
}

#[tokio::test]
async fn a_throttled_manual_sync_is_a_retryable_429_with_its_message() {
    let response = ApiError::Integration(EsiApplicationError::SyncTooSoon {
        retry_after_seconds: 42,
    })
    .into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let error = &serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"];
    assert_eq!(error["code"], "sync_too_soon");
    assert_eq!(error["retryable"], true);
    assert!(error["message"].as_str().unwrap().contains("42s"));
}

async fn response_code(response: Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"].clone()
}

/// A corrupt canonical graph or a
/// non-conserving freeze is a typed 422, never a generic failure; a
/// version-3 producer ticket that is gone is a typed 409.
#[tokio::test]
async fn canonical_epic_freeze_errors_are_typed() {
    for (error, status, code) in [
        (
            ApiError::from(OrderPlanError::Freeze(OrderError::CorruptProductionGraph {
                detail: "the operation graph has a cycle".to_string(),
            })),
            StatusCode::UNPROCESSABLE_ENTITY,
            "corrupt_production_graph",
        ),
        (
            ApiError::from(OrderPlanError::Freeze(OrderError::FrozenCostNotConserved {
                occurrence_key: "build:x".to_string(),
            })),
            StatusCode::UNPROCESSABLE_ENTITY,
            "frozen_cost_not_conserved",
        ),
        (
            ApiError::from(OrderError::FrozenProducerTicketUnavailable {
                occurrence_key: "build:x".to_string(),
            }),
            StatusCode::CONFLICT,
            "frozen_producer_ticket_unavailable",
        ),
    ] {
        let response = error.into_response();
        assert_eq!(response.status(), status);
        assert_eq!(response_code(response).await, code);
    }
}

#[tokio::test]
async fn ticket_recording_reversal_errors_have_stable_http_contracts() {
    for (error, status, code) in [
        (
            OrderError::RecordingNotFound,
            StatusCode::NOT_FOUND,
            "recording_not_found",
        ),
        (
            OrderError::RecordingAlreadyReversed,
            StatusCode::CONFLICT,
            "recording_already_reversed",
        ),
        (
            OrderError::RecordingEvidenceInvalid,
            StatusCode::CONFLICT,
            "recording_evidence_invalid",
        ),
        (
            OrderError::RecordingReversalInvalid,
            StatusCode::CONFLICT,
            "recording_reversal_invalid",
        ),
    ] {
        let response = ApiError::from(error).into_response();
        assert_eq!(response.status(), status);
        assert_eq!(response_code(response).await, code);
    }
}

#[tokio::test]
async fn build_preview_unavailable_markers_map_to_the_exact_legacy_api_error() {
    // The coordinator (iskworks-app) never models "dependency not wired
    // into API state"; the API layer maps it to the same response
    // `AppState`'s accessors produce. These assertions pin that contract.
    let production = ApiError::from(BuildPreviewError::ProductionUnavailable);
    assert!(matches!(
        &production,
        ApiError::Production(ProductionError::Persistence(message))
            if message == "Production persistence is unavailable."
    ));
    let response = production.into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response_code(response).await, "persistence_unavailable");

    let inventory = ApiError::from(BuildPreviewError::InventoryUnavailable);
    assert!(matches!(
        &inventory,
        ApiError::Inventory(InventoryError::Persistence(message))
            if message == "Inventory persistence is unavailable."
    ));
    let response = inventory.into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response_code(response).await, "persistence_unavailable");

    let esi = ApiError::from(BuildPreviewError::EsiUnavailable);
    assert!(matches!(
        &esi,
        ApiError::Integration(EsiApplicationError::Configuration(message))
            if message
                == "EVE integration is not configured. Set EVE SSO credentials or enable explicit fixture mode."
    ));
    let response = esi.into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response_code(response).await, "esi_not_configured");
}

#[tokio::test]
async fn build_preview_domain_errors_pass_straight_through() {
    let invalid_recipe =
        ApiError::from(BuildPreviewError::Industry(IndustryError::InvalidRecipe)).into_response();
    assert_eq!(invalid_recipe.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response_code(invalid_recipe).await, "validation_failed");

    let component_recipe = ApiError::from(BuildPreviewError::ComponentExpansion(
        ComponentExpansionError::RecipeNotFound,
    ))
    .into_response();
    assert_eq!(component_recipe.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_code(component_recipe).await,
        "component_recipe_not_found"
    );
}

// ---- Curated public errors + internal redaction ----

/// Implementation text an internal error must never carry to the client.
const SENSITIVE_MARKERS: [&str; 6] = [
    "secret_internal_table",
    "price_snapshots_captured_source_revision_check",
    "relation",
    "constraint",
    "sqlx",
    "postgres",
];

async fn response_json(response: Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn assert_body_has_no_leak(body: &serde_json::Value) {
    let error = &body["error"];
    // Inspect the client-visible free-text fields. (The whole-body
    // string would false-positive on the `correlationId` key, which
    // contains the substring "relation".)
    let message = error["message"].as_str().unwrap_or_default().to_lowercase();
    let code = error["code"].as_str().unwrap_or_default().to_lowercase();
    for marker in SENSITIVE_MARKERS {
        assert!(
            !message.contains(marker),
            "message leaked {marker:?}: {body}"
        );
        assert!(!code.contains(marker), "code leaked {marker:?}: {body}");
    }
    assert!(
        error["fields"].is_null(),
        "a redacted internal failure must not carry field detail: {body}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_persistence_error_is_redacted_on_the_real_into_response_path() {
    hold_internal_log_lock!();
    let raw = "error returned from database: new row for relation \"price_snapshots\" \
             violates check constraint \"price_snapshots_captured_source_revision_check\"";
    let response = ApiError::Order(OrderError::Persistence(raw.to_string())).into_response();

    assert!(
        response.status().is_server_error(),
        "persistence failures stay 5xx"
    );
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "persistence_unavailable");
    assert_eq!(
        body["error"]["message"],
        "A storage error occurred. Please try again."
    );
    assert_eq!(body["error"]["retryable"], true);
    let correlation_id = body["error"]["correlationId"].as_str().unwrap();
    assert!(
        Uuid::parse_str(correlation_id).is_ok(),
        "correlationId is a UUID: {correlation_id}"
    );
    assert_body_has_no_leak(&body);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn every_internal_failure_variant_redacts_and_keeps_5xx() {
    hold_internal_log_lock!();
    let raw = "relation \"secret_internal_table\" does not exist (constraint sqlx postgres)";
    let cases: Vec<(&str, ApiError)> = vec![
        (
            "application",
            ApiError::Application(AppError::Persistence(raw.into())),
        ),
        (
            "industry",
            ApiError::Industry(IndustryError::Persistence(raw.into())),
        ),
        (
            "industry static-data",
            ApiError::Industry(IndustryError::StaticData(raw.into())),
        ),
        (
            "industry market",
            ApiError::Industry(IndustryError::Market(MarketError::Persistence(raw.into()))),
        ),
        (
            "inventory",
            ApiError::Inventory(InventoryError::Persistence(raw.into())),
        ),
        (
            "production",
            ApiError::Production(ProductionError::Persistence(raw.into())),
        ),
        (
            "order",
            ApiError::Order(OrderError::Persistence(raw.into())),
        ),
        (
            "facility",
            ApiError::Facility(FacilityError::Persistence(raw.into())),
        ),
        (
            "finance",
            ApiError::Finance(FinanceError::Persistence(raw.into())),
        ),
        (
            "market",
            ApiError::Market(MarketError::Persistence(raw.into())),
        ),
        (
            "sde read",
            ApiError::StaticData(SdeError::Storage(raw.into())),
        ),
        (
            "build planning",
            ApiError::BuildPlanning(BuildPlanningError::StaticData(raw.into())),
        ),
        (
            "reaction planning",
            ApiError::ReactionPlanning(ReactionPlanningError::StaticData(raw.into())),
        ),
        (
            "component expansion",
            ApiError::ComponentExpansion(ComponentExpansionError::StaticData(raw.into())),
        ),
        (
            "opportunity adjusted price",
            ApiError::Opportunity(OpportunityError::AdjustedPrices(raw.into())),
        ),
        (
            "opportunity static data",
            ApiError::Opportunity(OpportunityError::StaticData(raw.into())),
        ),
        (
            "opportunity -> industry",
            ApiError::Opportunity(OpportunityError::Industry(IndustryError::Persistence(
                raw.into(),
            ))),
        ),
        (
            "integration persistence",
            ApiError::Integration(EsiApplicationError::Persistence(
                InventoryError::Persistence(raw.into()),
            )),
        ),
        (
            "auth persistence",
            ApiError::Auth(AuthApplicationError::Persistence(AuthError::Persistence(
                raw.into(),
            ))),
        ),
    ];

    for (label, error) in cases {
        let response = error.into_response();
        assert!(
            response.status().is_server_error(),
            "{label}: expected 5xx, got {}",
            response.status()
        );
        let body = response_json(response).await;
        assert_eq!(
            body["error"]["code"], "persistence_unavailable",
            "{label}: {body}"
        );
        assert_eq!(
            body["error"]["message"], "A storage error occurred. Please try again.",
            "{label}"
        );
        assert!(
            body["error"]["correlationId"].is_string(),
            "{label}: correlationId present"
        );
        assert_body_has_no_leak(&body);
    }
}

#[tokio::test]
async fn curated_domain_errors_pass_through_without_a_correlation_id() {
    let cases: Vec<(ApiError, StatusCode, &str)> = vec![
        (
            ApiError::BuildPlanning(BuildPlanningError::InvalidRuns),
            StatusCode::BAD_REQUEST,
            "invalid_runs",
        ),
        (
            ApiError::Industry(IndustryError::InvalidRecipe),
            StatusCode::BAD_REQUEST,
            "validation_failed",
        ),
        (
            ApiError::Order(OrderError::OrderNotFound),
            StatusCode::NOT_FOUND,
            "order_not_found",
        ),
        (
            ApiError::Application(AppError::WorkspaceAlreadyConfigured),
            StatusCode::CONFLICT,
            "workspace_already_configured",
        ),
        (
            ApiError::Unauthenticated,
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
        ),
        (ApiError::Forbidden, StatusCode::FORBIDDEN, "forbidden"),
        (ApiError::NotFound, StatusCode::NOT_FOUND, "not_found"),
        (ApiError::Conflict("no"), StatusCode::CONFLICT, "conflict"),
    ];

    for (error, expected_status, expected_code) in cases {
        let response = error.into_response();
        assert_eq!(response.status(), expected_status, "{expected_code}");
        let body = response_json(response).await;
        assert_eq!(body["error"]["code"], expected_code);
        assert!(
            body["error"].get("correlationId").is_none()
                || body["error"]["correlationId"].is_null(),
            "curated error must not carry a correlationId: {body}"
        );
        // The authored message survives unchanged (spot-check two).
        if expected_code == "workspace_already_configured" {
            assert_eq!(body["error"]["message"], "Workspace is already configured.");
        }
        if expected_code == "unauthenticated" {
            assert_eq!(
                body["error"]["message"],
                "Sign in with EVE Online to continue."
            );
        }
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn the_body_correlation_id_matches_the_single_server_side_log_line() {
    hold_internal_log_lock!();
    #[derive(Clone, Default)]
    struct Buffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Buffer {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for Buffer {
        type Writer = Buffer;
        fn make_writer(&'writer self) -> Self::Writer {
            self.clone()
        }
    }

    let buffer = Buffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::ERROR)
        .finish();

    let response = tracing::subscriber::with_default(subscriber, || {
        ApiError::Market(MarketError::Persistence(
            "relation \"secret_internal_table\" does not exist".to_string(),
        ))
        .into_response()
    });

    let body = response_json(response).await;
    let correlation_id = body["error"]["correlationId"].as_str().unwrap().to_string();

    let logged = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        logged
            .matches("internal error redacted from API response")
            .count(),
        1,
        "exactly one diagnostic log line per failing response: {logged}"
    );
    assert!(
        logged.contains(&correlation_id),
        "log line carries the same correlation id the client got: {logged}"
    );
    assert!(
        logged.contains("secret_internal_table"),
        "full diagnostic detail is preserved server-side: {logged}"
    );
    assert!(
        logged.contains("iskworks_api::internal_error"),
        "diagnostic is on the dedicated tracing target: {logged}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn bulk_outcome_messages_redact_internal_failures_but_keep_curated_ones() {
    hold_internal_log_lock!();
    // Internal failure -> opaque + Error ID.
    let redacted = public_outcome_message(InventoryError::Persistence(
        "relation \"secret_internal_table\" does not exist".to_string(),
    ));
    assert!(redacted.starts_with("A storage error occurred. Please try again. (Error ID: "));
    for marker in SENSITIVE_MARKERS {
        assert!(
            !redacted.to_lowercase().contains(marker),
            "leaked {marker:?}: {redacted}"
        );
    }

    // Curated domain error -> the domain error's own text, exactly as the
    // call site rendered it, no Error ID.
    let curated = public_outcome_message(InventoryError::Validation(
        "Structure ID must be positive.".to_string(),
    ));
    assert_eq!(curated, "validation failed: Structure ID must be positive.");
    assert!(!curated.contains("Error ID"));
}
