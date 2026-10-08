//! Postgres aggregation for Finance analytics.
//!
//! Every query starts from the same `base` CTE (workspace, characters, date
//! window, direction, category, own-character trades) and aggregates in SQL;
//! no raw transactions reach Rust. The independent aggregates run
//! concurrently with `tokio::try_join!`.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use iskworks_core::{
    balance_at_or_before, bucket_start, build_cash_flow, build_fees, delta, derive_insights,
    fill_heatmap, fold_top_categories, next_bucket, percent_of, wallet_balance_series,
    AnalyticsKpis, AnalyticsRange, CategoryTotal, CharacterTotal, ConnectedCharacterId,
    ExcludedTrades, FeeRow, FinanceAnalytics, FinanceAnalyticsQuery, FinanceAnalyticsRepository,
    FinanceError, Granularity, InsightInputs, KpiValue, LocationTotal, MarginKpi, Money, TopItem,
    WalletKpi, WorkspaceId, FEE_REF_TYPES, HEATMAP_DAYS, TOP_CATEGORY_COUNT, TOP_ITEM_COUNT,
    TOP_LOCATION_COUNT, TREND_POINTS,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::PgFinanceRepository;

/// How a query treats buys currently recorded into accounting Inventory.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InventoryBuys {
    Include,
    /// Build inputs, not spend: drop them.
    Exclude,
    /// Only those (to report what was dropped).
    Only,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnTrades {
    /// Drop trades whose counterparty is one of our own characters.
    Exclude,
    /// Keep only those trades (to report what was excluded).
    Only,
}

struct Window {
    /// Inclusive UTC date bounds the `base` CTE scans.
    scan_from: NaiveDate,
    scan_to: NaiveDate,
    current: (NaiveDate, NaiveDate),
    previous: Option<(NaiveDate, NaiveDate)>,
}

struct BaseParams<'a> {
    workspace_id: WorkspaceId,
    connection_ids: &'a [Uuid],
    window: &'a Window,
    include_income: bool,
    include_expenses: bool,
    category: Option<&'a str>,
    own_trades: OwnTrades,
    inventory_buys: InventoryBuys,
}

fn midnight(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
}

fn money(mut value: Decimal) -> Money {
    value.rescale(4);
    Money(value)
}

fn map_sqlx(error: sqlx::Error) -> FinanceError {
    FinanceError::Persistence(error.to_string())
}

fn money_err(error: impl ToString) -> FinanceError {
    FinanceError::Persistence(error.to_string())
}

/// Starts a query with the shared CTEs; the caller appends `SELECT ... FROM base`.
fn base_query<'a>(params: &BaseParams<'a>) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new(
        r#"
        WITH active_import AS (
          SELECT id FROM sde_imports WHERE active LIMIT 1
        ),
        own AS (
          SELECT eve_character_id FROM eve_connections
          WHERE disconnected_at IS NULL AND workspace_id=
        "#,
    );
    query.push_bind(params.workspace_id.0);
    query.push(
        r#"
        ),
        base AS (
          SELECT w.connection_id, c.character_name, w.type_id, w.quantity, w.total_price,
                 w.is_buy, w.location_id,
                 (w.transacted_at AT TIME ZONE 'UTC')::date AS day,
                 COALESCE(tc.category, 'Other') AS category,
                 ((w.transacted_at AT TIME ZONE 'UTC')::date BETWEEN
        "#,
    );
    query.push_bind(params.window.current.0);
    query.push(" AND ");
    query.push_bind(params.window.current.1);
    query.push(") AS in_cur, ((w.transacted_at AT TIME ZONE 'UTC')::date BETWEEN ");
    query.push_bind(params.window.previous.map(|window| window.0));
    query.push(" AND ");
    query.push_bind(params.window.previous.map(|window| window.1));
    query.push(
        r#") AS in_prev
          FROM esi_wallet_transactions w
          JOIN eve_connections c ON c.id=w.connection_id
          LEFT JOIN active_import ai ON true
          LEFT JOIN sde_type_categories tc ON tc.import_id=ai.id AND tc.type_id=w.type_id
          WHERE c.disconnected_at IS NULL AND c.workspace_id=
        "#,
    );
    query.push_bind(params.workspace_id.0);
    if !params.connection_ids.is_empty() {
        query.push(" AND w.connection_id = ANY(");
        query.push_bind(params.connection_ids.to_vec());
        query.push(")");
    }
    query.push(" AND w.transacted_at >= ");
    query.push_bind(midnight(params.window.scan_from));
    query.push(" AND w.transacted_at < ");
    query.push_bind(midnight(params.window.scan_to + Duration::days(1)));
    match (params.include_income, params.include_expenses) {
        (true, true) => {}
        (true, false) => {
            query.push(" AND NOT w.is_buy");
        }
        (false, true) => {
            query.push(" AND w.is_buy");
        }
        (false, false) => {
            query.push(" AND false");
        }
    }
    if let Some(category) = params.category {
        query.push(" AND COALESCE(tc.category, 'Other') = ");
        query.push_bind(category.to_string());
    }
    match params.inventory_buys {
        InventoryBuys::Include => {}
        InventoryBuys::Exclude => {
            query.push(" AND NOT (w.is_buy AND EXISTS (SELECT 1 FROM inventory_event_sources s WHERE s.observation_id=w.id AND s.accounting_effect_kind='purchase' AND s.reverted_at IS NULL))");
        }
        InventoryBuys::Only => {
            query.push(" AND w.is_buy AND EXISTS (SELECT 1 FROM inventory_event_sources s WHERE s.observation_id=w.id AND s.accounting_effect_kind='purchase' AND s.reverted_at IS NULL)");
        }
    }
    query.push(match params.own_trades {
        OwnTrades::Exclude => {
            " AND (w.client_id IS NULL OR w.client_id NOT IN (SELECT eve_character_id FROM own))"
        }
        OwnTrades::Only => " AND w.client_id IN (SELECT eve_character_id FROM own)",
    });
    query.push(") ");
    query
}

fn trunc_unit(granularity: Granularity) -> &'static str {
    match granularity {
        Granularity::Day => "day",
        Granularity::Week => "week",
        Granularity::Month => "month",
    }
}

type CategoryRow = (bool, String, Decimal, Decimal, i64);
type FlowRow = (NaiveDate, bool, Decimal);
type CharacterRow = (Uuid, String, bool, Decimal);
type LocationRow = (i64, String, Option<String>, bool, Decimal, i64);
type TopRow = (i64, String, String, bool, i64, Decimal);
type TrendRow = (i64, bool, i32, Decimal);

async fn category_rows(
    pool: &PgPool,
    params: &BaseParams<'_>,
) -> Result<Vec<CategoryRow>, FinanceError> {
    let mut query = base_query(params);
    query.push(
        r#"
        SELECT is_buy, category,
               COALESCE(SUM(total_price) FILTER (WHERE in_cur), 0) AS cur,
               COALESCE(SUM(total_price) FILTER (WHERE in_prev), 0) AS prev,
               COUNT(*) FILTER (WHERE in_cur) AS n
        FROM base WHERE in_cur OR in_prev
        GROUP BY is_buy, category
        "#,
    );
    query
        .build_query_as::<CategoryRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)
}

async fn flow_rows(
    pool: &PgPool,
    params: &BaseParams<'_>,
    granularity: Granularity,
) -> Result<Vec<FlowRow>, FinanceError> {
    let mut query = base_query(params);
    query.push(format!(
        r#"
        SELECT date_trunc('{}', day::timestamp)::date AS bucket, is_buy, SUM(total_price)
        FROM base WHERE in_cur
        GROUP BY 1, 2
        "#,
        trunc_unit(granularity)
    ));
    query
        .build_query_as::<FlowRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)
}

async fn character_rows(
    pool: &PgPool,
    params: &BaseParams<'_>,
) -> Result<Vec<CharacterRow>, FinanceError> {
    let mut query = base_query(params);
    query.push(
        r#"
        SELECT connection_id, character_name, is_buy, SUM(total_price)
        FROM base WHERE in_cur
        GROUP BY connection_id, character_name, is_buy
        "#,
    );
    query
        .build_query_as::<CharacterRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)
}

async fn location_rows(
    pool: &PgPool,
    params: &BaseParams<'_>,
) -> Result<Vec<LocationRow>, FinanceError> {
    let mut query = base_query(params);
    query.push(
        r#"
        , agg AS (
          SELECT location_id, is_buy, SUM(total_price) AS total, COUNT(*) AS n
          FROM base WHERE in_cur GROUP BY location_id, is_buy
        ),
        names AS (
          SELECT l.location_id,
                 COALESCE(station.name_en, loc.location_name,
                          'Structure ' || l.location_id::text) AS location_name,
                 region.name_en AS region_name
          FROM (SELECT DISTINCT location_id FROM agg) l
          LEFT JOIN active_import ai ON true
          LEFT JOIN sde_npc_stations station
            ON station.import_id=ai.id AND station.station_id=l.location_id
          LEFT JOIN market_location_names loc
            ON loc.workspace_id=
        "#,
    );
    query.push_bind(params.workspace_id.0);
    query.push(
        r#"
           AND loc.location_id=l.location_id
          LEFT JOIN sde_solar_systems system
            ON system.import_id=ai.id
           AND system.solar_system_id=COALESCE(station.solar_system_id, loc.solar_system_id)
          LEFT JOIN sde_regions region
            ON region.import_id=ai.id AND region.region_id=system.region_id
        )
        SELECT a.location_id, n.location_name, n.region_name, a.is_buy, a.total, a.n
        FROM agg a JOIN names n ON n.location_id=a.location_id
        "#,
    );
    query
        .build_query_as::<LocationRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)
}

/// Top items per side, then their per-slot trend (a second, dependent query).
async fn top_rows(
    pool: &PgPool,
    params: &BaseParams<'_>,
    date_from: NaiveDate,
    date_to: NaiveDate,
) -> Result<(Vec<TopRow>, Vec<TrendRow>), FinanceError> {
    let mut query = base_query(params);
    query.push(
        r#"
        , agg AS (
          SELECT type_id, category, is_buy, SUM(quantity)::bigint AS qty, SUM(total_price) AS total
          FROM base WHERE in_cur GROUP BY type_id, category, is_buy
        ),
        ranked AS (
          SELECT *, row_number() OVER (PARTITION BY is_buy ORDER BY total DESC, type_id) AS rn
          FROM agg
        )
        SELECT r.type_id,
               COALESCE(t.name_en, 'Unknown EVE type ' || r.type_id::text) AS type_name,
               r.category, r.is_buy, r.qty, r.total
        FROM ranked r
        LEFT JOIN active_import ai ON true
        LEFT JOIN sde_types t ON t.import_id=ai.id AND t.type_id=r.type_id
        WHERE r.rn <=
        "#,
    );
    query.push_bind(TOP_ITEM_COUNT as i64);
    query.push(" ORDER BY r.is_buy, r.total DESC, r.type_id");
    let tops = query
        .build_query_as::<TopRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)?;
    if tops.is_empty() {
        return Ok((tops, Vec::new()));
    }

    let days = (date_to - date_from).num_days() + 1;
    let step = ((days + TREND_POINTS as i64 - 1) / TREND_POINTS as i64).max(1) as i32;
    let ids: Vec<i64> = tops.iter().map(|row| row.0).collect();
    let mut query = base_query(params);
    query.push(", slots AS (SELECT type_id, is_buy, LEAST(");
    query.push_bind((TREND_POINTS - 1) as i32);
    query.push(", ((day - ");
    query.push_bind(date_from);
    query.push(")::int / ");
    query.push_bind(step);
    query.push(")) AS slot, total_price FROM base WHERE in_cur AND type_id = ANY(");
    query.push_bind(ids);
    query.push(
        ")) SELECT type_id, is_buy, slot, SUM(total_price) FROM slots GROUP BY type_id, is_buy, slot",
    );
    let trends = query
        .build_query_as::<TrendRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)?;
    Ok((tops, trends))
}

async fn heatmap_nets(
    pool: &PgPool,
    params: &BaseParams<'_>,
) -> Result<BTreeMap<NaiveDate, Money>, FinanceError> {
    let mut query = base_query(params);
    query.push(
        r#"
        SELECT day, SUM(CASE WHEN is_buy THEN -total_price ELSE total_price END)
        FROM base GROUP BY day
        "#,
    );
    let rows = query
        .build_query_as::<(NaiveDate, Decimal)>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(day, net)| (day, money(net)))
        .collect())
}

async fn excluded_trades(
    pool: &PgPool,
    params: &BaseParams<'_>,
) -> Result<ExcludedTrades, FinanceError> {
    let mut query = base_query(params);
    query.push("SELECT COUNT(*), COALESCE(SUM(total_price), 0) FROM base WHERE in_cur");
    let (count, total) = query
        .build_query_as::<(i64, Decimal)>()
        .fetch_one(pool)
        .await
        .map_err(map_sqlx)?;
    Ok(ExcludedTrades {
        transaction_count: u64::try_from(count).unwrap_or(0),
        total_isk: money(total),
    })
}

async fn earliest_observed(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    connection_ids: &[Uuid],
) -> Result<Option<DateTime<Utc>>, FinanceError> {
    sqlx::query_scalar(
        r#"
        SELECT MIN(w.transacted_at)
        FROM esi_wallet_transactions w
        JOIN eve_connections c ON c.id=w.connection_id
        WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
          AND (cardinality($2::uuid[]) = 0 OR w.connection_id = ANY($2))
        "#,
    )
    .bind(workspace_id.0)
    .bind(connection_ids.to_vec())
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)
}

/// Fees paid in the wallet journal, by type, for the current window (with its
/// bucket) and the previous one.
async fn fee_rows(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    connection_ids: &[Uuid],
    window: &Window,
    granularity: Granularity,
) -> Result<Vec<FeeRow>, FinanceError> {
    let rows: Vec<(String, bool, Option<NaiveDate>, Decimal)> = sqlx::query_as(&format!(
        r"
        SELECT ref_type, in_cur,
               CASE WHEN in_cur THEN date_trunc('{}', day::timestamp)::date END AS bucket,
               SUM(-amount)
        FROM (
          SELECT j.ref_type, j.amount,
                 (j.occurred_at AT TIME ZONE 'UTC')::date AS day,
                 ((j.occurred_at AT TIME ZONE 'UTC')::date BETWEEN $4 AND $5) AS in_cur
          FROM esi_wallet_journal j
          JOIN eve_connections c ON c.id=j.connection_id
          WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
            AND (cardinality($2::uuid[]) = 0 OR j.connection_id = ANY($2))
            AND j.ref_type = ANY($3)
            AND j.occurred_at >= $6 AND j.occurred_at < $7
        ) fees
        GROUP BY ref_type, in_cur, bucket
        ",
        trunc_unit(granularity)
    ))
    .bind(workspace_id.0)
    .bind(connection_ids.to_vec())
    .bind(
        FEE_REF_TYPES
            .iter()
            .map(|kind| kind.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(window.current.0)
    .bind(window.current.1)
    .bind(midnight(window.scan_from))
    .bind(midnight(window.scan_to + Duration::days(1)))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(ref_type, in_current, bucket, total)| FeeRow {
            ref_type,
            in_current,
            bucket,
            total: money(total),
        })
        .collect())
}

async fn earliest_journal(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    connection_ids: &[Uuid],
) -> Result<Option<NaiveDate>, FinanceError> {
    let earliest: Option<DateTime<Utc>> = sqlx::query_scalar(
        r"
        SELECT MIN(j.occurred_at)
        FROM esi_wallet_journal j
        JOIN eve_connections c ON c.id=j.connection_id
        WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
          AND (cardinality($2::uuid[]) = 0 OR j.connection_id = ANY($2))
        ",
    )
    .bind(workspace_id.0)
    .bind(connection_ids.to_vec())
    .fetch_one(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(earliest.map(|at| at.date_naive()))
}

/// The wallet balance after the last journal entry of each character-day: real
/// balance history, far denser than sync-time snapshots.
async fn journal_balances(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    connection_ids: &[Uuid],
    before: NaiveDate,
) -> Result<Vec<(ConnectedCharacterId, DateTime<Utc>, Money)>, FinanceError> {
    let rows: Vec<(Uuid, DateTime<Utc>, Decimal)> = sqlx::query_as(
        r"
        SELECT DISTINCT ON (j.connection_id, (j.occurred_at AT TIME ZONE 'UTC')::date)
               j.connection_id, j.occurred_at, j.balance
        FROM esi_wallet_journal j
        JOIN eve_connections c ON c.id=j.connection_id
        WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
          AND (cardinality($2::uuid[]) = 0 OR j.connection_id = ANY($2))
          AND j.balance IS NOT NULL
          AND j.occurred_at < $3
        ORDER BY j.connection_id, (j.occurred_at AT TIME ZONE 'UTC')::date,
                 j.occurred_at DESC, j.ref_id DESC
        ",
    )
    .bind(workspace_id.0)
    .bind(connection_ids.to_vec())
    .bind(midnight(before + Duration::days(1)))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(id, at, balance)| (ConnectedCharacterId(id), at, money(balance)))
        .collect())
}

async fn balance_snapshots(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    connection_ids: &[Uuid],
    before: NaiveDate,
) -> Result<Vec<(ConnectedCharacterId, DateTime<Utc>, Money)>, FinanceError> {
    let rows: Vec<(Uuid, DateTime<Utc>, Decimal)> = sqlx::query_as(
        r#"
        SELECT b.connection_id, b.observed_at, b.balance
        FROM esi_wallet_balances b
        JOIN eve_connections c ON c.id=b.connection_id
        WHERE c.workspace_id=$1 AND c.disconnected_at IS NULL
          AND (cardinality($2::uuid[]) = 0 OR b.connection_id = ANY($2))
          AND b.observed_at < $3
        ORDER BY b.observed_at
        "#,
    )
    .bind(workspace_id.0)
    .bind(connection_ids.to_vec())
    .bind(midnight(before + Duration::days(1)))
    .fetch_all(pool)
    .await
    .map_err(map_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|(id, at, balance)| (ConnectedCharacterId(id), at, money(balance)))
        .collect())
}

#[async_trait]
impl FinanceAnalyticsRepository for PgFinanceRepository {
    async fn analytics(
        &self,
        workspace_id: WorkspaceId,
        query: FinanceAnalyticsQuery,
    ) -> Result<FinanceAnalytics, FinanceError> {
        let query = query.validate()?;
        let pool = &self.pool;
        let connection_ids: Vec<Uuid> = query.connection_ids.iter().map(|id| id.0).collect();
        let previous = query.previous_window();
        let window = Window {
            scan_from: previous.map_or(query.date_from, |window| window.0),
            scan_to: query.date_to,
            current: (query.date_from, query.date_to),
            previous,
        };
        let params = BaseParams {
            workspace_id,
            connection_ids: &connection_ids,
            window: &window,
            include_income: query.include_income,
            include_expenses: query.include_expenses,
            category: query.category.as_deref(),
            own_trades: OwnTrades::Exclude,
            inventory_buys: if query.exclude_inventory_buys {
                InventoryBuys::Exclude
            } else {
                InventoryBuys::Include
            },
        };
        let today = Utc::now().date_naive();
        let heat_window = Window {
            scan_from: today - Duration::days(HEATMAP_DAYS - 1),
            scan_to: today,
            current: (today - Duration::days(HEATMAP_DAYS - 1), today),
            previous: None,
        };
        let heat_params = BaseParams {
            window: &heat_window,
            include_income: true,
            include_expenses: true,
            ..BaseParams {
                ..clone_params(&params)
            }
        };
        let own_params = BaseParams {
            own_trades: OwnTrades::Only,
            ..clone_params(&params)
        };
        let inventory_params = BaseParams {
            inventory_buys: InventoryBuys::Only,
            include_income: false,
            include_expenses: true,
            ..clone_params(&params)
        };
        let exclude_inventory = query.exclude_inventory_buys;
        let donut_params = BaseParams {
            category: None,
            ..clone_params(&params)
        };
        let filtered_category = params.category.is_some();
        let earliest_ids = connection_ids.clone();

        let (
            categories,
            flow,
            characters,
            locations,
            tops,
            heat,
            excluded,
            earliest,
            balances,
            available,
            excluded_inventory,
            donut_categories,
            fee_rows,
            journal_start,
            journal_balances,
        ) = tokio::try_join!(
            category_rows(pool, &params),
            flow_rows(pool, &params, query.granularity),
            character_rows(pool, &params),
            location_rows(pool, &params),
            top_rows(pool, &params, query.date_from, query.date_to),
            heatmap_nets(pool, &heat_params),
            excluded_trades(pool, &own_params),
            earliest_observed(pool, workspace_id, &earliest_ids),
            balance_snapshots(pool, workspace_id, &connection_ids, query.date_to),
            self.finance_characters(workspace_id),
            async {
                if exclude_inventory {
                    excluded_trades(pool, &inventory_params).await
                } else {
                    Ok(ExcludedTrades {
                        transaction_count: 0,
                        total_isk: Money::zero(),
                    })
                }
            },
            async {
                if filtered_category {
                    category_rows(pool, &donut_params).await.map(Some)
                } else {
                    Ok(None)
                }
            },
            async {
                // Fees cannot be attributed to a category, so there is nothing to fetch.
                if filtered_category {
                    Ok(Vec::new())
                } else {
                    fee_rows(
                        pool,
                        workspace_id,
                        &connection_ids,
                        &window,
                        query.granularity,
                    )
                    .await
                }
            },
            earliest_journal(pool, workspace_id, &earliest_ids),
            journal_balances(pool, workspace_id, &connection_ids, query.date_to),
        )?;

        assemble(
            &query,
            previous,
            today,
            TradeInputs {
                categories,
                flow,
                characters,
                locations,
                tops,
                heat,
                excluded,
                excluded_inventory,
                donut_categories,
                earliest,
            },
            &balances,
            available,
            JournalInputs {
                fee_rows,
                earliest: journal_start,
                balances: journal_balances,
            },
        )
    }
}

fn clone_params<'a>(params: &BaseParams<'a>) -> BaseParams<'a> {
    BaseParams {
        workspace_id: params.workspace_id,
        connection_ids: params.connection_ids,
        window: params.window,
        include_income: params.include_income,
        include_expenses: params.include_expenses,
        category: params.category,
        own_trades: params.own_trades,
        inventory_buys: params.inventory_buys,
    }
}

/// What the wallet journal contributes: fees, coverage, and balance history.
struct JournalInputs {
    fee_rows: Vec<FeeRow>,
    earliest: Option<NaiveDate>,
    balances: Vec<(ConnectedCharacterId, DateTime<Utc>, Money)>,
}

/// What the wallet transaction queries contribute.
struct TradeInputs {
    categories: Vec<CategoryRow>,
    flow: Vec<FlowRow>,
    characters: Vec<CharacterRow>,
    locations: Vec<LocationRow>,
    tops: (Vec<TopRow>, Vec<TrendRow>),
    heat: BTreeMap<NaiveDate, Money>,
    /// Own-character trades left out of the totals.
    excluded: ExcludedTrades,
    /// Inventory purchases left out of the totals.
    excluded_inventory: ExcludedTrades,
    donut_categories: Option<Vec<CategoryRow>>,
    earliest: Option<DateTime<Utc>>,
}

fn assemble(
    query: &FinanceAnalyticsQuery,
    previous: Option<(NaiveDate, NaiveDate)>,
    today: NaiveDate,
    trades: TradeInputs,
    balances: &[(ConnectedCharacterId, DateTime<Utc>, Money)],
    available: Vec<iskworks_core::FinanceCharacter>,
    journal: JournalInputs,
) -> Result<FinanceAnalytics, FinanceError> {
    let TradeInputs {
        categories,
        flow,
        characters,
        locations,
        tops,
        heat,
        excluded,
        excluded_inventory,
        donut_categories,
        earliest,
    } = trades;
    let compare = previous.is_some();

    // Categories, both sides, unfolded first (KPIs and insights use all of it).
    let mut spending_all = Vec::new();
    let mut income_all = Vec::new();
    let mut spend_total = Money::zero();
    let mut spend_previous = Money::zero();
    let mut income_total = Money::zero();
    let mut income_previous = Money::zero();
    let mut transaction_count = 0u64;
    for (is_buy, category, current, prior, count) in categories {
        transaction_count += u64::try_from(count).unwrap_or(0);
        let row = CategoryTotal {
            category,
            total: money(current),
            previous: compare.then(|| money(prior)),
        };
        let (total, prev_total, list) = if is_buy {
            (&mut spend_total, &mut spend_previous, &mut spending_all)
        } else {
            (&mut income_total, &mut income_previous, &mut income_all)
        };
        *total = total.checked_add(row.total).map_err(money_err)?;
        *prev_total = prev_total.checked_add(money(prior)).map_err(money_err)?;
        if !row.total.0.is_zero() || row.previous.is_some_and(|value| !value.0.is_zero()) {
            list.push(row);
        }
    }
    // With a category selected the donuts still show every category (the
    // page is filtered, the breakdown is not), so they come from their own
    // unfiltered rows; totals and KPIs above stay filtered.
    if let Some(rows) = donut_categories {
        spending_all.clear();
        income_all.clear();
        for (is_buy, category, current, prior, _) in rows {
            let row = CategoryTotal {
                category,
                total: money(current),
                previous: compare.then(|| money(prior)),
            };
            if !row.total.0.is_zero() || row.previous.is_some_and(|value| !value.0.is_zero()) {
                if is_buy {
                    spending_all.push(row);
                } else {
                    income_all.push(row);
                }
            }
        }
    }

    // Cash flow.
    let mut sums: BTreeMap<NaiveDate, (Money, Money)> = BTreeMap::new();
    for (bucket, is_buy, total) in flow {
        let entry = sums.entry(bucket).or_insert((Money::zero(), Money::zero()));
        if is_buy {
            entry.1 = entry.1.checked_add(money(total)).map_err(money_err)?;
        } else {
            entry.0 = entry.0.checked_add(money(total)).map_err(money_err)?;
        }
    }
    let cash_flow = build_cash_flow(query.date_from, query.date_to, query.granularity, &sums)?;

    // KPIs.
    let net_total = income_total.checked_sub(spend_total).map_err(money_err)?;
    let net_previous = income_previous
        .checked_sub(spend_previous)
        .map_err(money_err)?;
    let margin = |net: Money, income: Money| percent_of(net, income);
    // Snapshots are sparse (one per sync); the journal records the balance after
    // every entry, so together they give a real history.
    let mut all_balances = balances.to_vec();
    all_balances.extend(journal.balances);
    let series = wallet_balance_series(&all_balances)?;
    let wallet_value = series
        .iter()
        .rev()
        .find(|(day, _)| *day <= query.date_to)
        .map(|(_, balance)| *balance);
    let wallet_previous = previous.and_then(|window| balance_at_or_before(&series, window.1));
    let wallet_sparkline: Vec<Money> = {
        let mut points = Vec::new();
        let mut start = bucket_start(query.date_from, query.granularity);
        while start <= query.date_to {
            let end =
                (next_bucket(start, query.granularity) - Duration::days(1)).min(query.date_to);
            if let Some(balance) = balance_at_or_before(&series, end) {
                points.push(balance);
            }
            start = next_bucket(start, query.granularity);
        }
        if points.len() >= 2 {
            points
        } else {
            Vec::new()
        }
    };
    let fees = if query.category.is_some() {
        None
    } else {
        let starts: Vec<NaiveDate> = cash_flow.iter().map(|bucket| bucket.start).collect();
        build_fees(&journal.fee_rows, &starts, previous, journal.earliest)?
    };
    let kpis = AnalyticsKpis {
        income: KpiValue {
            value: income_total,
            delta: delta(income_total, compare.then_some(income_previous)),
            sparkline: cash_flow.iter().map(|bucket| bucket.income).collect(),
        },
        expenses: KpiValue {
            value: spend_total,
            delta: delta(spend_total, compare.then_some(spend_previous)),
            sparkline: cash_flow.iter().map(|bucket| bucket.expenses).collect(),
        },
        net: KpiValue {
            value: net_total,
            delta: delta(net_total, compare.then_some(net_previous)),
            sparkline: cash_flow.iter().map(|bucket| bucket.net).collect(),
        },
        margin: MarginKpi {
            percent: margin(net_total, income_total),
            previous_percent: compare
                .then(|| margin(net_previous, income_previous))
                .flatten(),
            sparkline: cash_flow
                .iter()
                .map(|bucket| margin(bucket.net, bucket.income).unwrap_or(0.0))
                .collect(),
        },
        wallet_balance: WalletKpi {
            value: wallet_value,
            delta: wallet_value.and_then(|value| delta(value, wallet_previous)),
            sparkline: wallet_sparkline,
        },
        fees: fees.clone(),
        transaction_count,
    };

    // By character.
    let mut by_character: HashMap<Uuid, CharacterTotal> = HashMap::new();
    for (id, name, is_buy, total) in characters {
        let entry = by_character.entry(id).or_insert_with(|| CharacterTotal {
            connection_id: ConnectedCharacterId(id),
            character_name: name,
            income: Money::zero(),
            expenses: Money::zero(),
            net: Money::zero(),
        });
        if is_buy {
            entry.expenses = money(total);
        } else {
            entry.income = money(total);
        }
    }
    let mut by_character: Vec<CharacterTotal> = by_character.into_values().collect();
    for row in &mut by_character {
        row.net = row.income.checked_sub(row.expenses).map_err(money_err)?;
    }
    by_character.sort_by(|a, b| {
        b.income
            .cmp(&a.income)
            .then_with(|| a.character_name.cmp(&b.character_name))
    });

    // By location (top N by traded volume).
    let mut by_location: HashMap<i64, LocationTotal> = HashMap::new();
    for (location_id, name, region, is_buy, total, count) in locations {
        let entry = by_location
            .entry(location_id)
            .or_insert_with(|| LocationTotal {
                location_id,
                location_name: name,
                region_name: region,
                income: Money::zero(),
                expenses: Money::zero(),
                net: Money::zero(),
                transaction_count: 0,
            });
        entry.transaction_count += u64::try_from(count).unwrap_or(0);
        if is_buy {
            entry.expenses = money(total);
        } else {
            entry.income = money(total);
        }
    }
    let mut by_location: Vec<LocationTotal> = by_location.into_values().collect();
    for row in &mut by_location {
        row.net = row.income.checked_sub(row.expenses).map_err(money_err)?;
    }
    let volume = |row: &LocationTotal| row.income.0 + row.expenses.0;
    by_location.sort_by(|a, b| {
        volume(b)
            .cmp(&volume(a))
            .then_with(|| a.location_name.cmp(&b.location_name))
    });

    // Insights run on the full location list, before truncation for display.
    let insights = derive_insights(&InsightInputs {
        spending: &spending_all,
        income: &income_all,
        by_location: &by_location,
        by_character: &by_character,
        fees: fees.as_ref(),
        income_total,
    })?;
    by_location.truncate(TOP_LOCATION_COUNT);

    // Top items.
    let (top_rows, trend_rows) = tops;
    let mut trends: HashMap<(i64, bool), Vec<Money>> = HashMap::new();
    for (type_id, is_buy, slot, total) in trend_rows {
        let trend = trends
            .entry((type_id, is_buy))
            .or_insert_with(|| vec![Money::zero(); TREND_POINTS]);
        if let Some(cell) = usize::try_from(slot)
            .ok()
            .and_then(|slot| trend.get_mut(slot))
        {
            *cell = money(total);
        }
    }
    let mut top_expenses = Vec::new();
    let mut top_earners = Vec::new();
    for (type_id, type_name, category, is_buy, quantity, total) in top_rows {
        let total = money(total);
        let quantity = u64::try_from(quantity).unwrap_or(0);
        let side_total = if is_buy { spend_total } else { income_total };
        let item = TopItem {
            type_id,
            type_name,
            category,
            quantity,
            average_unit_price: if quantity == 0 {
                Money::zero()
            } else {
                total.checked_div_quantity(quantity).map_err(money_err)?
            },
            total,
            share_percent: percent_of(total, side_total),
            trend: trends
                .remove(&(type_id, is_buy))
                .unwrap_or_else(|| vec![Money::zero(); TREND_POINTS]),
        };
        if is_buy {
            top_expenses.push(item);
        } else {
            top_earners.push(item);
        }
    }

    Ok(FinanceAnalytics {
        range: AnalyticsRange {
            date_from: query.date_from,
            date_to: query.date_to,
            previous_date_from: previous.map(|window| window.0),
            previous_date_to: previous.map(|window| window.1),
            granularity: query.granularity,
        },
        earliest_observed_at: earliest,
        available_characters: available,
        excluded_intra_account: excluded,
        excluded_inventory_buys: excluded_inventory,
        kpis,
        cash_flow,
        spending_by_category: fold_top_categories(spending_all, TOP_CATEGORY_COUNT)?,
        income_by_category: fold_top_categories(income_all, TOP_CATEGORY_COUNT)?,
        by_character,
        by_location,
        top_expenses,
        top_earners,
        heatmap: fill_heatmap(today, HEATMAP_DAYS, &heat),
        insights,
    })
}
