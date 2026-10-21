use chrono::{DateTime, Utc};
use iskworks_core::market_orders::{
    CharacterMarketOrder, CharacterMarketOrderRepository, OrderRange,
};
use iskworks_core::MarketOrderSide;
use rust_decimal::Decimal;

use super::*;

fn order(order_id: i64, price: &str) -> CharacterMarketOrder {
    CharacterMarketOrder {
        order_id,
        type_id: 12_005,
        region_id: 10_000_002,
        location_id: 60_003_760,
        side: MarketOrderSide::Sell,
        is_corporation: false,
        price: price.parse::<Decimal>().unwrap(),
        volume_total: 5,
        volume_remain: 5,
        min_volume: 1,
        range: OrderRange::Region,
        issued_at: "2026-10-20T18:00:00Z".parse().unwrap(),
        duration_days: 90,
        escrow: None,
    }
}

fn buy_order(order_id: i64) -> CharacterMarketOrder {
    CharacterMarketOrder {
        side: MarketOrderSide::Buy,
        range: OrderRange::Jumps(5),
        escrow: Some("1105200000.00".parse().unwrap()),
        ..order(order_id, "9210.00")
    }
}

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn replacing_open_orders_closes_missing_ones_and_updates_the_rest(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let repository = PgEsiRepository::new(pool.clone());

    repository
        .replace_open_orders(
            alpha.id,
            &[order(1, "312400000.00"), buy_order(2)],
            at("2026-10-21T10:00:00Z"),
        )
        .await
        .unwrap();
    let repriced = order(1, "311800000.00");
    repository
        .replace_open_orders(
            alpha.id,
            std::slice::from_ref(&repriced),
            at("2026-10-21T10:20:00Z"),
        )
        .await
        .unwrap();

    let open = repository.open_market_orders(workspace_id).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order, repriced);
    assert_eq!(open[0].character_name, "Alpha");
    assert_eq!(open[0].first_seen_at, at("2026-10-21T10:00:00Z"));
    assert_eq!(open[0].last_seen_at, at("2026-10-21T10:20:00Z"));

    let closed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT closed_at FROM character_market_orders WHERE order_id = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(closed_at, Some(at("2026-10-21T10:20:00Z")));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn an_order_returned_again_after_closing_reopens(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let repository = PgEsiRepository::new(pool);

    repository
        .replace_open_orders(alpha.id, &[buy_order(2)], at("2026-10-21T10:00:00Z"))
        .await
        .unwrap();
    repository
        .replace_open_orders(alpha.id, &[], at("2026-10-21T10:20:00Z"))
        .await
        .unwrap();
    assert!(repository
        .open_market_orders(workspace_id)
        .await
        .unwrap()
        .is_empty());

    repository
        .replace_open_orders(alpha.id, &[buy_order(2)], at("2026-10-21T10:40:00Z"))
        .await
        .unwrap();
    let open = repository.open_market_orders(workspace_id).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order, buy_order(2));
    assert_eq!(open[0].first_seen_at, at("2026-10-21T10:00:00Z"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn replacing_one_connections_orders_leaves_others_alone(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let alpha = fixture_connection(&pool, workspace_id, owner_id, "Alpha").await;
    let bravo = fixture_connection(&pool, workspace_id, owner_id, "Bravo").await;
    let (other_workspace, other_owner) = fixture_workspace(&pool).await;
    let outsider = fixture_connection(&pool, other_workspace, other_owner, "Outsider").await;
    let repository = PgEsiRepository::new(pool);

    let now = at("2026-10-21T10:00:00Z");
    repository
        .replace_open_orders(alpha.id, &[order(1, "100.00")], now)
        .await
        .unwrap();
    repository
        .replace_open_orders(bravo.id, &[order(2, "101.00")], now)
        .await
        .unwrap();
    repository
        .replace_open_orders(outsider.id, &[order(3, "102.00")], now)
        .await
        .unwrap();
    repository
        .replace_open_orders(alpha.id, &[], now + chrono::Duration::minutes(20))
        .await
        .unwrap();

    let open = repository.open_market_orders(workspace_id).await.unwrap();
    assert_eq!(
        open.iter()
            .map(|row| (row.character_name.as_str(), row.order.order_id))
            .collect::<Vec<_>>(),
        vec![("Bravo", 2)]
    );
}
