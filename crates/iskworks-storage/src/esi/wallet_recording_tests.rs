//! Finance Market Buy -> Inventory recording, against real Postgres.
//!
//! These prove the ONE-source-of-truth contract: Finance and generic ledger
//! reversal both move the same provenance rows.

use iskworks_core::{
    adjustment_posting, purchase_posting, CostInputQuality, FinanceInventoryState,
    FinanceTransactionFilter, FinanceTransactionSort, InventoryRepository, PostInventoryCommand,
};
use iskworks_core::{InventoryBalance, InventoryItemKey};
use serde_json::json;
use sqlx::PgPool;

use super::tests::{fixture_connection, fixture_workspace};
use super::*;
use crate::finance::PgFinanceRepository;

const TRITANIUM: i64 = 34;
const PYERITE: i64 = 35;

struct Fx {
    pool: PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    connection: ConnectedCharacter,
    esi: PgEsiRepository,
    finance: PgFinanceRepository,
    inventory: PgInventoryRepository,
}

async fn fx(pool: PgPool) -> Fx {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Buyer").await;
    let import_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1,'test','fixture','wallet-recording','active',true,now(),now())",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    for (type_id, name) in [(TRITANIUM, "Tritanium"), (PYERITE, "Pyerite")] {
        sqlx::query(
            "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1,$2,$3,true)",
        )
        .bind(import_id)
        .bind(type_id)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }
    Fx {
        esi: PgEsiRepository::new(pool.clone()),
        finance: PgFinanceRepository::new(pool.clone()),
        inventory: PgInventoryRepository::new(pool.clone()),
        pool,
        workspace_id,
        owner_id,
        connection,
    }
}

impl Fx {
    /// Imports through the real importer, so the wallet transaction exists
    /// exactly as it does in production. Returns the observation id.
    async fn import(
        &self,
        transaction_id: i64,
        type_id: i64,
        quantity: i64,
        unit_price: Decimal,
        is_buy: bool,
    ) -> Uuid {
        self.import_for(
            &self.connection,
            transaction_id,
            type_id,
            quantity,
            unit_price,
            is_buy,
        )
        .await
    }

    async fn import_for(
        &self,
        connection: &ConnectedCharacter,
        transaction_id: i64,
        type_id: i64,
        quantity: i64,
        unit_price: Decimal,
        is_buy: bool,
    ) -> Uuid {
        let run = self
            .esi
            .start_sync(connection, EsiSyncKind::WalletTransactions)
            .await
            .unwrap();
        self.esi
            .complete_wallet(
                &run,
                &[WalletTransactionObservation {
                    transaction_id,
                    client_id: 1,
                    location_id: 60_003_760,
                    type_id,
                    quantity,
                    unit_price,
                    is_buy,
                    is_personal: true,
                    journal_ref_id: transaction_id + 1,
                    transacted_at: Utc::now(),
                    raw: json!({ "transaction_id": transaction_id }),
                }],
                None,
            )
            .await
            .unwrap();
        sqlx::query_scalar("SELECT id FROM esi_wallet_transactions WHERE connection_id = $1 AND source_transaction_id = $2")
            .bind(connection.id.0)
            .bind(transaction_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn balance(&self, type_id: i64) -> Option<InventoryBalance> {
        self.inventory
            .list_balances(self.workspace_id, self.owner_id)
            .await
            .unwrap()
            .into_iter()
            .find(|balance| balance.key.type_id == type_id)
    }

    async fn event_count(&self, type_id: i64) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM inventory_events WHERE workspace_id = $1 AND type_id = $2",
        )
        .bind(self.workspace_id.0)
        .bind(type_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn finance_row(&self, observation_id: Uuid) -> iskworks_core::FinanceTransaction {
        self.finance
            .transactions(
                self.workspace_id,
                FinanceTransactionFilter::default(),
                FinanceTransactionSort::default(),
            )
            .await
            .unwrap()
            .rows
            .into_iter()
            .find(|row| row.observation_id == observation_id)
            .expect("finance lists the transaction")
    }

    fn import_posting(
        &self,
        type_id: i64,
        name: &str,
        quantity: u64,
        unit: &str,
    ) -> InventoryPosting {
        purchase_posting(
            self.workspace_id,
            self.owner_id,
            PostInventoryCommand {
                type_id,
                type_name: name.to_string(),
                quantity,
                unit_cost: Some(unit.to_string()),
                cost_quality: CostInputQuality::Known,
                source_reference: "imports".to_string(),
                note: String::new(),
                effective_at: Utc::now(),
                expected_revision: 0,
                acknowledge_zero_cost: false,
            },
        )
        .unwrap()
    }
}

fn price(text: &str) -> Decimal {
    Decimal::from_str_exact(text).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn records_exact_type_quantity_basis_and_owner(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx
        .import(9001, TRITANIUM, 50_000, price("729.2000"), true)
        .await;
    let before = fx.finance_row(observation).await;
    assert_eq!(
        before.inventory_recording.as_ref().unwrap().state,
        FinanceInventoryState::Unrecorded
    );

    let (recording, created) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created);
    assert_eq!(recording.state, FinanceInventoryState::Recorded);
    assert_eq!(recording.quantity, Some(50_000));
    assert_eq!(
        recording.total_basis.unwrap().0.to_string(),
        "36460000.0000"
    );

    let balance = fx.balance(TRITANIUM).await.unwrap();
    assert_eq!(balance.key.owner_id, fx.owner_id);
    assert_eq!(balance.quantity, 50_000);
    assert_eq!(balance.total_historical_cost.0.to_string(), "36460000.0000");

    let event = sqlx::query_as::<_, (String, i64, Decimal, String, String)>(
        "SELECT event_kind, quantity_delta, total_cost_delta, cost_quality, source_reference FROM inventory_events WHERE workspace_id = $1 AND type_id = $2",
    )
    .bind(fx.workspace_id.0)
    .bind(TRITANIUM)
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(event.0, "purchase");
    assert_eq!(event.1, 50_000);
    assert_eq!(event.2.to_string(), "36460000.0000");
    assert_eq!(event.3, "known");
    assert_eq!(event.4, "EVE wallet transaction 9001");

    // The Finance transaction itself is untouched; only its recording state moved.
    let after = fx.finance_row(observation).await;
    assert_eq!(after.quantity, before.quantity);
    assert_eq!(after.unit_price, before.unit_price);
    assert_eq!(after.total_price, before.total_price);
    assert_eq!(
        after.inventory_recording.unwrap().recording_id,
        recording.recording_id
    );
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Recorded
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn a_synced_buy_is_recordable_without_any_review_state(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9010, TRITANIUM, 25, price("4.0000"), true).await;
    // Finance alone decides recordability: a personal buy of a known type,
    // with no per-transaction review row involved.
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Unrecorded
    );

    let (recording, created) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created);
    assert_eq!(recording.state, FinanceInventoryState::Recorded);
    assert_eq!(fx.balance(TRITANIUM).await.unwrap().quantity, 25);

    fx.esi
        .revert_wallet_purchase_recording(
            fx.workspace_id,
            observation,
            recording.recording_id.unwrap(),
        )
        .await
        .unwrap();
    let (again, created_again) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created_again);
    assert_eq!(again.state, FinanceInventoryState::Recorded);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn sequential_double_record_posts_once(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9002, TRITANIUM, 10, price("5.0000"), true).await;
    let (first, created_first) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    let (second, created_second) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created_first && !created_second);
    assert_eq!(first, second);
    assert_eq!(fx.event_count(TRITANIUM).await, 1);
    assert_eq!(fx.balance(TRITANIUM).await.unwrap().quantity, 10);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn concurrent_double_record_creates_exactly_one_purchase(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9003, TRITANIUM, 100, price("2.5000"), true).await;
    let attempts = (0..8).map(|_| {
        let esi = fx.esi.clone();
        let workspace_id = fx.workspace_id;
        tokio::spawn(async move { esi.record_wallet_purchase(workspace_id, observation).await })
    });
    let results = join_all(attempts).await;
    assert_eq!(results.iter().filter(|(_, created)| *created).count(), 1);
    assert_eq!(fx.event_count(TRITANIUM).await, 1);
    assert_eq!(fx.balance(TRITANIUM).await.unwrap().quantity, 100);
}

async fn join_all<I>(handles: I) -> Vec<(iskworks_core::FinanceInventoryRecording, bool)>
where
    I: Iterator<
        Item = tokio::task::JoinHandle<
            Result<(iskworks_core::FinanceInventoryRecording, bool), InventoryError>,
        >,
    >,
{
    let mut results = Vec::new();
    for handle in handles.collect::<Vec<_>>() {
        results.push(
            handle
                .await
                .unwrap()
                .expect("every racer resolves to the recorded state"),
        );
    }
    results
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn distinct_identical_transactions_both_record(pool: PgPool) {
    let fx = fx(pool).await;
    let a = fx
        .import(9004, TRITANIUM, 1_000, price("4.0000"), true)
        .await;
    let b = fx
        .import(9005, TRITANIUM, 1_000, price("4.0000"), true)
        .await;
    fx.esi
        .record_wallet_purchase(fx.workspace_id, a)
        .await
        .unwrap();
    fx.esi
        .record_wallet_purchase(fx.workspace_id, b)
        .await
        .unwrap();
    assert_eq!(fx.event_count(TRITANIUM).await, 2);
    let balance = fx.balance(TRITANIUM).await.unwrap();
    assert_eq!(balance.quantity, 2_000);
    assert_eq!(balance.total_historical_cost.0.to_string(), "8000.0000");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn concurrent_first_posts_of_a_new_type_both_succeed(pool: PgPool) {
    let fx = fx(pool).await;
    let a = fx.import(9006, PYERITE, 10, price("1.0000"), true).await;
    let b = fx.import(9007, PYERITE, 20, price("1.0000"), true).await;
    let handles = [a, b].into_iter().map(|observation| {
        let esi = fx.esi.clone();
        let workspace_id = fx.workspace_id;
        tokio::spawn(async move { esi.record_wallet_purchase(workspace_id, observation).await })
    });
    join_all(handles).await;
    assert_eq!(fx.event_count(PYERITE).await, 2);
    let balance = fx.balance(PYERITE).await.unwrap();
    assert_eq!((balance.quantity, balance.revision), (30, 2));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn ineligible_and_inaccessible_transactions_are_rejected(pool: PgPool) {
    let fx = fx(pool.clone()).await;
    let sell = fx.import(9008, TRITANIUM, 5, price("3.0000"), false).await;
    assert!(
        matches!(
            fx.esi.record_wallet_purchase(fx.workspace_id, sell).await,
            Err(InventoryError::Validation(_))
        ),
        "a sell can never be recorded"
    );
    assert!(fx.finance_row(sell).await.inventory_recording.is_none());

    let unknown_type = fx.import(9009, 999_999, 5, price("3.0000"), true).await;
    assert_eq!(
        fx.finance_row(unknown_type)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Unavailable
    );
    assert!(matches!(
        fx.esi
            .record_wallet_purchase(fx.workspace_id, unknown_type)
            .await,
        Err(InventoryError::Validation(_))
    ));

    let buy = fx.import(9010, TRITANIUM, 5, price("3.0000"), true).await;
    let (other_workspace, _) = fixture_workspace(&pool).await;
    assert!(matches!(
        fx.esi.record_wallet_purchase(other_workspace, buy).await,
        Err(InventoryError::ItemNotFound)
    ));
    assert!(matches!(
        fx.esi
            .record_wallet_purchase(fx.workspace_id, Uuid::new_v4())
            .await,
        Err(InventoryError::ItemNotFound)
    ));
    sqlx::query("UPDATE eve_connections SET disconnected_at = now() WHERE id = $1")
        .bind(fx.connection.id.0)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        fx.esi.record_wallet_purchase(fx.workspace_id, buy).await,
        Err(InventoryError::ItemNotFound)
    ));
    assert_eq!(fx.event_count(TRITANIUM).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn reverting_compensates_marks_history_and_allows_a_fresh_recording(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9015, TRITANIUM, 50, price("10.0000"), true).await;
    let (first, _) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    let first_id = first.recording_id.unwrap();

    let reverted = fx
        .esi
        .revert_wallet_purchase_recording(fx.workspace_id, observation, first_id)
        .await
        .unwrap();
    assert_eq!(reverted.state, FinanceInventoryState::Reverted);
    assert!(reverted.reverted_at.is_some());
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Reverted
    );
    let balance = fx.balance(TRITANIUM).await.unwrap();
    assert_eq!(balance.quantity, 0);
    assert!(balance.total_historical_cost.0.is_zero());
    // The original purchase event is preserved and linked to its exact compensation.
    let (reversal_kind, reverses, quantity_delta, cost_delta): (String, Option<Uuid>, i64, Decimal) = sqlx::query_as(
        "SELECT event_kind, reverses_event_id, quantity_delta, total_cost_delta FROM inventory_events WHERE reverses_event_id = $1",
    )
    .bind(first_id)
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(
        (reversal_kind.as_str(), reverses),
        ("reversal", Some(first_id))
    );
    assert_eq!(
        (quantity_delta, cost_delta.to_string()),
        (-50, "-500.0000".to_string())
    );
    assert_eq!(fx.event_count(TRITANIUM).await, 2);

    // Double reversal is rejected cleanly.
    assert!(matches!(
        fx.esi
            .revert_wallet_purchase_recording(fx.workspace_id, observation, first_id)
            .await,
        Err(InventoryError::AlreadyReversed)
    ));

    // Explicit re-record creates a NEW recording; the reverted one stays as history.
    let (second, created) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created);
    assert_eq!(second.state, FinanceInventoryState::Recorded);
    assert_ne!(second.recording_id, Some(first_id));
    let rows: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT inventory_event_id, reverted_at IS NULL FROM inventory_event_sources WHERE observation_id = $1 ORDER BY accepted_at",
    )
    .bind(observation)
    .fetch_all(&fx.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![(first_id, false), (second.recording_id.unwrap(), true)]
    );
    assert_eq!(fx.balance(TRITANIUM).await.unwrap().quantity, 50);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn the_database_refuses_a_second_active_recording(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9016, TRITANIUM, 5, price("1.0000"), true).await;
    let (recording, _) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();

    // A second purchase event exists in the ledger...
    let revision = fx.balance(TRITANIUM).await.unwrap().revision;
    let mut extra = fx.import_posting(TRITANIUM, "Tritanium", 5, "1.0");
    extra.expected_revision = revision;
    let extra_id = extra.id.0;
    fx.inventory.post(extra).await.unwrap();

    // ...but claiming it as a second ACTIVE recording of the same wallet
    // transaction, bypassing every application check, violates the index.
    let duplicate = sqlx::query(
        r#"INSERT INTO inventory_event_sources (
             inventory_event_id, workspace_id, owner_id, source_system, source_record_kind,
             source_record_id, accounting_effect_kind, connection_id, observation_id,
             sync_run_id, source_transaction_at, accepted_at)
           SELECT $1, workspace_id, owner_id, source_system, source_record_kind,
                  source_record_id, accounting_effect_kind, connection_id, observation_id,
                  sync_run_id, source_transaction_at, now()
           FROM inventory_event_sources WHERE inventory_event_id = $2"#,
    )
    .bind(extra_id)
    .bind(recording.recording_id.unwrap())
    .execute(&fx.pool)
    .await
    .unwrap_err();
    let code = duplicate
        .as_database_error()
        .and_then(|error| error.code().map(|c| c.to_string()));
    assert_eq!(code.as_deref(), Some("23505"), "{duplicate:?}");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn reverting_after_stock_was_consumed_is_rejected_atomically(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx
        .import(9017, TRITANIUM, 50_000, price("729.2000"), true)
        .await;
    let (recording, _) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    let current = fx.balance(TRITANIUM).await.unwrap();
    fx.inventory
        .post(
            adjustment_posting(
                &current,
                -40_000,
                "Tritanium".to_string(),
                None,
                "consumed".to_string(),
                String::new(),
                Utc::now(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let events_before = fx.event_count(TRITANIUM).await;
    let balance_before = fx.balance(TRITANIUM).await.unwrap();

    let error = fx
        .esi
        .revert_wallet_purchase_recording(
            fx.workspace_id,
            observation,
            recording.recording_id.unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, InventoryError::Validation(_)), "{error:?}");

    assert_eq!(fx.event_count(TRITANIUM).await, events_before);
    assert_eq!(fx.balance(TRITANIUM).await.unwrap(), balance_before);
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Recorded
    );
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Recorded
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn generic_ledger_reversal_of_a_sourced_purchase_settles_its_provenance(pool: PgPool) {
    let fx = fx(pool).await;
    let observation = fx.import(9018, TRITANIUM, 30, price("2.0000"), true).await;
    let (recording, _) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    let key = InventoryItemKey {
        workspace_id: fx.workspace_id,
        owner_id: fx.owner_id,
        type_id: TRITANIUM,
    };
    let revision = fx.balance(TRITANIUM).await.unwrap().revision;
    fx.inventory
        .reverse_latest(
            &key,
            InventoryEventId(recording.recording_id.unwrap()),
            revision,
            "wrong".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Reverted
    );
    assert_eq!(
        fx.finance_row(observation)
            .await
            .inventory_recording
            .unwrap()
            .state,
        FinanceInventoryState::Reverted
    );
    // ... and the explicit "record again" now works.
    let (again, created) = fx
        .esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    assert!(created);
    assert_eq!(again.state, FinanceInventoryState::Recorded);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn recording_moves_the_shared_balance_that_esi_reconciliation_reads(pool: PgPool) {
    // ESI reconciliation is `included observed - accounting balance quantity`,
    // both read from `inventory_balances`. Recording through Finance moves that
    // one table, so the difference shrinks with no ESI mutation at all.
    let fx = fx(pool).await;
    let observation = fx
        .import(9019, TRITANIUM, 50_000, price("729.2000"), true)
        .await;
    let esi_observed = 50_000_i64;
    let before = esi_observed - fx.balance(TRITANIUM).await.map_or(0, |b| b.quantity as i64);
    fx.esi
        .record_wallet_purchase(fx.workspace_id, observation)
        .await
        .unwrap();
    let after = esi_observed - fx.balance(TRITANIUM).await.unwrap().quantity as i64;
    assert_eq!((before, after), (50_000, 0));
    let asset_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM esi_asset_observations")
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    let exclusions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inventory_reconciliation_exclusions")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!((asset_rows, exclusions), (0, 0));
}

fn journal_entry(ref_id: i64, ref_type: &str, amount: &str) -> WalletJournalObservation {
    WalletJournalObservation {
        ref_id,
        date: "2026-09-28T10:00:00Z".parse().unwrap(),
        ref_type: ref_type.to_string(),
        amount: Decimal::from_str_exact(amount).unwrap(),
        balance: Some(Decimal::from_str_exact("1000.5000").unwrap()),
        first_party_id: Some(1),
        second_party_id: None,
        context_id: Some(77),
        context_id_type: Some("market_transaction_id".to_string()),
        description: Some("Broker fee".to_string()),
        reason: None,
        tax: None,
        tax_receiver_id: None,
        raw: json!({"id": ref_id}),
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn wallet_journal_ingestion_is_idempotent_and_keeps_signs(pool: PgPool) {
    let fx = fx(pool).await;
    let entries = [
        journal_entry(1, "brokers_fee", "-12.3400"),
        journal_entry(2, "transaction_tax", "-4.5600"),
        journal_entry(3, "player_donation", "100.0000"),
    ];
    let connection_id = fx.connection.id;

    assert_eq!(
        fx.esi
            .save_wallet_journal(connection_id, &entries)
            .await
            .unwrap(),
        3
    );
    // A second sync sees the same 30-day window plus one new entry.
    let mut again = entries.to_vec();
    again.push(journal_entry(4, "brokers_fee", "-1.0000"));
    assert_eq!(
        fx.esi
            .save_wallet_journal(connection_id, &again)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        fx.esi
            .save_wallet_journal(connection_id, &[])
            .await
            .unwrap(),
        0
    );

    let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT ref_id, ref_type, amount::text, context_id_type FROM esi_wallet_journal WHERE connection_id=$1 ORDER BY ref_id",
    )
    .bind(connection_id.0)
    .fetch_all(&fx.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].2, "-12.3400");
    assert_eq!(rows[2].1, "player_donation");
    assert_eq!(rows[0].3.as_deref(), Some("market_transaction_id"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn analytics_and_transactions_can_leave_out_buys_recorded_into_inventory(pool: PgPool) {
    use iskworks_core::{
        FinanceAnalyticsQuery, FinanceAnalyticsRepository, FinanceDirection, Granularity,
    };

    let fx = fx(pool).await;
    let recorded = fx
        .import(9101, TRITANIUM, 10, price("100.0000"), true)
        .await;
    fx.import(9102, PYERITE, 5, price("40.0000"), true).await;
    fx.import(9103, TRITANIUM, 1, price("500.0000"), false)
        .await;
    fx.esi
        .record_wallet_purchase(fx.workspace_id, recorded)
        .await
        .unwrap();

    let today = Utc::now().date_naive();
    let query = |exclude: bool| FinanceAnalyticsQuery {
        connection_ids: vec![],
        date_from: today - chrono::Duration::days(1),
        date_to: today,
        include_income: true,
        include_expenses: true,
        granularity: Granularity::Day,
        compare_previous: false,
        exclude_inventory_buys: exclude,
        category: None,
    };
    let kept = fx
        .finance
        .analytics(fx.workspace_id, query(false))
        .await
        .unwrap();
    assert_eq!(kept.kpis.expenses.value.0.to_string(), "1200.0000");
    assert_eq!(kept.excluded_inventory_buys.transaction_count, 0);

    let excluded = fx
        .finance
        .analytics(fx.workspace_id, query(true))
        .await
        .unwrap();
    // Only the unrecorded 5 x 40 buy remains; the recorded 10 x 100 is a build input.
    assert_eq!(excluded.kpis.expenses.value.0.to_string(), "200.0000");
    assert_eq!(excluded.kpis.income.value.0.to_string(), "500.0000");
    assert_eq!(excluded.excluded_inventory_buys.transaction_count, 1);
    assert_eq!(
        excluded.excluded_inventory_buys.total_isk.0.to_string(),
        "1000.0000"
    );

    // The Transactions list agrees, so a deep link shows the same rows.
    let listed = |exclude: bool| {
        let finance = fx.finance.clone();
        let workspace_id = fx.workspace_id;
        async move {
            finance
                .transactions(
                    workspace_id,
                    FinanceTransactionFilter {
                        direction: FinanceDirection::Expense,
                        exclude_inventory_buys: exclude,
                        ..FinanceTransactionFilter::default()
                    },
                    FinanceTransactionSort::default(),
                )
                .await
                .unwrap()
                .rows
                .len()
        }
    };
    assert_eq!(listed(false).await, 2);
    assert_eq!(listed(true).await, 1);

    // Reverting the recording puts the buy back.
    let recording = fx.finance_row(recorded).await.inventory_recording.unwrap();
    fx.esi
        .revert_wallet_purchase_recording(
            fx.workspace_id,
            recorded,
            recording.recording_id.unwrap(),
        )
        .await
        .unwrap();
    let reverted = fx
        .finance
        .analytics(fx.workspace_id, query(true))
        .await
        .unwrap();
    assert_eq!(reverted.kpis.expenses.value.0.to_string(), "1200.0000");
}
