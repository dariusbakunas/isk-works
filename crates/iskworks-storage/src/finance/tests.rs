use chrono::TimeZone;
use iskworks_core::{
    ConnectedCharacterId, FinanceDirection, FinanceTransactionFilter, FinanceTransactionSort,
    FinanceTransactionType, OwnerId, WorkspaceId,
};
use serde_json::json;

use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn finance_query_filters_characters_and_calculates_summary(pool: PgPool) {
    let fixture = seed_finance_workspace(&pool).await;
    seed_transaction(
        &pool,
        fixture.first_connection,
        1,
        false,
        "120.0000",
        Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
    )
    .await;
    sqlx::query("INSERT INTO eve_entity_names (entity_id,entity_name,category,observed_at) VALUES (1,'Caldari Navy','corporation',now())")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO market_location_names (workspace_id,location_id,location_name,owner_id,solar_system_id,resolved_at,updated_at) VALUES ($1,1050000000001,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',1000125,30000142,now(),now())")
        .bind(fixture.workspace_id.0).execute(&pool).await.unwrap();
    seed_transaction(
        &pool,
        fixture.first_connection,
        2,
        true,
        "30.0000",
        Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap(),
    )
    .await;
    seed_transaction(
        &pool,
        fixture.second_connection,
        3,
        false,
        "999.0000",
        Utc.with_ymd_and_hms(2026, 8, 2, 12, 0, 0).unwrap(),
    )
    .await;
    let filter = FinanceTransactionFilter {
        connection_ids: vec![ConnectedCharacterId(fixture.first_connection)],
        date_from: Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()),
        date_to: Some(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap()),
        direction: FinanceDirection::All,
        transaction_types: vec![
            FinanceTransactionType::MarketBuy,
            FinanceTransactionType::MarketSell,
        ],
        ..FinanceTransactionFilter::default()
    };

    let page = PgFinanceRepository::new(pool)
        .transactions(
            fixture.workspace_id,
            filter,
            FinanceTransactionSort::default(),
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 2);
    assert_eq!(
        page.rows[0].counterparty_name.as_deref(),
        Some("Caldari Navy")
    );
    assert_eq!(
        page.rows[0].location_name.as_deref(),
        Some("Jita IV - Moon 4 - Caldari Navy Assembly Plant")
    );
    assert_eq!(page.summary.income.0.to_string(), "120.0000");
    assert_eq!(page.summary.expenses.0.to_string(), "30.0000");
    assert_eq!(page.summary.net_isk.0.to_string(), "90.0000");
    assert_eq!(page.summary.average_daily_isk.0.to_string(), "30.0000");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn finance_query_filters_category_location_and_type(pool: PgPool) {
    let fixture = seed_finance_workspace(&pool).await;
    seed_transaction(
        &pool,
        fixture.first_connection,
        1,
        false,
        "120.0000",
        Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
    )
    .await;
    let repository = PgFinanceRepository::new(pool);
    let count = |filter: FinanceTransactionFilter| {
        let repository = repository.clone();
        let workspace_id = fixture.workspace_id;
        async move {
            repository
                .transactions(workspace_id, filter, FinanceTransactionSort::default())
                .await
                .unwrap()
                .rows
                .len()
        }
    };
    let base = FinanceTransactionFilter::default();
    assert_eq!(count(base.clone()).await, 1);
    // Seeded type 34 has no SDE row, so it rolls up to "Other".
    let with = |patch: fn(&mut FinanceTransactionFilter)| {
        let mut filter = base.clone();
        patch(&mut filter);
        filter
    };
    assert_eq!(count(with(|f| f.category = Some("Other".into()))).await, 1);
    assert_eq!(count(with(|f| f.category = Some("Ships".into()))).await, 0);
    assert_eq!(count(with(|f| f.type_id = Some(34))).await, 1);
    assert_eq!(count(with(|f| f.type_id = Some(35))).await, 0);
    assert_eq!(
        count(with(|f| f.location_id = Some(1_050_000_000_001))).await,
        1
    );
    assert_eq!(count(with(|f| f.location_id = Some(5))).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn saved_filters_are_scoped_and_validate_on_load(pool: PgPool) {
    let first = seed_finance_workspace(&pool).await;
    let unrelated_workspace = WorkspaceId(Uuid::new_v4());
    let repository = PgFinanceRepository::new(pool);

    let saved = repository
        .save_filter(
            first.workspace_id,
            " Market only ",
            FinanceTransactionFilter::default(),
        )
        .await
        .unwrap();

    assert_eq!(saved.name, "Market only");
    assert_eq!(
        repository
            .saved_filters(first.workspace_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(repository
        .saved_filters(unrelated_workspace)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        repository
            .delete_filter(unrelated_workspace, saved.id)
            .await,
        Err(FinanceError::NotFound)
    );
    repository
        .delete_filter(first.workspace_id, saved.id)
        .await
        .unwrap();
}

struct FinanceFixture {
    workspace_id: WorkspaceId,
    first_connection: Uuid,
    second_connection: Uuid,
}

async fn seed_finance_workspace(pool: &PgPool) -> FinanceFixture {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let first_connection = Uuid::new_v4();
    let second_connection = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1,'Finance Test',$2,$3,$3)")
        .bind(workspace_id.0).bind(owner_id.0).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1,$2,'manual','Finance Test',false,$3,$3)")
        .bind(owner_id.0).bind(workspace_id.0).bind(now).execute(&mut *tx).await.unwrap();
    for (index, connection_id) in [first_connection, second_connection]
        .into_iter()
        .enumerate()
    {
        sqlx::query(r#"INSERT INTO eve_connections (
                id, workspace_id, owner_id, eve_character_id, character_name, status,
                granted_scopes, connected_at, updated_at, revision
              ) VALUES ($1,$2,$3,$4,$5,'connected',ARRAY['esi-wallet.read_character_wallet.v1'],$6,$6,1)"#)
            .bind(connection_id).bind(workspace_id.0).bind(owner_id.0)
            .bind(10_000_i64 + index as i64).bind(format!("Character {index}"))
            .bind(now).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    FinanceFixture {
        workspace_id,
        first_connection,
        second_connection,
    }
}

async fn seed_transaction(
    pool: &PgPool,
    connection_id: Uuid,
    transaction_id: i64,
    is_buy: bool,
    total: &str,
    transacted_at: DateTime<Utc>,
) {
    let run_id = Uuid::new_v4();
    let workspace_id =
        sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM eve_connections WHERE id=$1")
            .bind(connection_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let owner_id =
        sqlx::query_scalar::<_, Uuid>("SELECT owner_id FROM eve_connections WHERE id=$1")
            .bind(connection_id)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        r#"INSERT INTO esi_sync_runs (
            id, workspace_id, owner_id, connection_id, requested_kind, status, phase,
            started_at, completed_at, summary
          ) VALUES ($1,$2,$3,$4,'wallet_transactions','succeeded','complete',$5,$5,'test')"#,
    )
    .bind(run_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(connection_id)
    .bind(transacted_at)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO esi_wallet_transactions (
            id, connection_id, source_transaction_id, first_sync_run_id, last_sync_run_id,
            type_id, quantity, unit_price, total_price, is_buy, is_personal, transacted_at,
            location_id, client_id, journal_ref_id, raw_payload, source_checksum,
            first_observed_at, last_observed_at
          ) VALUES ($1,$2,$3,$4,$4,34,1,$5,$5,$6,true,$7,1050000000001,1,$8,$9,'test',$7,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(connection_id)
    .bind(transaction_id)
    .bind(run_id)
    .bind(Decimal::from_str_exact(total).unwrap())
    .bind(is_buy)
    .bind(transacted_at)
    .bind(transaction_id + 100)
    .bind(json!({"transaction_id": transaction_id}))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn analytics_aggregates_compare_windows_and_exclude_own_trades(pool: PgPool) {
    use iskworks_core::{FinanceAnalyticsQuery, FinanceAnalyticsRepository, Granularity};

    let fixture = seed_finance_workspace(&pool).await;
    let import_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at) VALUES ($1,'v','l','c','active',true,now())")
        .bind(import_id).execute(&pool).await.unwrap();
    for (type_id, name, category) in [
        (34_i64, "Tritanium", "Ores & Minerals"),
        (587, "Rifter", "Ships"),
    ] {
        sqlx::query(
            "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1,$2,$3,true)",
        )
        .bind(import_id)
        .bind(type_id)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sde_type_categories (import_id, type_id, category) VALUES ($1,$2,$3)",
        )
        .bind(import_id)
        .bind(type_id)
        .bind(category)
        .execute(&pool)
        .await
        .unwrap();
    }
    // (id, connection, is_buy, total, quantity, type, client, when)
    let at = |m: u32, d: u32| Utc.with_ymd_and_hms(2026, m, d, 12, 0, 0).unwrap();
    let rows = [
        (
            1_i64,
            fixture.first_connection,
            false,
            "1000",
            2_i64,
            587_i64,
            5_i64,
            at(9, 10),
        ),
        (
            2,
            fixture.first_connection,
            true,
            "300",
            10,
            34,
            5,
            at(9, 11),
        ),
        (
            3,
            fixture.second_connection,
            false,
            "500",
            1,
            587,
            5,
            at(9, 12),
        ),
        // Sold to our own second character (eve id 10001): excluded.
        (
            4,
            fixture.first_connection,
            false,
            "200",
            4,
            34,
            10_001,
            at(9, 13),
        ),
        // Previous window.
        (
            5,
            fixture.first_connection,
            false,
            "400",
            1,
            587,
            5,
            at(8, 15),
        ),
        (
            6,
            fixture.first_connection,
            true,
            "100",
            5,
            34,
            5,
            at(8, 16),
        ),
    ];
    for (id, connection, is_buy, total, quantity, type_id, client, when) in rows {
        seed_transaction(&pool, connection, id, is_buy, total, when).await;
        sqlx::query("UPDATE esi_wallet_transactions SET type_id=$1, client_id=$2, quantity=$3 WHERE source_transaction_id=$4")
            .bind(type_id).bind(client).bind(quantity).bind(id).execute(&pool).await.unwrap();
    }
    let repository = PgFinanceRepository::new(pool);
    let query = FinanceAnalyticsQuery {
        connection_ids: vec![],
        date_from: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        include_income: true,
        include_expenses: true,
        granularity: Granularity::Week,
        compare_previous: true,
        exclude_inventory_buys: false,
        category: None,
    };

    let analytics = repository
        .analytics(fixture.workspace_id, query.clone())
        .await
        .unwrap();

    let value = |money: iskworks_core::Money| money.0.to_string();
    assert_eq!(value(analytics.kpis.income.value), "1500.0000");
    assert_eq!(value(analytics.kpis.expenses.value), "300.0000");
    assert_eq!(value(analytics.kpis.net.value), "1200.0000");
    assert_eq!(analytics.kpis.transaction_count, 3);
    let income_delta = analytics.kpis.income.delta.as_ref().unwrap();
    assert_eq!(value(income_delta.previous), "400.0000");
    assert!((income_delta.percent.unwrap() - 275.0).abs() < 1e-9);
    assert_eq!(analytics.excluded_intra_account.transaction_count, 1);
    assert_eq!(
        value(analytics.excluded_intra_account.total_isk),
        "200.0000"
    );
    assert_eq!(analytics.income_by_category.len(), 1);
    assert_eq!(analytics.income_by_category[0].category, "Ships");
    assert_eq!(
        analytics.spending_by_category[0].category,
        "Ores & Minerals"
    );
    assert_eq!(
        analytics.spending_by_category[0]
            .previous
            .map(value)
            .as_deref(),
        Some("100.0000")
    );
    assert_eq!(analytics.by_character.len(), 2);
    assert_eq!(analytics.by_character[0].character_name, "Character 0");
    assert_eq!(value(analytics.by_character[0].income), "1000.0000");
    assert_eq!(value(analytics.by_character[0].expenses), "300.0000");
    assert_eq!(analytics.top_earners[0].type_name, "Rifter");
    assert_eq!(analytics.top_earners[0].quantity, 3);
    assert_eq!(analytics.top_expenses[0].quantity, 10);
    assert_eq!(
        value(analytics.top_expenses[0].average_unit_price),
        "30.0000"
    );
    assert_eq!(analytics.top_expenses[0].trend.len(), 8);
    assert_eq!(value(analytics.top_expenses[0].trend[2]), "300.0000"); // Sep 11 is day 10, step 4 -> slot 2
    assert_eq!(analytics.by_location.len(), 1);
    assert_eq!(analytics.by_location[0].transaction_count, 3);
    assert_eq!(analytics.heatmap.len(), 91);
    assert_eq!(analytics.available_characters.len(), 2);
    assert_eq!(analytics.earliest_observed_at, Some(at(8, 15)));
    let cash_in: iskworks_core::Money = analytics
        .cash_flow
        .iter()
        .try_fold(iskworks_core::Money::zero(), |sum, b| {
            sum.checked_add(b.income)
        })
        .unwrap();
    assert_eq!(value(cash_in), "1500.0000");
    assert_eq!(
        value(analytics.cash_flow.last().unwrap().cumulative_net),
        "1200.0000"
    );

    // Comparison off: no deltas or previous totals.
    let no_compare = repository
        .analytics(
            fixture.workspace_id,
            FinanceAnalyticsQuery {
                compare_previous: false,
                exclude_inventory_buys: false,
                ..query.clone()
            },
        )
        .await
        .unwrap();
    assert!(no_compare.kpis.income.delta.is_none());
    assert!(no_compare.range.previous_date_from.is_none());
    assert_eq!(no_compare.spending_by_category[0].previous, None);

    // Category filter narrows every aggregate; direction narrows sides.
    let ships = repository
        .analytics(
            fixture.workspace_id,
            FinanceAnalyticsQuery {
                category: Some("Ships".into()),
                ..query.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(value(ships.kpis.income.value), "1500.0000");
    assert_eq!(value(ships.kpis.expenses.value), "0.0000");
    // The breakdown stays complete so the donuts can dim, not drop, the
    // other categories; only the totals above are filtered.
    assert_eq!(ships.spending_by_category.len(), 1);
    assert_eq!(ships.spending_by_category[0].category, "Ores & Minerals");
    assert_eq!(ships.income_by_category[0].category, "Ships");
    let income_only = repository
        .analytics(
            fixture.workspace_id,
            FinanceAnalyticsQuery {
                include_expenses: false,
                ..query.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(value(income_only.kpis.expenses.value), "0.0000");
    // One character only.
    let first_only = repository
        .analytics(
            fixture.workspace_id,
            FinanceAnalyticsQuery {
                connection_ids: vec![ConnectedCharacterId(fixture.first_connection)],
                ..query
            },
        )
        .await
        .unwrap();
    assert_eq!(value(first_only.kpis.income.value), "1000.0000");
}

async fn seed_journal(
    pool: &PgPool,
    connection_id: Uuid,
    ref_id: i64,
    at: DateTime<Utc>,
    ref_type: &str,
    amount: &str,
    balance: &str,
) {
    sqlx::query(
        r#"INSERT INTO esi_wallet_journal
               (id, connection_id, ref_id, occurred_at, ref_type, amount, balance, raw_payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,'{}')"#,
    )
    .bind(Uuid::new_v4())
    .bind(connection_id)
    .bind(ref_id)
    .bind(at)
    .bind(ref_type)
    .bind(Decimal::from_str_exact(amount).unwrap())
    .bind(Decimal::from_str_exact(balance).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn analytics_reports_fees_and_balance_history_from_the_wallet_journal(pool: PgPool) {
    use iskworks_core::{FinanceAnalyticsQuery, FinanceAnalyticsRepository, Granularity};

    let fixture = seed_finance_workspace(&pool).await;
    seed_transaction(
        &pool,
        fixture.first_connection,
        1,
        false,
        "1000",
        Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap(),
    )
    .await;
    let repository = PgFinanceRepository::new(pool.clone());
    let query = FinanceAnalyticsQuery {
        connection_ids: vec![],
        date_from: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
        include_income: true,
        include_expenses: true,
        granularity: Granularity::Week,
        compare_previous: true,
        exclude_inventory_buys: false,
        category: None,
    };

    // Nothing ingested yet: no fee card, which is not the same as zero fees.
    let before = repository
        .analytics(fixture.workspace_id, query.clone())
        .await
        .unwrap();
    assert!(before.kpis.fees.is_none());

    let at = |m: u32, d: u32, h: u32| Utc.with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap();
    let (a, b) = (fixture.first_connection, fixture.second_connection);
    // Previous window (Aug 2 - Aug 31) and the current one (Sep 1 - Sep 30).
    // The journal reaches back to the start of the previous window.
    seed_journal(&pool, b, 10, at(8, 2, 9), "player_donation", "1", "1").await;
    seed_journal(&pool, a, 1, at(8, 20, 9), "brokers_fee", "-40", "900").await;
    seed_journal(&pool, b, 2, at(8, 20, 10), "player_donation", "100", "100").await;
    seed_journal(&pool, a, 3, at(9, 10, 9), "brokers_fee", "-10", "1010").await;
    seed_journal(&pool, a, 4, at(9, 10, 9), "transaction_tax", "-20", "990").await;
    // Same day, later: this is the balance that ends the day.
    seed_journal(&pool, a, 5, at(9, 10, 18), "player_donation", "10", "1000").await;
    seed_journal(
        &pool,
        a,
        6,
        at(9, 11, 9),
        "market_provider_tax",
        "-5",
        "995",
    )
    .await;
    seed_journal(&pool, b, 7, at(9, 12, 9), "player_donation", "100", "200").await;
    seed_journal(&pool, a, 8, at(9, 20, 9), "player_donation", "505", "1500").await;
    // Not a market fee, so never counted as one.
    seed_journal(&pool, a, 9, at(9, 15, 9), "skill_purchase", "-99", "1400").await;

    let analytics = repository
        .analytics(fixture.workspace_id, query.clone())
        .await
        .unwrap();
    let fees = analytics.kpis.fees.as_ref().unwrap();
    assert_eq!(fees.value.0.to_string(), "35.0000");
    assert_eq!(fees.brokers_fee.0.to_string(), "10.0000");
    assert_eq!(fees.transaction_tax.0.to_string(), "20.0000");
    assert_eq!(fees.market_provider_tax.0.to_string(), "5.0000");
    let delta = fees.delta.as_ref().unwrap();
    assert_eq!(delta.previous.0.to_string(), "40.0000");
    assert!((delta.percent.unwrap() + 12.5).abs() < 1e-9);
    assert_eq!(fees.available_from, NaiveDate::from_ymd_opt(2026, 8, 2));
    assert_eq!(fees.sparkline.len(), analytics.cash_flow.len());
    // 1000 of income, 35 of fees: 3.5%, above the insight threshold.
    assert!(analytics
        .insights
        .iter()
        .any(|insight| insight.kind == iskworks_core::InsightKind::FeeBurden));

    // Balance history: both characters are known from Sep 12, when their
    // last balances of the day were 1000 and 200 (not the intra-day 990).
    let wallet = &analytics.kpis.wallet_balance;
    assert_eq!(wallet.value.unwrap().0.to_string(), "1700.0000");
    // 900 + 100 at the end of the previous window.
    let previous = wallet.delta.as_ref().unwrap();
    assert_eq!(previous.previous.0.to_string(), "1000.0000");
    assert!((previous.percent.unwrap() - 70.0).abs() < 1e-9);
    assert!(wallet.sparkline.len() >= 2);

    // Fees cannot be attributed to a category, so the card goes away.
    let filtered = repository
        .analytics(
            fixture.workspace_id,
            FinanceAnalyticsQuery {
                category: Some("Other".into()),
                ..query.clone()
            },
        )
        .await
        .unwrap();
    assert!(filtered.kpis.fees.is_none());

    // A journal that starts after the previous window: no misleading delta.
    sqlx::query("DELETE FROM esi_wallet_journal WHERE occurred_at < '2026-09-01'")
        .execute(&pool)
        .await
        .unwrap();
    let late = repository
        .analytics(fixture.workspace_id, query)
        .await
        .unwrap();
    assert!(late.kpis.fees.as_ref().unwrap().delta.is_none());
}
