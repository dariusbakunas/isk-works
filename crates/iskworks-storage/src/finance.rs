use chrono::{DateTime, NaiveDate, Utc};
use iskworks_core::{
    ConnectedCharacterId, FinanceCharacter, FinanceDirection, FinanceError, FinanceSummary,
    FinanceTransaction, FinanceTransactionFilter, FinanceTransactionPage, FinanceTransactionSort,
    FinanceTransactionType, Money, SavedFinanceFilter, SavedFinanceFilterId, SortDirection,
    WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::wallet_recording::{RecordingColumns, RECORDING_COLUMNS, RECORDING_JOINS};

#[derive(Clone)]
pub struct PgFinanceRepository {
    pub(crate) pool: PgPool,
}

impl PgFinanceRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn transactions(
        &self,
        workspace_id: WorkspaceId,
        filter: FinanceTransactionFilter,
        sort: FinanceTransactionSort,
    ) -> Result<FinanceTransactionPage, FinanceError> {
        let filter = filter.validate()?;
        let characters = self.finance_characters(workspace_id).await?;
        let selected_ids = selected_connection_ids(&filter, &characters);
        let mut query = transaction_query(workspace_id, &filter, &selected_ids);
        push_sort(&mut query, sort);
        let rows = query
            .build_query_as::<FinanceTransactionRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
        let all_rows = rows
            .into_iter()
            .map(FinanceTransactionRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        let total_count = all_rows.len() as u64;
        let wallet_balance = characters
            .iter()
            .filter(|character| selected_ids.contains(&character.connection_id))
            .filter_map(|character| character.wallet_balance)
            .try_fold(Money::zero(), |total, balance| total.checked_add(balance))
            .map_err(|error| FinanceError::Persistence(error.to_string()))?;
        let (date_from, date_to) = effective_dates(&filter, &all_rows);
        let summary =
            FinanceSummary::from_transactions(wallet_balance, &all_rows, date_from, date_to)
                .map_err(|error| FinanceError::Persistence(error.to_string()))?;
        let offset = usize::try_from((filter.page - 1) * filter.page_size)
            .map_err(|_| FinanceError::Validation("page offset is too large".to_string()))?;
        let page_size = filter.page_size as usize;
        let rows = all_rows.into_iter().skip(offset).take(page_size).collect();
        Ok(FinanceTransactionPage {
            rows,
            summary,
            available_characters: characters,
            total_count,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    pub async fn save_wallet_balance(
        &self,
        connection_id: ConnectedCharacterId,
        sync_run_id: iskworks_core::EsiSyncRunId,
        balance: Money,
        observed_at: DateTime<Utc>,
        source_checksum: &str,
    ) -> Result<(), FinanceError> {
        sqlx::query(
            r#"
            INSERT INTO esi_wallet_balances (
              id, connection_id, sync_run_id, balance, observed_at, source_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (connection_id, sync_run_id) DO UPDATE SET
              balance=EXCLUDED.balance,
              observed_at=EXCLUDED.observed_at,
              source_checksum=EXCLUDED.source_checksum
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(connection_id.0)
        .bind(sync_run_id.0)
        .bind(balance.0)
        .bind(observed_at)
        .bind(source_checksum)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    pub async fn saved_filters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SavedFinanceFilter>, FinanceError> {
        sqlx::query_as::<_, SavedFilterRow>(
            r#"
            SELECT id, name, filter_payload, created_at, updated_at
            FROM finance_saved_filters
            WHERE workspace_id=$1
            ORDER BY lower(name), id
            "#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(SavedFilterRow::into_domain)
        .collect()
    }

    pub async fn save_filter(
        &self,
        workspace_id: WorkspaceId,
        name: &str,
        filter: FinanceTransactionFilter,
    ) -> Result<SavedFinanceFilter, FinanceError> {
        let name = SavedFinanceFilter::validate_name(name)?;
        let filter = filter.validate()?;
        let payload = serde_json::to_value(&filter)
            .map_err(|error| FinanceError::Persistence(error.to_string()))?;
        let now = crate::db_now();
        let row = sqlx::query_as::<_, SavedFilterRow>(
            r#"
            INSERT INTO finance_saved_filters (
              id, workspace_id, name, filter_payload, created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$5)
            ON CONFLICT (workspace_id, lower(name)) DO UPDATE SET
              name=EXCLUDED.name,
              filter_payload=EXCLUDED.filter_payload,
              updated_at=EXCLUDED.updated_at
            RETURNING id, name, filter_payload, created_at, updated_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id.0)
        .bind(name)
        .bind(payload)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;
        row.into_domain()
    }

    pub async fn delete_filter(
        &self,
        workspace_id: WorkspaceId,
        id: SavedFinanceFilterId,
    ) -> Result<(), FinanceError> {
        let result =
            sqlx::query("DELETE FROM finance_saved_filters WHERE workspace_id=$1 AND id=$2")
                .bind(workspace_id.0)
                .bind(id.0)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx)?;
        if result.rows_affected() == 0 {
            return Err(FinanceError::NotFound);
        }
        Ok(())
    }

    pub(crate) async fn finance_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<FinanceCharacter>, FinanceError> {
        sqlx::query_as::<_, FinanceCharacterRow>(
            r#"
            SELECT c.id AS connection_id, c.character_name,
                   balance.balance AS wallet_balance,
                   balance.observed_at AS balance_observed_at
            FROM eve_connections c
            LEFT JOIN LATERAL (
              SELECT b.balance, b.observed_at
              FROM esi_wallet_balances b
              WHERE b.connection_id=c.id
              ORDER BY b.observed_at DESC, b.id DESC
              LIMIT 1
            ) balance ON true
            WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
            ORDER BY lower(c.character_name), c.id
            "#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(FinanceCharacterRow::into_domain)
        .collect()
    }
}

fn transaction_query<'a>(
    workspace_id: WorkspaceId,
    filter: &'a FinanceTransactionFilter,
    selected_ids: &'a [ConnectedCharacterId],
) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new(format!(
        r#"
        WITH active_import AS (
          SELECT id FROM sde_imports WHERE active LIMIT 1
        )
        SELECT w.id, w.source_transaction_id, w.connection_id, c.character_name,
               w.type_id,
               COALESCE(t.name_en, 'Unknown EVE type ' || w.type_id::text) AS type_name,
               w.quantity, w.unit_price, w.total_price, w.is_buy, w.transacted_at,
               entity.entity_name AS counterparty_name,
               COALESCE(station.name_en, location.location_name) AS location_name,
               region.name_en AS region_name,
               {RECORDING_COLUMNS}
        FROM esi_wallet_transactions w
        JOIN eve_connections c ON c.id=w.connection_id
        LEFT JOIN active_import ai ON true
        LEFT JOIN sde_types t ON t.import_id=ai.id AND t.type_id=w.type_id
        LEFT JOIN sde_type_categories tc ON tc.import_id=ai.id AND tc.type_id=w.type_id
        LEFT JOIN eve_entity_names entity ON entity.entity_id=w.client_id
        LEFT JOIN sde_npc_stations station
          ON station.import_id=ai.id AND station.station_id=w.location_id
        LEFT JOIN market_location_names location
          ON location.workspace_id=c.workspace_id AND location.location_id=w.location_id
        LEFT JOIN sde_solar_systems system
          ON system.import_id=ai.id
         AND system.solar_system_id=COALESCE(station.solar_system_id, location.solar_system_id)
        LEFT JOIN sde_regions region
          ON region.import_id=ai.id AND region.region_id=system.region_id
        {RECORDING_JOINS}
        WHERE c.workspace_id=
        "#
    ));
    query.push_bind(workspace_id.0);
    query.push(" AND c.disconnected_at IS NULL");
    if !selected_ids.is_empty() {
        query.push(" AND w.connection_id = ANY(");
        query.push_bind(selected_ids.iter().map(|id| id.0).collect::<Vec<_>>());
        query.push(")");
    }
    if let Some(date_from) = filter.date_from {
        query.push(" AND w.transacted_at >= ");
        query.push_bind(date_from.and_hms_opt(0, 0, 0).expect("midnight is valid"));
        query.push(" AT TIME ZONE 'UTC'");
    }
    if let Some(date_to) = filter.date_to {
        let exclusive_to = date_to.succ_opt().unwrap_or(date_to);
        query.push(" AND w.transacted_at < ");
        query.push_bind(
            exclusive_to
                .and_hms_opt(0, 0, 0)
                .expect("midnight is valid"),
        );
        query.push(" AT TIME ZONE 'UTC'");
    }
    let include_buy = filter
        .transaction_types
        .contains(&FinanceTransactionType::MarketBuy);
    let include_sell = filter
        .transaction_types
        .contains(&FinanceTransactionType::MarketSell);
    match (include_buy, include_sell, filter.direction) {
        (_, _, FinanceDirection::Income) | (false, true, FinanceDirection::All) => {
            query.push(" AND NOT w.is_buy");
        }
        (_, _, FinanceDirection::Expense) | (true, false, FinanceDirection::All) => {
            query.push(" AND w.is_buy");
        }
        (false, false, FinanceDirection::All) => {
            query.push(" AND false");
        }
        (true, true, FinanceDirection::All) => {}
    }
    if filter.exclude_inventory_buys {
        query.push(" AND NOT (w.is_buy AND EXISTS (SELECT 1 FROM inventory_event_sources s WHERE s.observation_id=w.id AND s.accounting_effect_kind='purchase' AND s.reverted_at IS NULL))");
    }
    if let Some(category) = &filter.category {
        query.push(" AND COALESCE(tc.category, 'Other') = ");
        query.push_bind(category);
    }
    if let Some(location_id) = filter.location_id {
        query.push(" AND w.location_id = ");
        query.push_bind(location_id);
    }
    if let Some(type_id) = filter.type_id {
        query.push(" AND w.type_id = ");
        query.push_bind(type_id);
    }
    if let Some(search) = &filter.search {
        let pattern = format!("%{}%", search.to_lowercase());
        query.push(" AND (lower(c.character_name) LIKE ");
        query.push_bind(pattern.clone());
        query.push(" OR lower(COALESCE(t.name_en, '')) LIKE ");
        query.push_bind(pattern.clone());
        query.push(" OR w.type_id::text LIKE ");
        query.push_bind(pattern.clone());
        query.push(" OR w.source_transaction_id::text LIKE ");
        query.push_bind(pattern.clone());
        query.push(" OR lower(COALESCE(entity.entity_name, '')) LIKE ");
        query.push_bind(pattern.clone());
        query.push(" OR lower(COALESCE(station.name_en, location.location_name, '')) LIKE ");
        query.push_bind(pattern);
        query.push(")");
    }
    query
}

fn push_sort(query: &mut QueryBuilder<'_, Postgres>, sort: FinanceTransactionSort) {
    use iskworks_core::FinanceSortColumn;
    query.push(" ORDER BY ");
    query.push(match sort.column {
        FinanceSortColumn::Time => "w.transacted_at",
        FinanceSortColumn::Character => "lower(c.character_name)",
        FinanceSortColumn::TransactionType | FinanceSortColumn::Direction => "w.is_buy",
        FinanceSortColumn::Item => "lower(COALESCE(t.name_en, ''))",
        FinanceSortColumn::Quantity => "w.quantity",
        FinanceSortColumn::UnitPrice => "w.unit_price",
        FinanceSortColumn::TotalPrice => "w.total_price",
        FinanceSortColumn::Counterparty => "lower(COALESCE(entity.entity_name, ''))",
        FinanceSortColumn::Location => {
            "lower(COALESCE(station.name_en, location.location_name, ''))"
        }
        FinanceSortColumn::Region => "lower(COALESCE(region.name_en, ''))",
    });
    query.push(match sort.direction {
        SortDirection::Asc => " ASC",
        SortDirection::Desc => " DESC",
    });
    query.push(", w.source_transaction_id DESC");
}

fn selected_connection_ids(
    filter: &FinanceTransactionFilter,
    characters: &[FinanceCharacter],
) -> Vec<ConnectedCharacterId> {
    if filter.connection_ids.is_empty() {
        characters
            .iter()
            .map(|character| character.connection_id)
            .collect()
    } else {
        filter.connection_ids.clone()
    }
}

fn effective_dates(
    filter: &FinanceTransactionFilter,
    rows: &[FinanceTransaction],
) -> (NaiveDate, NaiveDate) {
    let today = Utc::now().date_naive();
    let first = rows
        .iter()
        .map(|row| row.transacted_at.date_naive())
        .min()
        .unwrap_or(today);
    let last = rows
        .iter()
        .map(|row| row.transacted_at.date_naive())
        .max()
        .unwrap_or(today);
    (
        filter.date_from.unwrap_or(first),
        filter.date_to.unwrap_or(last),
    )
}

#[derive(sqlx::FromRow)]
struct FinanceTransactionRow {
    id: Uuid,
    source_transaction_id: i64,
    connection_id: Uuid,
    character_name: String,
    type_id: i64,
    type_name: String,
    quantity: i64,
    unit_price: Decimal,
    total_price: Decimal,
    is_buy: bool,
    transacted_at: DateTime<Utc>,
    counterparty_name: Option<String>,
    location_name: Option<String>,
    region_name: Option<String>,
    #[sqlx(flatten)]
    recording: RecordingColumns,
}

impl FinanceTransactionRow {
    fn into_domain(self) -> Result<FinanceTransaction, FinanceError> {
        Ok(FinanceTransaction {
            observation_id: self.id,
            transaction_id: self.source_transaction_id,
            connection_id: ConnectedCharacterId(self.connection_id),
            character_name: self.character_name,
            transaction_type: if self.is_buy {
                FinanceTransactionType::MarketBuy
            } else {
                FinanceTransactionType::MarketSell
            },
            type_id: self.type_id,
            type_name: self.type_name,
            quantity: self
                .quantity
                .try_into()
                .map_err(|_| FinanceError::Persistence("negative wallet quantity".to_string()))?,
            unit_price: Money(self.unit_price),
            total_price: Money(self.total_price),
            transacted_at: self.transacted_at,
            counterparty_name: self.counterparty_name,
            location_name: self.location_name,
            region_name: self.region_name,
            inventory_recording: self
                .recording
                .recording(self.is_buy)
                .map_err(|error| FinanceError::Persistence(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct FinanceCharacterRow {
    connection_id: Uuid,
    character_name: String,
    wallet_balance: Option<Decimal>,
    balance_observed_at: Option<DateTime<Utc>>,
}

impl FinanceCharacterRow {
    fn into_domain(self) -> Result<FinanceCharacter, FinanceError> {
        Ok(FinanceCharacter {
            connection_id: ConnectedCharacterId(self.connection_id),
            character_name: self.character_name,
            wallet_balance: self.wallet_balance.map(Money),
            balance_observed_at: self.balance_observed_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SavedFilterRow {
    id: Uuid,
    name: String,
    filter_payload: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SavedFilterRow {
    fn into_domain(self) -> Result<SavedFinanceFilter, FinanceError> {
        let filter = serde_json::from_value::<FinanceTransactionFilter>(self.filter_payload)
            .map_err(|error| FinanceError::Persistence(error.to_string()))?
            .validate()?;
        Ok(SavedFinanceFilter {
            id: SavedFinanceFilterId(self.id),
            name: self.name,
            filter,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_sqlx(error: sqlx::Error) -> FinanceError {
    FinanceError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests;
