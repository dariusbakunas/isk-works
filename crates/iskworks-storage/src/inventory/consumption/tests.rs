use chrono::{NaiveDate, TimeZone, Utc};
use iskworks_core::{InventoryConsumptionRepository, InventoryRepository, MaterialCategory};

use super::super::tests::{fixture, posting};
use super::*;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

/// Posts `quantity` of the fixture type at `unit` ISK on `on` (noon UTC).
async fn post(
    repository: &PgInventoryRepository,
    key: &InventoryItemKey,
    revision: u64,
    quantity: i64,
    kind: InventoryEventKind,
    on: NaiveDate,
) -> InventoryEventId {
    let mut posting = posting(key, revision, quantity, "10.0000", kind);
    if quantity < 0 {
        posting.total_cost_delta = MoneyDelta(-posting.total_cost_delta.0);
    }
    posting.effective_at = Utc.from_utc_datetime(&on.and_hms_opt(12, 0, 0).unwrap());
    let history = repository.post(posting).await.unwrap();
    history.events.last().unwrap().id
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn consumption_facts_sum_live_consumption_per_day(pool: PgPool) {
    let (key, repository) = fixture(&pool).await;
    post(
        &repository,
        &key,
        0,
        1000,
        InventoryEventKind::OpeningBalance,
        date(9, 1),
    )
    .await;
    post(
        &repository,
        &key,
        1,
        -100,
        InventoryEventKind::Consumption,
        date(10, 5),
    )
    .await;
    post(
        &repository,
        &key,
        2,
        -40,
        InventoryEventKind::Consumption,
        date(10, 5),
    )
    .await;
    post(
        &repository,
        &key,
        3,
        -10,
        InventoryEventKind::Consumption,
        date(9, 20),
    )
    .await;
    let reversed = post(
        &repository,
        &key,
        4,
        -50,
        InventoryEventKind::Consumption,
        date(10, 6),
    )
    .await;
    // A recording reversal's row: consumption itself has no reverse API.
    sqlx::query(
        "INSERT INTO inventory_events (id, workspace_id, owner_id, type_id, captured_name, \
           event_kind, quantity_delta, total_cost_delta, cost_quality, effective_at, recorded_at, \
           sequence, reverses_event_id, resulting_quantity, resulting_total_cost, \
           resulting_average_cost, resulting_revision) \
         SELECT gen_random_uuid(), workspace_id, owner_id, type_id, captured_name, 'reversal', \
           50, 500, 'known', now(), now(), sequence + 1, id, 850, 8500, 10, 6 \
         FROM inventory_events WHERE id = $1",
    )
    .bind(reversed.0)
    .execute(&pool)
    .await
    .unwrap();

    let facts = repository
        .consumption_facts(key.workspace_id, date(10, 1), date(10, 31))
        .await
        .unwrap();

    assert_eq!(facts.days.len(), 1);
    let day = &facts.days[0];
    assert_eq!((day.day, day.type_id, day.quantity), (date(10, 5), 34, 140));
    assert_eq!(day.value, Money::parse("1400").unwrap());
    // Facts cover every consumed type, whatever the range.
    assert_eq!(facts.types.len(), 1);
    let tritanium = &facts.types[0];
    assert_eq!(tritanium.name, "Tritanium");
    assert_eq!(tritanium.category, MaterialCategory::Other);
    assert_eq!(tritanium.first_consumed, date(9, 20));
    assert_eq!(tritanium.on_hand, 800);
    assert_eq!(facts.earliest_observed(), Some(date(9, 20)));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn consumption_facts_are_empty_without_consumption(pool: PgPool) {
    let (key, repository) = fixture(&pool).await;
    post(
        &repository,
        &key,
        0,
        1000,
        InventoryEventKind::OpeningBalance,
        date(9, 1),
    )
    .await;

    let facts = repository
        .consumption_facts(key.workspace_id, date(9, 1), date(10, 31))
        .await
        .unwrap();

    assert!(facts.days.is_empty() && facts.types.is_empty());
}
