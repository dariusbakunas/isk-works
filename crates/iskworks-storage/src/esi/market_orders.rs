//! The characters' own market orders per connection (the `market_orders`
//! character source).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::market_orders::{
    CharacterMarketOrder, CharacterMarketOrderRepository, OrderRange, WorkspaceMarketOrder,
};
use iskworks_core::{ConnectedCharacterId, InventoryError, MarketOrderSide, WorkspaceId};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::{map_sqlx, PgEsiRepository};

#[derive(sqlx::FromRow)]
struct OpenOrderRow {
    connection_id: Uuid,
    order_id: i64,
    type_id: i64,
    region_id: i64,
    location_id: i64,
    order_side: String,
    is_corporation: bool,
    price: Decimal,
    volume_total: i64,
    volume_remain: i64,
    min_volume: i64,
    order_range: i32,
    issued_at: DateTime<Utc>,
    duration_days: i32,
    escrow: Option<Decimal>,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    eve_character_id: i64,
    character_name: String,
}

impl OpenOrderRow {
    fn into_domain(self) -> Result<WorkspaceMarketOrder, InventoryError> {
        let side = match self.order_side.as_str() {
            "buy" => MarketOrderSide::Buy,
            "sell" => MarketOrderSide::Sell,
            other => {
                return Err(InventoryError::Persistence(format!(
                    "unknown market order side {other}"
                )))
            }
        };
        let range = OrderRange::from_db_i32(self.order_range).ok_or_else(|| {
            InventoryError::Persistence(format!("unknown order range {}", self.order_range))
        })?;
        Ok(WorkspaceMarketOrder {
            order: CharacterMarketOrder {
                order_id: self.order_id,
                type_id: self.type_id,
                region_id: self.region_id,
                location_id: self.location_id,
                side,
                is_corporation: self.is_corporation,
                price: self.price,
                volume_total: self.volume_total,
                volume_remain: self.volume_remain,
                min_volume: self.min_volume,
                range,
                issued_at: self.issued_at,
                duration_days: self.duration_days,
                escrow: self.escrow,
            },
            connection_id: ConnectedCharacterId(self.connection_id),
            eve_character_id: self.eve_character_id,
            character_name: self.character_name,
            first_seen_at: self.first_seen_at,
            last_seen_at: self.last_seen_at,
        })
    }
}

fn side_db_str(side: MarketOrderSide) -> &'static str {
    match side {
        MarketOrderSide::Buy => "buy",
        MarketOrderSide::Sell => "sell",
    }
}

#[async_trait]
impl CharacterMarketOrderRepository for PgEsiRepository {
    async fn replace_open_orders(
        &self,
        connection_id: ConnectedCharacterId,
        orders: &[CharacterMarketOrder],
        observed_at: DateTime<Utc>,
    ) -> Result<(), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let seen: Vec<i64> = orders.iter().map(|order| order.order_id).collect();
        sqlx::query(
            "UPDATE character_market_orders SET closed_at = $3
             WHERE connection_id = $1 AND closed_at IS NULL AND order_id <> ALL($2)",
        )
        .bind(connection_id.0)
        .bind(&seen)
        .bind(observed_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        for order in orders {
            sqlx::query(
                r#"INSERT INTO character_market_orders (
                     connection_id, order_id, type_id, region_id, location_id, order_side,
                     is_corporation, price, volume_total, volume_remain, min_volume,
                     order_range, issued_at, duration_days, escrow,
                     first_seen_at, last_seen_at, closed_at
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$16,NULL)
                   ON CONFLICT (connection_id, order_id) DO UPDATE SET
                     type_id = EXCLUDED.type_id,
                     region_id = EXCLUDED.region_id,
                     location_id = EXCLUDED.location_id,
                     order_side = EXCLUDED.order_side,
                     is_corporation = EXCLUDED.is_corporation,
                     price = EXCLUDED.price,
                     volume_total = EXCLUDED.volume_total,
                     volume_remain = EXCLUDED.volume_remain,
                     min_volume = EXCLUDED.min_volume,
                     order_range = EXCLUDED.order_range,
                     issued_at = EXCLUDED.issued_at,
                     duration_days = EXCLUDED.duration_days,
                     escrow = EXCLUDED.escrow,
                     last_seen_at = EXCLUDED.last_seen_at,
                     closed_at = NULL"#,
            )
            .bind(connection_id.0)
            .bind(order.order_id)
            .bind(order.type_id)
            .bind(order.region_id)
            .bind(order.location_id)
            .bind(side_db_str(order.side))
            .bind(order.is_corporation)
            .bind(order.price)
            .bind(order.volume_total)
            .bind(order.volume_remain)
            .bind(order.min_volume)
            .bind(order.range.as_db_i32())
            .bind(order.issued_at)
            .bind(order.duration_days)
            .bind(order.escrow)
            .bind(observed_at)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }
        tx.commit().await.map_err(map_sqlx)
    }

    async fn open_market_orders(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<WorkspaceMarketOrder>, InventoryError> {
        sqlx::query_as::<_, OpenOrderRow>(
            "SELECT o.connection_id, o.order_id, o.type_id, o.region_id, o.location_id,
                    o.order_side, o.is_corporation, o.price, o.volume_total, o.volume_remain,
                    o.min_volume, o.order_range, o.issued_at, o.duration_days, o.escrow,
                    o.first_seen_at, o.last_seen_at, c.eve_character_id, c.character_name
             FROM character_market_orders o
             JOIN eve_connections c ON c.id = o.connection_id
             WHERE c.workspace_id = $1 AND c.disconnected_at IS NULL AND o.closed_at IS NULL
             ORDER BY c.character_name, o.order_id",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(OpenOrderRow::into_domain)
        .collect()
    }
}
