//! Market-order / order-book reads: scoped and per-source book retrieval,
//! batched multi-type reads, and the imported/ESI merge. Sort order and
//! filtering are unchanged.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_core::{
    ImportedMarketFileId, MarketError, MarketImportBatchId, MarketOrderBook, MarketOrderSide,
    MarketOrderView, MarketScope, PriceSourceId, WorkspaceId,
};
use uuid::Uuid;

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;

impl PgMarketRepository {
    /// Batched sibling of `get_order_book`/`get_source_order_book`: fetches
    /// order books for many type IDs in a constant number of queries
    /// instead of one round trip per type ID. A type ID with no resolvable
    /// order book (see `MarketError::OrdersUnavailable` on the singular
    /// path) is simply absent from the returned map rather than an error --
    /// callers should treat a missing key the same way they'd treat that
    /// error on the singular path.
    pub(super) async fn get_source_order_books_batch(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
        observed_cutoff: Option<DateTime<Utc>>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        if type_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let source_kind = sqlx::query_scalar::<_, String>(
            "SELECT source_kind FROM price_sources WHERE workspace_id=$1 AND id=$2",
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::PriceSourceNotFound)?;
        if source_kind != "esi_market_orders" {
            return self
                .get_order_books(workspace_id, type_ids, location_id, pinned_batch_id)
                .await;
        }
        // A public scope (NPC station or region-wide) reads the app-wide
        // book; only structure sources keep per-workspace books.
        if let Some((region_id, alias)) = self.public_source_region(workspace_id, source_id).await?
        {
            return self
                .public_order_books(region_id, location_id, &alias, type_ids, observed_cutoff)
                .await;
        }
        // Per covered type, the newest *completed* batch at or before
        // `observed_cutoff` (the graph request's frozen `as_of`). With a
        // `NULL` cutoff this is the newest completed batch, i.e. the same
        // one `coverage.last_completed_batch_id` points at -- the previous
        // behaviour. With a cutoff it is the batch that was current when the
        // evidence resolved, so a refresh completing mid-projection can
        // neither move a later node's price nor blank it. Uses
        // `market_observation_batches_latest_idx`
        // `(workspace_id, price_source_id, type_id, location_id, observed_at DESC) WHERE status='completed'`.
        let batches = sqlx::query_as::<_, BatchedEsiBookBatchRow>(
            r#"
            SELECT batch.id,batch.type_id,batch.captured_type_name,batch.location_id,
                   config.location_alias,batch.solar_system_id,batch.region_id,
                   batch.observed_at,coverage.revalidated_at
            FROM market_source_coverage coverage
            JOIN market_price_source_configs config
              ON config.price_source_id=coverage.price_source_id
            JOIN LATERAL (
              SELECT b.id,b.type_id,b.captured_type_name,b.location_id,
                     b.solar_system_id,b.region_id,b.observed_at
              FROM market_observation_batches b
              WHERE b.workspace_id=coverage.workspace_id
                AND b.price_source_id=coverage.price_source_id
                AND b.type_id=coverage.type_id
                AND b.location_id=$4
                AND b.status='completed'
                AND ($5::timestamptz IS NULL OR b.observed_at<=$5::timestamptz)
              ORDER BY b.observed_at DESC
              LIMIT 1
            ) batch ON true
            WHERE coverage.workspace_id=$1 AND coverage.price_source_id=$2
              AND coverage.type_id=ANY($3)
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(type_ids)
        .bind(location_id)
        .bind(observed_cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if batches.is_empty() {
            return Ok(BTreeMap::new());
        }
        let batch_ids: Vec<Uuid> = batches.iter().map(|batch| batch.id).collect();
        let mut orders_by_batch: BTreeMap<Uuid, Vec<MarketOrderView>> = BTreeMap::new();
        for row in sqlx::query_as::<_, BatchedEsiOrderRow>(
            r#"
            SELECT id,observation_batch_id,order_id,type_id,captured_type_name,order_side,
                   price,remaining_volume,entered_volume,minimum_volume,order_range,
                   issued_at,duration_days,observed_at,location_id,solar_system_id,region_id,jumps
            FROM market_order_observations
            WHERE workspace_id=$1 AND observation_batch_id=ANY($2)
            ORDER BY observation_batch_id,
                     CASE WHEN order_side='sell' THEN 0 ELSE 1 END,
                     CASE WHEN order_side='sell' THEN price END ASC,
                     CASE WHEN order_side='buy' THEN price END DESC,
                     order_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(&batch_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        {
            let batch_id = row.observation_batch_id;
            let order = row.into_order()?;
            orders_by_batch.entry(batch_id).or_default().push(order);
        }
        let books = batches
            .into_iter()
            .map(|batch| {
                let mut orders = orders_by_batch.remove(&batch.id).unwrap_or_default();
                // ESI orders inherit their coverage row's revalidation: a
                // 304 confirms the whole snapshot, every order in it.
                for order in &mut orders {
                    order.revalidated_at = batch.revalidated_at;
                }
                let buys: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Buy)
                    .collect();
                let sells: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Sell)
                    .collect();
                let book = MarketOrderBook {
                    type_id: batch.type_id,
                    type_name: batch.captured_type_name,
                    location_id: batch.location_id,
                    location_name: if batch.location_alias.trim().is_empty() {
                        format!("Station {}", batch.location_id)
                    } else {
                        batch.location_alias
                    },
                    solar_system_id: batch.solar_system_id,
                    region_id: batch.region_id,
                    observed_at: batch.observed_at,
                    revalidated_at: batch.revalidated_at,
                    observation_batch_id: iskworks_core::MarketObservationBatchId(batch.id),
                    import_batch_id: None,
                    imported_file_id: None,
                    buy_order_count: buys.len() as u64,
                    sell_order_count: sells.len() as u64,
                    total_buy_volume: buys.iter().map(|order| order.remaining_volume).sum(),
                    total_sell_volume: sells.iter().map(|order| order.remaining_volume).sum(),
                    lowest_sell: sells.iter().map(|order| order.price).min(),
                    highest_buy: buys.iter().map(|order| order.price).max(),
                    orders,
                };
                (batch.type_id, book)
            })
            .collect();
        Ok(books)
    }

    /// Batched sibling of `get_order_book` -- see `get_source_order_books`.
    pub(super) async fn get_order_books(
        &self,
        workspace_id: WorkspaceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        let files = sqlx::query_as::<_, BatchedBookFileRow>(
            r#"
            SELECT DISTINCT ON (mif.type_id)
                   mif.id,mif.batch_id,mif.type_id,mif.captured_type_name,mif.location_id,
                   COALESCE(mif.captured_location_name,mln.location_name) captured_location_name,
                   mif.solar_system_id,mif.region_id,mif.observed_at
            FROM market_import_files mif
            LEFT JOIN market_location_names mln
              ON mln.workspace_id=mif.workspace_id AND mln.location_id=mif.location_id
            WHERE mif.workspace_id=$1 AND mif.type_id=ANY($2) AND mif.location_id=$3
              AND ($4::uuid IS NULL OR mif.batch_id=$4)
            ORDER BY mif.type_id, mif.observed_at DESC, mif.imported_at DESC, mif.id DESC
            "#,
        )
        .bind(workspace_id.0)
        .bind(type_ids)
        .bind(location_id)
        .bind(pinned_batch_id.map(|id| id.0))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if files.is_empty() {
            return Ok(BTreeMap::new());
        }
        let file_ids: Vec<Uuid> = files.iter().map(|file| file.id).collect();
        let mut orders_by_file: BTreeMap<Uuid, Vec<MarketOrderView>> = BTreeMap::new();
        for row in sqlx::query_as::<_, OrderRow>(
            r#"
            SELECT o.id,f.batch_id,f.id imported_file_id,o.order_id,o.type_id,
                   o.captured_type_name,o.order_side,o.price,o.remaining_volume,
                   o.entered_volume,o.minimum_volume,o.order_range,o.issued_at,o.duration_days,o.observed_at,
                   o.location_id,o.solar_system_id,o.region_id,o.jumps
            FROM market_import_file_observations link
            JOIN market_import_files f ON f.id=link.market_import_file_id
            JOIN market_order_observations o ON o.id=link.market_order_observation_id
            WHERE link.workspace_id=$1 AND link.market_import_file_id=ANY($2)
            ORDER BY f.id,
                     CASE WHEN o.order_side='sell' THEN 0 ELSE 1 END,
                     CASE WHEN o.order_side='sell' THEN o.price END ASC,
                     CASE WHEN o.order_side='buy' THEN o.price END DESC,
                     o.order_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(&file_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        {
            let file_id = row.imported_file_id;
            let order = row.into_order()?;
            orders_by_file.entry(file_id).or_default().push(order);
        }
        let books = files
            .into_iter()
            .map(|file| {
                let orders = orders_by_file.remove(&file.id).unwrap_or_default();
                let buys: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Buy)
                    .collect();
                let sells: Vec<_> = orders
                    .iter()
                    .filter(|order| order.side == MarketOrderSide::Sell)
                    .collect();
                let book = MarketOrderBook {
                    type_id: file.type_id,
                    type_name: file.captured_type_name,
                    location_id: file.location_id,
                    location_name: file
                        .captured_location_name
                        .unwrap_or_else(|| format!("Structure {}", file.location_id)),
                    solar_system_id: file.solar_system_id,
                    region_id: file.region_id,
                    observed_at: file.observed_at,
                    revalidated_at: None,
                    observation_batch_id: iskworks_core::MarketObservationBatchId(file.id),
                    import_batch_id: Some(MarketImportBatchId(file.batch_id)),
                    imported_file_id: Some(ImportedMarketFileId(file.id)),
                    buy_order_count: buys.len() as u64,
                    sell_order_count: sells.len() as u64,
                    total_buy_volume: buys.iter().map(|order| order.remaining_volume).sum(),
                    total_sell_volume: sells.iter().map(|order| order.remaining_volume).sum(),
                    lowest_sell: sells.iter().map(|order| order.price).min(),
                    highest_buy: buys.iter().map(|order| order.price).max(),
                    orders,
                };
                (file.type_id, book)
            })
            .collect();
        Ok(books)
    }

    pub(super) async fn get_source_order_books(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        self.get_source_order_books_batch(
            workspace_id,
            source_id,
            type_ids,
            location_id,
            pinned_batch_id,
            None,
        )
        .await
    }

    pub(super) async fn scoped_order_books(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        self.scoped_order_books_as_of(workspace_id, scope, type_ids, None, None)
            .await
    }

    /// `scoped_order_books`, but with the merge pinned to one evidence
    /// snapshot: ESI batches are cut off at `observed_cutoff`, and the
    /// import half of the merge is fixed to `import_batch_id`. Both `None`
    /// == the live read `scoped_order_books` has always done. Used by
    /// `derive_market_price_items` when a graph request supplied resolved
    /// evidence, so every node on a scope prices from the same batch.
    pub(crate) async fn scoped_order_books_as_of(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        type_ids: &[i64],
        observed_cutoff: Option<DateTime<Utc>>,
        import_batch_id: Option<MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        if type_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut merged: BTreeMap<i64, Vec<MarketOrderView>> = BTreeMap::new();
        // A public scope reads the app-wide book directly -- even a workspace
        // with no price source of its own for it sees prices. A region-wide
        // read also merges the workspace's own non-public sources in the
        // region (structure markets never appear in the public regional
        // feed); its public sources are skipped, their orders already being
        // in the regional book. A station scope needs nothing else, and a
        // structure scope merges the workspace's own source books as before.
        let public_region = self.public_region_for_scope(workspace_id, scope).await?;
        let sources = match public_region {
            Some(region_id) => {
                let books = self
                    .public_order_books(
                        region_id,
                        scope.location_id.unwrap_or(0),
                        "",
                        type_ids,
                        observed_cutoff,
                    )
                    .await?;
                for (type_id, book) in books {
                    merged.entry(type_id).or_default().extend(book.orders);
                }
                if scope.location_id.is_some() {
                    Vec::new()
                } else {
                    let mut private_sources = Vec::new();
                    for source in self.resolve_scope_sources(workspace_id, scope).await? {
                        let source_scope = MarketScope {
                            region_id: scope.region_id,
                            location_id: (source.location_id != 0).then_some(source.location_id),
                        };
                        if self
                            .public_region_for_scope(workspace_id, source_scope)
                            .await?
                            .is_none()
                        {
                            private_sources.push(source);
                        }
                    }
                    private_sources
                }
            }
            None => self.resolve_scope_sources(workspace_id, scope).await?,
        };
        for source in sources {
            let books = self
                .get_source_order_books_batch(
                    workspace_id,
                    PriceSourceId(source.price_source_id),
                    type_ids,
                    source.location_id,
                    import_batch_id,
                    observed_cutoff,
                )
                .await?;
            for (type_id, book) in books {
                merged.entry(type_id).or_default().extend(book.orders);
            }
        }

        // Import-derived observations merge in unconditionally -- whether
        // or not any PriceSource is configured, and alongside ESI data at the
        // same location when both exist (imports are additional market
        // observations alongside ESI data, not a separate source).
        let import_locations = match scope.location_id {
            Some(location_id) => vec![location_id],
            None => {
                self.known_import_locations_in_scope(workspace_id, scope.region_id)
                    .await?
            }
        };
        for location_id in import_locations {
            let books = self
                .get_order_books(workspace_id, type_ids, location_id, import_batch_id)
                .await?;
            for (type_id, book) in books {
                merged.entry(type_id).or_default().extend(book.orders);
            }
        }
        Ok(merged)
    }

    pub(super) async fn get_order_book(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        let file = sqlx::query_as::<_, BookFileRow>(
            r#"
            SELECT mif.id,mif.batch_id,mif.captured_type_name,mif.location_id,
                   COALESCE(mif.captured_location_name,mln.location_name) captured_location_name,
                   mif.solar_system_id,mif.region_id,mif.observed_at
            FROM market_import_files mif
            LEFT JOIN market_location_names mln
              ON mln.workspace_id=mif.workspace_id AND mln.location_id=mif.location_id
            WHERE mif.workspace_id=$1 AND mif.type_id=$2 AND mif.location_id=$3
              AND ($4::uuid IS NULL OR mif.batch_id=$4)
            ORDER BY mif.observed_at DESC,mif.imported_at DESC,mif.id DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id.0)
        .bind(type_id)
        .bind(location_id)
        .bind(pinned_batch_id.map(|id| id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::OrdersUnavailable)?;
        let orders = sqlx::query_as::<_, OrderRow>(
            r#"
            SELECT o.id,f.batch_id,f.id imported_file_id,o.order_id,o.type_id,
                   o.captured_type_name,o.order_side,o.price,o.remaining_volume,
                   o.entered_volume,o.minimum_volume,o.order_range,o.issued_at,o.duration_days,o.observed_at,
                   o.location_id,o.solar_system_id,o.region_id,o.jumps
            FROM market_import_file_observations link
            JOIN market_import_files f ON f.id=link.market_import_file_id
            JOIN market_order_observations o ON o.id=link.market_order_observation_id
            WHERE link.workspace_id=$1 AND link.market_import_file_id=$2
            ORDER BY CASE WHEN o.order_side='sell' THEN 0 ELSE 1 END,
                     CASE WHEN o.order_side='sell' THEN o.price END ASC,
                     CASE WHEN o.order_side='buy' THEN o.price END DESC,
                     o.order_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(file.id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(OrderRow::into_order)
        .collect::<Result<Vec<_>, _>>()?;
        let buys: Vec<_> = orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Buy)
            .collect();
        let sells: Vec<_> = orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Sell)
            .collect();
        Ok(MarketOrderBook {
            type_id,
            type_name: file.captured_type_name,
            location_id: file.location_id,
            location_name: file
                .captured_location_name
                .unwrap_or_else(|| format!("Structure {location_id}")),
            solar_system_id: file.solar_system_id,
            region_id: file.region_id,
            observed_at: file.observed_at,
            revalidated_at: None,
            observation_batch_id: iskworks_core::MarketObservationBatchId(file.id),
            import_batch_id: Some(MarketImportBatchId(file.batch_id)),
            imported_file_id: Some(ImportedMarketFileId(file.id)),
            buy_order_count: buys.len() as u64,
            sell_order_count: sells.len() as u64,
            total_buy_volume: buys.iter().map(|order| order.remaining_volume).sum(),
            total_sell_volume: sells.iter().map(|order| order.remaining_volume).sum(),
            lowest_sell: sells.iter().map(|order| order.price).min(),
            highest_buy: buys.iter().map(|order| order.price).max(),
            orders,
        })
    }

    pub(super) async fn get_source_order_book(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        location_id: i64,
        pinned_batch_id: Option<MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        let source_kind = sqlx::query_scalar::<_, String>(
            "SELECT source_kind FROM price_sources WHERE workspace_id=$1 AND id=$2",
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::PriceSourceNotFound)?;
        if source_kind != "esi_market_orders" {
            return self
                .get_order_book(workspace_id, type_id, location_id, pinned_batch_id)
                .await;
        }
        if let Some((region_id, alias)) = self.public_source_region(workspace_id, source_id).await?
        {
            return self
                .public_order_books(region_id, location_id, &alias, &[type_id], None)
                .await?
                .remove(&type_id)
                .ok_or(MarketError::OrdersUnavailable);
        }
        let batch = sqlx::query_as::<_, EsiBookBatchRow>(
            r#"
            SELECT batch.id,batch.captured_type_name,batch.location_id,
                   config.location_alias,batch.solar_system_id,batch.region_id,
                   batch.observed_at,coverage.revalidated_at
            FROM market_source_coverage coverage
            JOIN market_observation_batches batch
              ON batch.id=coverage.last_completed_batch_id
            JOIN market_price_source_configs config
              ON config.price_source_id=coverage.price_source_id
            WHERE coverage.workspace_id=$1 AND coverage.price_source_id=$2
              AND coverage.type_id=$3 AND batch.location_id=$4
              AND batch.status='completed'
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(type_id)
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(MarketError::OrdersUnavailable)?;
        let mut orders = sqlx::query_as::<_, EsiOrderRow>(
            r#"
            SELECT id,order_id,type_id,captured_type_name,order_side,price,
                   remaining_volume,entered_volume,minimum_volume,order_range,
                   issued_at,duration_days,observed_at,location_id,solar_system_id,region_id,jumps
            FROM market_order_observations
            WHERE workspace_id=$1 AND observation_batch_id=$2
            ORDER BY CASE WHEN order_side='sell' THEN 0 ELSE 1 END,
                     CASE WHEN order_side='sell' THEN price END ASC,
                     CASE WHEN order_side='buy' THEN price END DESC,
                     order_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(batch.id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(EsiOrderRow::into_order)
        .collect::<Result<Vec<_>, _>>()?;
        // ESI orders inherit their coverage row's revalidation.
        for order in &mut orders {
            order.revalidated_at = batch.revalidated_at;
        }
        let buys: Vec<_> = orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Buy)
            .collect();
        let sells: Vec<_> = orders
            .iter()
            .filter(|order| order.side == MarketOrderSide::Sell)
            .collect();
        Ok(MarketOrderBook {
            type_id,
            type_name: batch.captured_type_name,
            location_id: batch.location_id,
            location_name: if batch.location_alias.trim().is_empty() {
                format!("Station {}", batch.location_id)
            } else {
                batch.location_alias
            },
            solar_system_id: batch.solar_system_id,
            region_id: batch.region_id,
            observed_at: batch.observed_at,
            revalidated_at: batch.revalidated_at,
            observation_batch_id: iskworks_core::MarketObservationBatchId(batch.id),
            import_batch_id: None,
            imported_file_id: None,
            buy_order_count: buys.len() as u64,
            sell_order_count: sells.len() as u64,
            total_buy_volume: buys.iter().map(|order| order.remaining_volume).sum(),
            total_sell_volume: sells.iter().map(|order| order.remaining_volume).sum(),
            lowest_sell: sells.iter().map(|order| order.price).min(),
            highest_buy: buys.iter().map(|order| order.price).max(),
            orders,
        })
    }
}
