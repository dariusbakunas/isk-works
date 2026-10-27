//! What a saved fit costs on the workspace's market, and where the
//! workspace's characters already hold it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_core::fitting::{fit_cost, fit_needs, AssetHolding, CostLine, FitCost, FitId};
use iskworks_core::{summarize_scoped_orders, InventoryError, MarketScope, Money, WorkspaceId};
use serde::Serialize;

use super::{fit_type, sde_error, FittingService};

/// A cost line with its type's name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitCostLine {
    #[serde(flatten)]
    pub line: CostLine,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitCostView {
    /// The market the prices come from.
    pub scope: MarketScope,
    pub lines: Vec<FitCostLine>,
    pub hull: Money,
    pub fittings: Money,
    pub drones: Money,
    pub cargo: Money,
    pub total: Money,
    /// Types the market has no sell order for; left out of the totals.
    pub missing: Vec<i64>,
    /// When the newest price used was observed.
    pub observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitAssetsView {
    /// Every holding of the fit's types (hull, items, stacks; not loaded
    /// charges), fitted or packaged alike: by type, the most held first.
    pub holdings: Vec<AssetHolding>,
}

impl FittingService {
    /// A saved fit priced at the lowest sell order in `scope`; `None` when
    /// the fit isn't in the workspace.
    pub async fn cost(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        scope: MarketScope,
    ) -> Result<Option<FitCostView>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let market = self.market.as_ref().ok_or_else(|| {
            InventoryError::Persistence("Market data is unavailable.".to_string())
        })?;
        let input = fit.input();
        let type_ids: Vec<i64> = fit_needs(&input).into_keys().collect();
        let books = market
            .scoped_order_books(workspace_id, scope, &type_ids)
            .await
            .map_err(sde_error)?;
        let mut prices = BTreeMap::new();
        let mut observed_at = None;
        for (type_id, orders) in &books {
            let summary = summarize_scoped_orders(orders);
            if let Some(best_sell) = summary.best_sell {
                prices.insert(*type_id, best_sell);
                observed_at = observed_at.max(summary.observed_at);
            }
        }
        let FitCost {
            lines,
            hull,
            fittings,
            drones,
            cargo,
            total,
            missing,
        } = fit_cost(&input, &prices);
        let references = self.references(&[(fit.ship_type_id, &fit.items)]).await?;
        Ok(Some(FitCostView {
            scope,
            lines: lines
                .into_iter()
                .map(|line| FitCostLine {
                    name: fit_type(&references, line.type_id).name,
                    line,
                })
                .collect(),
            hull,
            fittings,
            drones,
            cargo,
            total,
            missing,
            observed_at,
        }))
    }

    /// Where the workspace's characters already hold a saved fit's types;
    /// `None` when the fit isn't in the workspace.
    pub async fn assets(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<Option<FitAssetsView>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let holdings_source = self
            .holdings
            .as_ref()
            .ok_or_else(|| InventoryError::Persistence("Asset data is unavailable.".to_string()))?;
        let type_ids: Vec<i64> = fit_needs(&fit.input()).into_keys().collect();
        let mut holdings = holdings_source
            .type_holdings(workspace_id, &type_ids)
            .await?;
        holdings.sort_by(|a, b| {
            a.type_id
                .cmp(&b.type_id)
                .then_with(|| b.quantity.cmp(&a.quantity))
                .then_with(|| a.character_name.cmp(&b.character_name))
                .then_with(|| a.location_name.cmp(&b.location_name))
        });
        Ok(Some(FitAssetsView { holdings }))
    }
}
