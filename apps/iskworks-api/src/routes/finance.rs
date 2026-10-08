use std::collections::HashSet;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use iskworks_core::{
    ConnectedCharacterId, EsiSyncKind, FinanceColumn, FinanceDirection, FinanceError,
    FinanceInventoryRecording, FinanceSortColumn, FinanceTransactionFilter, FinanceTransactionPage,
    FinanceTransactionSort, FinanceTransactionType, InventoryPreview, SavedFinanceFilter,
    SavedFinanceFilterId, SortDirection,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::csv_cell::safe_cell;
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/finance/transactions", get(list_transactions))
        .route("/api/finance/transactions/export", get(export_transactions))
        .route(
            "/api/finance/transactions/:observation_id/inventory-recording",
            post(record_inventory),
        )
        .route(
            "/api/finance/transactions/:observation_id/inventory-recording/preview",
            post(preview_inventory_recording),
        )
        .route(
            "/api/finance/transactions/:observation_id/inventory-recording/:recording_id/revert",
            post(revert_inventory_recording),
        )
        .route("/api/finance/sync", post(sync_finance))
        .route(
            "/api/finance/saved-filters",
            get(list_saved_filters).post(save_filter),
        )
        .route(
            "/api/finance/saved-filters/:filter_id",
            delete(delete_filter),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionQuery {
    connection_ids: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    search: Option<String>,
    transaction_types: Option<String>,
    direction: Option<String>,
    category: Option<String>,
    location_id: Option<i64>,
    type_id: Option<i64>,
    exclude_inventory_buys: Option<bool>,
    sort: Option<String>,
    order: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
    columns: Option<String>,
}

impl TransactionQuery {
    fn domain(&self) -> Result<(FinanceTransactionFilter, FinanceTransactionSort), FinanceError> {
        let mut filter = FinanceTransactionFilter::default();
        filter.connection_ids = parse_ids(self.connection_ids.as_deref())?;
        filter.date_from = parse_date(self.date_from.as_deref(), "dateFrom")?;
        filter.date_to = parse_date(self.date_to.as_deref(), "dateTo")?;
        filter.search = self.search.clone();
        filter.transaction_types = parse_transaction_types(self.transaction_types.as_deref())?;
        filter.direction = parse_direction(self.direction.as_deref())?;
        filter.category = self.category.clone();
        filter.location_id = self.location_id;
        filter.type_id = self.type_id;
        filter.exclude_inventory_buys = self.exclude_inventory_buys.unwrap_or(false);
        filter.page = self.page.unwrap_or(filter.page);
        filter.page_size = self.page_size.unwrap_or(filter.page_size);
        let sort = FinanceTransactionSort {
            column: parse_sort(self.sort.as_deref())?,
            direction: parse_order(self.order.as_deref())?,
        };
        Ok((filter.validate()?, sort))
    }
}

async fn list_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Json<FinanceTransactionPage>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let (filter, sort) = query.domain()?;
    Ok(Json(
        state
            .finance_repository()?
            .transactions(workspace_id, filter, sort)
            .await?,
    ))
}

async fn export_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionQuery>,
) -> Result<Response<Body>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let (filter, sort) = query.domain()?;
    let columns = parse_columns(query.columns.as_deref())?;
    let page = state
        .finance_repository()?
        .transactions(workspace_id, filter, sort)
        .await?;
    let csv = finance_csv(&page, &columns)?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=isk-works-transactions.csv"),
        )
        .body(Body::from(csv))
        .expect("static Finance response headers are valid"))
}

/// Adds one Market Buy to accounting Inventory. The body is empty on purpose:
/// type, quantity, basis and owner are all derived server-side from the stored
/// transaction. `201` when a purchase was posted, `200` when the transaction
/// was already recorded (a retry, second tab, or the import review got there
/// first) and the existing recording is returned.
async fn record_inventory(
    State(state): State<AppState>,
    Path(observation_id): Path<Uuid>,
) -> Result<(StatusCode, Json<FinanceInventoryRecording>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let (recording, created) = state
        .esi_repository()?
        .record_wallet_purchase(workspace_id, observation_id)
        .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(recording)))
}

/// What recording this Market Buy would do to Inventory right now, derived
/// exactly as `record_inventory` derives it. Writes nothing.
async fn preview_inventory_recording(
    State(state): State<AppState>,
    Path(observation_id): Path<Uuid>,
) -> Result<Json<InventoryPreview>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .esi_repository()?
            .preview_wallet_purchase(workspace_id, observation_id)
            .await?,
    ))
}

async fn revert_inventory_recording(
    State(state): State<AppState>,
    Path((observation_id, recording_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<FinanceInventoryRecording>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .esi_repository()?
            .revert_wallet_purchase_recording(workspace_id, observation_id, recording_id)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveFilterRequest {
    name: String,
    filter: FinanceTransactionFilter,
}

async fn list_saved_filters(
    State(state): State<AppState>,
) -> Result<Json<Vec<SavedFinanceFilter>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .finance_repository()?
            .saved_filters(workspace_id)
            .await?,
    ))
}

async fn save_filter(
    State(state): State<AppState>,
    Json(request): Json<SaveFilterRequest>,
) -> Result<(StatusCode, Json<SavedFinanceFilter>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let saved = state
        .finance_repository()?
        .save_filter(workspace_id, &request.name, request.filter)
        .await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

async fn delete_filter(
    State(state): State<AppState>,
    Path(filter_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .finance_repository()?
        .delete_filter(workspace_id, SavedFinanceFilterId(filter_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FinanceSyncRequest {
    connection_ids: Vec<ConnectedCharacterId>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceSyncOutcome {
    connection_id: ConnectedCharacterId,
    character_name: String,
    succeeded: bool,
    runs: Vec<iskworks_core::EsiSyncRun>,
    error: Option<String>,
}

async fn sync_finance(
    State(state): State<AppState>,
    Json(request): Json<FinanceSyncRequest>,
) -> Result<Json<Vec<FinanceSyncOutcome>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let service = state.esi_service()?;
    if service.fixture_mode() {
        return Err(FinanceError::Validation(
            "Finance synchronization requires real EVE connections; fixture mode is not supported."
                .to_string(),
        )
        .into());
    }
    let connections = state
        .esi_repository()?
        .list_connections(workspace_id)
        .await?;
    let requested = if request.connection_ids.is_empty() {
        connections.iter().map(|connection| connection.id).collect()
    } else {
        request.connection_ids.into_iter().collect::<HashSet<_>>()
    };
    let known = connections
        .iter()
        .map(|connection| connection.id)
        .collect::<HashSet<_>>();
    if !requested.is_subset(&known) {
        return Err(FinanceError::Validation(
            "One or more selected characters do not belong to this workspace.".to_string(),
        )
        .into());
    }
    let mut outcomes = Vec::new();
    for connection in connections
        .into_iter()
        .filter(|connection| requested.contains(&connection.id))
    {
        match service
            .sync(connection.id, EsiSyncKind::WalletTransactions)
            .await
        {
            Ok(runs) => outcomes.push(FinanceSyncOutcome {
                connection_id: connection.id,
                character_name: connection.character_name,
                succeeded: true,
                runs,
                error: None,
            }),
            Err(error) => outcomes.push(FinanceSyncOutcome {
                connection_id: connection.id,
                character_name: connection.character_name,
                succeeded: false,
                runs: Vec::new(),
                error: Some(crate::error::public_outcome_message(error)),
            }),
        }
    }
    Ok(Json(outcomes))
}

fn finance_csv(
    page: &FinanceTransactionPage,
    columns: &[FinanceColumn],
) -> Result<Vec<u8>, FinanceError> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(columns.iter().map(column_label))
        .map_err(|error| FinanceError::Persistence(error.to_string()))?;
    for row in &page.rows {
        writer
            .write_record(
                columns
                    .iter()
                    .map(|column| safe_cell(&column_value(*column, row)).into_owned()),
            )
            .map_err(|error| FinanceError::Persistence(error.to_string()))?;
    }
    writer
        .into_inner()
        .map_err(|error| FinanceError::Persistence(error.to_string()))
}

fn column_label(column: &FinanceColumn) -> &'static str {
    match column {
        FinanceColumn::Time => "Time",
        FinanceColumn::Character => "Character",
        FinanceColumn::TransactionType => "Transaction Type",
        FinanceColumn::Item => "Item",
        FinanceColumn::Quantity => "Quantity",
        FinanceColumn::UnitPrice => "Unit Price",
        FinanceColumn::TotalPrice => "Total Price",
        FinanceColumn::Direction => "Direction",
        FinanceColumn::Counterparty => "Client",
        FinanceColumn::Location => "Where",
        FinanceColumn::Region => "Region",
    }
}

fn column_value(column: FinanceColumn, row: &iskworks_core::FinanceTransaction) -> String {
    match column {
        FinanceColumn::Time => row.transacted_at.to_rfc3339(),
        FinanceColumn::Character => row.character_name.clone(),
        FinanceColumn::TransactionType => match row.transaction_type {
            FinanceTransactionType::MarketBuy => "Market Buy".to_string(),
            FinanceTransactionType::MarketSell => "Market Sell".to_string(),
        },
        FinanceColumn::Item => row.type_name.clone(),
        FinanceColumn::Quantity => row.quantity.to_string(),
        FinanceColumn::UnitPrice => row.unit_price.0.to_string(),
        FinanceColumn::TotalPrice => row.total_price.0.to_string(),
        FinanceColumn::Direction => match row.direction() {
            FinanceDirection::Income => "Income".to_string(),
            FinanceDirection::Expense => "Expense".to_string(),
            FinanceDirection::All => unreachable!("a transaction always has a concrete direction"),
        },
        FinanceColumn::Counterparty => row.counterparty_name.clone().unwrap_or_default(),
        FinanceColumn::Location => row.location_name.clone().unwrap_or_default(),
        FinanceColumn::Region => row.region_name.clone().unwrap_or_default(),
    }
}

pub(super) fn parse_ids(value: Option<&str>) -> Result<Vec<ConnectedCharacterId>, FinanceError> {
    split_values(value)
        .map(|value| {
            Uuid::parse_str(value)
                .map(ConnectedCharacterId)
                .map_err(|_| {
                    FinanceError::Validation("connectionIds contains an invalid UUID".to_string())
                })
        })
        .collect()
}

pub(super) fn parse_date(
    value: Option<&str>,
    field: &str,
) -> Result<Option<NaiveDate>, FinanceError> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| FinanceError::Validation(format!("{field} must use YYYY-MM-DD")))
        })
        .transpose()
}

pub(super) fn parse_transaction_types(
    value: Option<&str>,
) -> Result<Vec<FinanceTransactionType>, FinanceError> {
    if value.is_none() {
        return Ok(vec![
            FinanceTransactionType::MarketBuy,
            FinanceTransactionType::MarketSell,
        ]);
    }
    split_values(value)
        .map(|value| match value {
            "marketBuy" => Ok(FinanceTransactionType::MarketBuy),
            "marketSell" => Ok(FinanceTransactionType::MarketSell),
            _ => Err(FinanceError::Validation(format!(
                "unsupported transaction type: {value}"
            ))),
        })
        .collect()
}

pub(super) fn parse_direction(value: Option<&str>) -> Result<FinanceDirection, FinanceError> {
    match value.unwrap_or("all") {
        "all" => Ok(FinanceDirection::All),
        "income" => Ok(FinanceDirection::Income),
        "expense" => Ok(FinanceDirection::Expense),
        value => Err(FinanceError::Validation(format!(
            "unsupported direction: {value}"
        ))),
    }
}

fn parse_sort(value: Option<&str>) -> Result<FinanceSortColumn, FinanceError> {
    match value.unwrap_or("time") {
        "time" => Ok(FinanceSortColumn::Time),
        "character" => Ok(FinanceSortColumn::Character),
        "transactionType" => Ok(FinanceSortColumn::TransactionType),
        "item" => Ok(FinanceSortColumn::Item),
        "quantity" => Ok(FinanceSortColumn::Quantity),
        "unitPrice" => Ok(FinanceSortColumn::UnitPrice),
        "totalPrice" => Ok(FinanceSortColumn::TotalPrice),
        "direction" => Ok(FinanceSortColumn::Direction),
        "counterparty" => Ok(FinanceSortColumn::Counterparty),
        "location" => Ok(FinanceSortColumn::Location),
        "region" => Ok(FinanceSortColumn::Region),
        value => Err(FinanceError::Validation(format!(
            "unsupported sort column: {value}"
        ))),
    }
}

fn parse_order(value: Option<&str>) -> Result<SortDirection, FinanceError> {
    match value.unwrap_or("desc") {
        "asc" => Ok(SortDirection::Asc),
        "desc" => Ok(SortDirection::Desc),
        value => Err(FinanceError::Validation(format!(
            "unsupported sort order: {value}"
        ))),
    }
}

fn parse_columns(value: Option<&str>) -> Result<Vec<FinanceColumn>, FinanceError> {
    let values = split_values(value).collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(vec![
            FinanceColumn::Time,
            FinanceColumn::Character,
            FinanceColumn::TransactionType,
            FinanceColumn::Item,
            FinanceColumn::Quantity,
            FinanceColumn::UnitPrice,
            FinanceColumn::TotalPrice,
            FinanceColumn::Direction,
            FinanceColumn::Counterparty,
            FinanceColumn::Location,
            FinanceColumn::Region,
        ]);
    }
    values
        .into_iter()
        .map(|value| match value {
            "time" => Ok(FinanceColumn::Time),
            "character" => Ok(FinanceColumn::Character),
            "transactionType" => Ok(FinanceColumn::TransactionType),
            "item" => Ok(FinanceColumn::Item),
            "quantity" => Ok(FinanceColumn::Quantity),
            "unitPrice" => Ok(FinanceColumn::UnitPrice),
            "totalPrice" => Ok(FinanceColumn::TotalPrice),
            "direction" => Ok(FinanceColumn::Direction),
            "counterparty" => Ok(FinanceColumn::Counterparty),
            "location" => Ok(FinanceColumn::Location),
            "region" => Ok(FinanceColumn::Region),
            _ => Err(FinanceError::Validation(format!(
                "unsupported export column: {value}"
            ))),
        })
        .collect()
}

fn split_values(value: Option<&str>) -> impl Iterator<Item = &str> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use iskworks_core::{FinanceSummary, FinanceTransaction, Money};

    use super::*;

    #[test]
    fn query_rejects_unsupported_sort_and_inverted_dates() {
        let query = TransactionQuery {
            connection_ids: None,
            date_from: Some("2026-08-03".to_string()),
            date_to: Some("2026-08-01".to_string()),
            search: None,
            transaction_types: None,
            direction: None,
            category: None,
            location_id: None,
            type_id: None,
            exclude_inventory_buys: None,
            sort: Some("madeUp".to_string()),
            order: None,
            page: None,
            page_size: None,
            columns: None,
        };
        assert!(query.domain().is_err());
    }

    #[test]
    fn export_columns_are_validated_in_requested_order() {
        assert_eq!(
            parse_columns(Some("item,totalPrice,counterparty,location,time")).unwrap(),
            vec![
                FinanceColumn::Item,
                FinanceColumn::TotalPrice,
                FinanceColumn::Counterparty,
                FinanceColumn::Location,
                FinanceColumn::Time
            ]
        );
        assert_eq!(
            parse_sort(Some("location")).unwrap(),
            FinanceSortColumn::Location
        );
        assert!(parse_columns(Some("item,profit")).is_err());
    }

    #[test]
    fn csv_escapes_names_and_preserves_exact_money() {
        let page = FinanceTransactionPage {
            rows: vec![FinanceTransaction {
                observation_id: Uuid::new_v4(),
                transaction_id: 42,
                connection_id: ConnectedCharacterId::new(),
                character_name: "Pilot, One".to_string(),
                transaction_type: FinanceTransactionType::MarketSell,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                quantity: 2,
                unit_price: Money::parse("4.2500").unwrap(),
                total_price: Money::parse("8.5000").unwrap(),
                transacted_at: Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap(),
                counterparty_name: Some("=HYPERLINK(\"http://evil\")".to_string()),
                location_name: Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant".to_string()),
                region_name: Some("The Forge".to_string()),
                inventory_recording: None,
            }],
            summary: FinanceSummary {
                wallet_balance: Money::zero(),
                income: Money::zero(),
                expenses: Money::zero(),
                net_isk: Money::zero(),
                transaction_count: 1,
                average_daily_isk: Money::zero(),
            },
            available_characters: Vec::new(),
            total_count: 1,
            page: 1,
            page_size: 250,
        };

        let csv = String::from_utf8(
            finance_csv(
                &page,
                &[
                    FinanceColumn::Character,
                    FinanceColumn::TotalPrice,
                    FinanceColumn::Counterparty,
                    FinanceColumn::Location,
                    FinanceColumn::Region,
                ],
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            csv,
            "Character,Total Price,Client,Where,Region\n\"Pilot, One\",8.5000,\"'=HYPERLINK(\"\"http://evil\"\")\",Jita IV - Moon 4 - Caldari Navy Assembly Plant,The Forge\n"
        );
    }

    /// The Finance -> Inventory HTTP contract against real Postgres: derived
    /// (empty-body) recording, idempotent replay, information-hiding 404s across
    /// workspaces, and reversal.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn inventory_recording_endpoints_derive_everything_server_side(pool: sqlx::PgPool) {
        use crate::routes::esi::tests::{login, response_json};
        use axum::body::Body;
        use axum::http::{header, Method, Request};
        use iskworks_esi::{SecretCipher, WalletTransactionObservation};
        use tower::ServiceExt;

        let auth_service = crate::AuthService::new_for_tests(
            std::sync::Arc::new(iskworks_storage::PgUserRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgSessionRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgInviteRepository::new(pool.clone())),
            std::sync::Arc::new(crate::auth::MultiIdentityFakeTransport),
        );
        let esi = std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
        let state = AppState::new(std::sync::Arc::new(
            iskworks_storage::PgWorkspaceRepository::new(pool.clone()),
        ))
        .with_auth(Some(auth_service))
        .with_esi(esi.clone(), None)
        .with_finance_repository(std::sync::Arc::new(
            iskworks_storage::PgFinanceRepository::new(pool.clone()),
        ));
        let app = crate::build_router(state);
        let cookie_a = login(&app, "1:CharacterA").await;
        let cookie_b = login(&app, "2:CharacterB").await;

        let import_id = Uuid::new_v4();
        sqlx::query("INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1,'t','t','finance-api','active',true,now(),now())")
            .bind(import_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1, 17425, 'Nitrogen Isotopes', true)")
            .bind(import_id).execute(&pool).await.unwrap();

        let request = |method: Method, uri: &str, cookie: &str| {
            let app = app.clone();
            let request = Request::builder()
                .method(method)
                .uri(uri.to_string())
                .header(header::COOKIE, cookie.to_string())
                .body(Body::empty())
                .unwrap();
            async move { app.oneshot(request).await.unwrap() }
        };

        let workspace =
            response_json(request(Method::GET, "/api/workspace", &cookie_a).await).await;
        let workspace_id = iskworks_core::WorkspaceId(
            workspace["workspace"]["id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        );
        let owner_id =
            iskworks_core::OwnerId(workspace["owner"]["id"].as_str().unwrap().parse().unwrap());
        let cipher = SecretCipher::for_tests();
        let connection = esi
            .mock_connect(
                workspace_id,
                owner_id,
                cipher.encrypt("token").unwrap(),
                &[],
            )
            .await
            .unwrap();
        let run = esi
            .start_sync(&connection, iskworks_core::EsiSyncKind::WalletTransactions)
            .await
            .unwrap();
        esi.complete_wallet(
            &run,
            &[WalletTransactionObservation {
                transaction_id: 5_000_001,
                client_id: 1,
                location_id: 60_003_760,
                type_id: 17425,
                quantity: 50_000,
                unit_price: "729.20".parse().unwrap(),
                is_buy: true,
                is_personal: true,
                journal_ref_id: 5_000_002,
                transacted_at: Utc::now(),
                raw: serde_json::json!({}),
            }],
            None,
        )
        .await
        .unwrap();

        let list = |cookie: String| {
            let request = &request;
            async move {
                response_json(
                    request(
                        Method::GET,
                        "/api/finance/transactions?dateFrom=2000-01-01&dateTo=2100-01-01",
                        &cookie,
                    )
                    .await,
                )
                .await
            }
        };
        let page = list(cookie_a.clone()).await;
        let row = &page["rows"][0];
        assert_eq!(row["inventoryRecording"]["state"], "unrecorded");
        let observation_id = row["observationId"].as_str().unwrap().to_string();
        let record_uri = format!("/api/finance/transactions/{observation_id}/inventory-recording");
        let preview_uri = format!("{record_uri}/preview");

        assert_eq!(
            request(Method::POST, &record_uri, &cookie_b).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(Method::POST, &preview_uri, &cookie_b)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );

        // Preview derives the same posting and writes nothing.
        let preview = request(Method::POST, &preview_uri, &cookie_a).await;
        assert_eq!(preview.status(), StatusCode::OK);
        let preview = response_json(preview).await;
        assert_eq!(preview["posting"]["kind"], "purchase");
        assert_eq!(preview["posting"]["quantityDelta"], 50_000);
        assert_eq!(preview["posting"]["totalCostDelta"], "36460000.0000");
        assert_eq!(preview["current"]["quantity"], 0);
        assert_eq!(preview["resulting"]["quantity"], 50_000);
        assert_eq!(
            list(cookie_a.clone()).await["rows"][0]["inventoryRecording"]["state"],
            "unrecorded"
        );

        let created = request(Method::POST, &record_uri, &cookie_a).await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let created = response_json(created).await;
        assert_eq!(created["state"], "recorded");
        assert_eq!(created["quantity"], 50_000);
        assert_eq!(created["totalBasis"], "36460000.0000");
        let recording_id = created["recordingId"].as_str().unwrap().to_string();
        assert_eq!(
            request(Method::POST, &preview_uri, &cookie_a)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );

        let replay = request(Method::POST, &record_uri, &cookie_a).await;
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(
            response_json(replay).await["recordingId"],
            recording_id.as_str()
        );
        assert_eq!(
            list(cookie_a.clone()).await["rows"][0]["inventoryRecording"]["state"],
            "recorded"
        );

        let revert_uri = format!("{record_uri}/{recording_id}/revert");
        assert_eq!(
            request(Method::POST, &revert_uri, &cookie_b).await.status(),
            StatusCode::NOT_FOUND
        );
        let reverted = request(Method::POST, &revert_uri, &cookie_a).await;
        assert_eq!(reverted.status(), StatusCode::OK);
        assert_eq!(response_json(reverted).await["state"], "reverted");
        let again = request(Method::POST, &revert_uri, &cookie_a).await;
        assert_eq!(again.status(), StatusCode::CONFLICT);
        assert_eq!(
            response_json(again).await["error"]["code"],
            "invalid_inventory_state"
        );
        assert_eq!(
            request(Method::POST, &record_uri, &cookie_a).await.status(),
            StatusCode::CREATED
        );
    }
}
