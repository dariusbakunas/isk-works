//! Freshness projections used by Market Browser invalidation: per-scope and
//! per-item recency. These report *effective* freshness --
//! `GREATEST(batch.observed_at, COALESCE(coverage.revalidated_at, batch.observed_at))`
//! -- so a snapshot ESI has since confirmed unchanged via a `304` reads as
//! fresh-as-of-that-confirmation, not as-of the original fetch. The
//! `COALESCE` covers the (pre-backfill / mid-flight) NULL case; the outer
//! `GREATEST` defends against a malformed `revalidated_at < observed_at`.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_core::{MarketError, ScopeFreshness, WorkspaceId};

use super::convert::*;
use super::rows::*;
use super::PgMarketRepository;

#[derive(sqlx::FromRow)]
struct PublicRegionFreshnessRow {
    region_id: i64,
    tracked_type_count: i64,
    observed_type_count: i64,
    most_recent_observed_at: Option<DateTime<Utc>>,
}

impl PgMarketRepository {
    pub(super) async fn scope_freshness(
        &self,
        workspace_id: WorkspaceId,
        scopes: &[(i64, i64)],
    ) -> Result<BTreeMap<(i64, i64), ScopeFreshness>, MarketError> {
        if scopes.is_empty() {
            return Ok(BTreeMap::new());
        }
        // Public scopes (region-wide `0`, or an NPC station) are answered from
        // the app-wide rows of their region -- every public scope in a region
        // shares one regional book, so its counts are app-wide. Structure
        // scopes keep the workspace's own coverage.
        let mut result = BTreeMap::new();
        let mut private_scopes = Vec::new();
        let mut public_regions: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
        for &(region_id, location_id) in scopes {
            let scope = iskworks_core::MarketScope {
                region_id,
                location_id: (location_id != 0).then_some(location_id),
            };
            match self.public_region_for_scope(workspace_id, scope).await? {
                Some(region_id) => public_regions
                    .entry(region_id)
                    .or_default()
                    .push((region_id, location_id)),
                None => private_scopes.push((region_id, location_id)),
            }
        }
        if !public_regions.is_empty() {
            let regions: Vec<i64> = public_regions.keys().copied().collect();
            for row in sqlx::query_as::<_, PublicRegionFreshnessRow>(
                r#"
                SELECT coverage.region_id,
                       count(*) AS tracked_type_count,
                       count(*) FILTER (WHERE coverage.last_completed_batch_id IS NOT NULL)
                         AS observed_type_count,
                       max(GREATEST(
                         batch.observed_at,
                         COALESCE(coverage.revalidated_at, batch.observed_at)
                       )) AS most_recent_observed_at
                FROM public_market_coverage coverage
                LEFT JOIN market_observation_batches batch
                  ON batch.id = coverage.last_completed_batch_id
                WHERE coverage.region_id = ANY($1)
                GROUP BY coverage.region_id
                "#,
            )
            .bind(&regions)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?
            {
                for key in public_regions.get(&row.region_id).into_iter().flatten() {
                    result.insert(
                        *key,
                        ScopeFreshness {
                            tracked_type_count: row.tracked_type_count,
                            observed_type_count: row.observed_type_count,
                            most_recent_observed_at: row.most_recent_observed_at,
                        },
                    );
                }
            }
        }
        if private_scopes.is_empty() {
            return Ok(result);
        }
        let scopes = private_scopes;
        let region_ids: Vec<i64> = scopes.iter().map(|(region_id, _)| *region_id).collect();
        let location_ids: Vec<i64> = scopes.iter().map(|(_, location_id)| *location_id).collect();
        sqlx::query_as::<_, ScopeFreshnessRow>(
            r#"
            SELECT coverage.region_id, coverage.location_id,
                   count(*) AS tracked_type_count,
                   count(*) FILTER (WHERE coverage.last_completed_batch_id IS NOT NULL)
                     AS observed_type_count,
                   max(GREATEST(
                     batch.observed_at,
                     COALESCE(coverage.revalidated_at, batch.observed_at)
                   )) AS most_recent_observed_at
            FROM market_source_coverage coverage
            LEFT JOIN market_observation_batches batch
              ON batch.id = coverage.last_completed_batch_id
            WHERE coverage.workspace_id=$1
              AND (coverage.region_id, coverage.location_id) IN (
                SELECT * FROM UNNEST($2::bigint[], $3::bigint[])
              )
            GROUP BY coverage.region_id, coverage.location_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(&region_ids)
        .bind(&location_ids)
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            result.extend(rows.into_iter().map(|row| {
                (
                    (row.region_id, row.location_id),
                    ScopeFreshness {
                        tracked_type_count: row.tracked_type_count,
                        observed_type_count: row.observed_type_count,
                        most_recent_observed_at: row.most_recent_observed_at,
                    },
                )
            }));
            result
        })
        .map_err(map_sqlx)
    }

    pub(super) async fn item_freshness(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<Option<DateTime<Utc>>, MarketError> {
        if type_ids.is_empty() {
            return Ok(None);
        }
        if let Some(region_id) = self.public_region_for_scope(workspace_id, scope).await? {
            return sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                r#"
                SELECT max(GREATEST(
                  batch.observed_at,
                  COALESCE(coverage.revalidated_at, batch.observed_at)
                ))
                FROM public_market_coverage coverage
                JOIN market_observation_batches batch ON batch.id = coverage.last_completed_batch_id
                WHERE coverage.region_id=$1 AND coverage.type_id = ANY($2)
                "#,
            )
            .bind(region_id)
            .bind(type_ids)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx);
        }
        let location_id = scope.location_id.unwrap_or(0);
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            r#"
            SELECT max(GREATEST(
              batch.observed_at,
              COALESCE(coverage.revalidated_at, batch.observed_at)
            ))
            FROM market_source_coverage coverage
            JOIN market_observation_batches batch ON batch.id = coverage.last_completed_batch_id
            WHERE coverage.workspace_id=$1 AND coverage.region_id=$2 AND coverage.location_id=$3
              AND coverage.type_id = ANY($4)
            "#,
        )
        .bind(workspace_id.0)
        .bind(scope.region_id)
        .bind(location_id)
        .bind(type_ids)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }
}
