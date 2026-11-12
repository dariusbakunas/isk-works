//! The character's open market orders.

use chrono::{DateTime, Utc};
use iskworks_core::{
    market_orders::{CharacterMarketOrder, OrderRange},
    ConnectedCharacter, MarketOrderSide,
};
use iskworks_esi::CharacterOrderObservation;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

fn market_order(order: CharacterOrderObservation) -> Result<CharacterMarketOrder, SourceFailure> {
    let range = OrderRange::from_esi(&order.order_range).ok_or_else(|| {
        SourceFailure::from(format!(
            "order {} has an unknown range {:?}",
            order.order_id, order.order_range
        ))
    })?;
    let count = |value: u64| {
        i64::try_from(value).map_err(|_| {
            SourceFailure::from(format!("order {} volume is out of range", order.order_id))
        })
    };
    Ok(CharacterMarketOrder {
        order_id: order.order_id,
        type_id: order.type_id,
        region_id: order.region_id,
        location_id: order.location_id,
        side: if order.is_buy_order {
            MarketOrderSide::Buy
        } else {
            MarketOrderSide::Sell
        },
        is_corporation: order.is_corporation,
        price: order.price,
        volume_total: count(order.volume_total)?,
        volume_remain: count(order.volume_remain)?,
        min_volume: count(order.min_volume)?,
        range,
        issued_at: order.issued_at,
        duration_days: i32::try_from(order.duration_days).unwrap_or(i32::MAX),
        escrow: order.escrow,
    })
}

impl CharacterSyncService {
    /// Replaces the connection's open orders with ESI's active list; orders
    /// it no longer lists are closed. A failed fetch or an unreadable order
    /// fails the source before anything is stored, so open orders are never
    /// closed by mistake. Corporation orders are stored but counted apart.
    pub(in crate::character_sync) async fn sync_market_orders(
        &self,
        access_token: &str,
        connection: &ConnectedCharacter,
        now: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let orders = self
            .transport
            .character_orders(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?
            .records
            .into_iter()
            .map(market_order)
            .collect::<Result<Vec<_>, _>>()?;
        self.repository
            .store_market_orders(connection.id, &orders, now)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let personal = |side| {
            orders
                .iter()
                .filter(|order| !order.is_corporation && order.side == side)
                .count()
        };
        Ok(serde_json::json!({
            "openSellOrders": personal(MarketOrderSide::Sell),
            "openBuyOrders": personal(MarketOrderSide::Buy),
            "corporationOrders": orders.iter().filter(|order| order.is_corporation).count(),
        }))
    }
}
