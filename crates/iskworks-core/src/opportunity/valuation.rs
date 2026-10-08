//! Output valuation projection: sell-side vs immediate-liquidation
//! `OpportunityValuation`, the `OpportunityOutputMarketEvidence` book
//! summary, and the adjusted-price EIV basis.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::{MarketDepthResult, MarketOrderBook, Money};

use super::command::OpportunityError;
use super::evidence::{
    classify_evidence, OpportunityCompleteness, OpportunityEivBasis, OpportunityEvidenceStatus,
    OpportunityMissingMaterial,
};
use super::metrics::{checked_ratio_string, derive_opportunity_metrics, opportunity_completeness};

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityValuation {
    pub revenue: Option<Money>,
    pub gross_profit: Option<Money>,
    pub gross_margin_percent: Option<String>,
    pub gross_profit_per_manufacturing_hour: Option<String>,
    pub completeness: OpportunityCompleteness,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityValuations {
    pub sell_side: OpportunityValuation,
    pub immediate_liquidation: OpportunityValuation,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityOutputMarketEvidence {
    pub best_sell_unit_price: Option<Money>,
    pub best_sell_level_quantity: Option<u64>,
    pub total_visible_sell_quantity: u64,
    pub sell_order_count: u64,
    pub sell_consumed_fraction: Option<String>,
    pub best_buy_unit_price: Option<Money>,
    pub best_buy_level_quantity: Option<u64>,
    pub total_visible_buy_quantity: u64,
    pub buy_order_count: u64,
    pub buy_consumed_fraction: Option<String>,
    pub status: OpportunityEvidenceStatus,
    pub observation_batch_id: crate::MarketObservationBatchId,
    pub import_batch_id: Option<crate::MarketImportBatchId>,
    pub imported_file_id: Option<crate::ImportedMarketFileId>,
}

pub(super) fn project_output_valuation(
    material_cost: Option<Money>,
    installation_cost: Option<Money>,
    depth: Option<&MarketDepthResult>,
    output_quantity: u64,
    effective_duration_seconds: Option<u64>,
    product_count: usize,
) -> Result<OpportunityValuation, OpportunityError> {
    let revenue = depth
        .filter(|depth| depth.fully_covered)
        .map(|depth| depth.total);
    let metrics = derive_opportunity_metrics(
        material_cost,
        installation_cost,
        revenue,
        output_quantity,
        effective_duration_seconds,
    )?;
    let completeness =
        opportunity_completeness(material_cost, installation_cost, revenue, product_count);
    Ok(OpportunityValuation {
        revenue: metrics.estimated_output_value,
        gross_profit: metrics.estimated_gross_profit,
        gross_margin_percent: metrics.gross_margin_percent,
        gross_profit_per_manufacturing_hour: metrics.estimated_gross_profit_per_manufacturing_hour,
        completeness,
    })
}

pub(super) fn project_output_market_evidence(
    book: &MarketOrderBook,
    output_quantity: u64,
    calculated_at: DateTime<Utc>,
    market_freshness: chrono::Duration,
) -> OpportunityOutputMarketEvidence {
    let sell = crate::summarize_market_order_book_side(&book.orders, crate::MarketOrderSide::Sell);
    let buy = crate::summarize_market_order_book_side(&book.orders, crate::MarketOrderSide::Buy);
    let consumed_fraction = |visible: u64| -> Option<String> {
        if visible == 0 {
            None
        } else {
            checked_ratio_string(
                Decimal::from(output_quantity),
                Decimal::from(visible),
                Decimal::ONE,
            )
            .ok()
        }
    };
    OpportunityOutputMarketEvidence {
        best_sell_unit_price: sell.best_unit_price,
        best_sell_level_quantity: sell.best_level_quantity,
        total_visible_sell_quantity: sell.total_visible_quantity,
        sell_order_count: sell.order_count,
        sell_consumed_fraction: consumed_fraction(sell.total_visible_quantity),
        best_buy_unit_price: buy.best_unit_price,
        best_buy_level_quantity: buy.best_level_quantity,
        total_visible_buy_quantity: buy.total_visible_quantity,
        buy_order_count: buy.order_count,
        buy_consumed_fraction: consumed_fraction(buy.total_visible_quantity),
        // Effective freshness: a snapshot ESI has since confirmed unchanged
        // via a `304` is not stale just because the exact rows were fetched
        // hours ago. Physical provenance stays on `book.observed_at` /
        // `book.observation_batch_id`.
        status: classify_evidence(
            Some(book.effective_observed_at()),
            calculated_at,
            market_freshness,
        ),
        observation_batch_id: book.observation_batch_id,
        import_batch_id: book.import_batch_id,
        imported_file_id: book.imported_file_id,
    }
}

pub(super) fn project_eiv_basis(
    materials: &[crate::CapturedRecipeLine],
    adjusted_prices: &BTreeMap<i64, Decimal>,
) -> OpportunityEivBasis {
    let mut missing_materials = Vec::new();
    let mut observed_material_count = 0_u64;
    for material in materials {
        if adjusted_prices.contains_key(&material.type_id) {
            observed_material_count += 1;
        } else {
            missing_materials.push(OpportunityMissingMaterial {
                type_id: material.type_id,
                type_name: material.type_name.clone(),
            });
        }
    }
    OpportunityEivBasis {
        complete: missing_materials.is_empty(),
        required_material_count: materials.len() as u64,
        observed_material_count,
        missing_materials,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    use super::super::tests_common::*;
    use super::*;
    use crate::opportunity::*;

    #[test]
    fn dual_output_valuation_diverges_by_depth_aware_sell_and_liquidation_policy() {
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
        let book = two_sided_book(
            5_876,
            "Rifter",
            &[("140000000", 1), ("150000000", 11)],
            &[("80000000", 1), ("70000000", 11)],
            now,
        );

        let sell_depth = crate::calculate_market_depth(
            &book.orders,
            crate::MarketPricingPolicy::AcquireQuantityFromSellOrders,
            2,
        )
        .unwrap();
        let liquidation_depth = crate::calculate_market_depth(
            &book.orders,
            crate::MarketPricingPolicy::LiquidateQuantityIntoBuyOrders,
            2,
        )
        .unwrap();

        let sell_side = project_output_valuation(
            Some(money("10000000")),
            Some(money("1000000")),
            Some(&sell_depth),
            2,
            Some(3600),
            1,
        )
        .unwrap();
        let immediate_liquidation = project_output_valuation(
            Some(money("10000000")),
            Some(money("1000000")),
            Some(&liquidation_depth),
            2,
            Some(3600),
            1,
        )
        .unwrap();

        assert_eq!(sell_side.revenue, Some(money("290000000")));
        assert_eq!(immediate_liquidation.revenue, Some(money("150000000")));
        assert_ne!(sell_side.gross_profit, immediate_liquidation.gross_profit);
        assert_eq!(sell_side.completeness, OpportunityCompleteness::Complete);
        assert_eq!(
            immediate_liquidation.completeness,
            OpportunityCompleteness::Complete
        );

        let evidence = project_output_market_evidence(&book, 2, now, chrono::Duration::minutes(15));
        assert_eq!(evidence.best_sell_unit_price, Some(money("140000000")));
        assert_eq!(evidence.best_sell_level_quantity, Some(1));
        assert_eq!(evidence.total_visible_sell_quantity, 12);
        assert_eq!(evidence.sell_order_count, 2);
        assert_eq!(evidence.best_buy_unit_price, Some(money("80000000")));
        assert_eq!(evidence.best_buy_level_quantity, Some(1));
        assert_eq!(evidence.total_visible_buy_quantity, 12);
        assert_eq!(evidence.buy_order_count, 2);
        assert_eq!(evidence.status.state, OpportunityEvidenceState::Fresh);
    }

    #[test]
    fn missing_output_book_leaves_valuations_incomplete_without_evidence() {
        let sell_side = project_output_valuation(
            Some(money("10000000")),
            Some(money("1000000")),
            None,
            2,
            Some(3600),
            1,
        )
        .unwrap();

        assert_eq!(sell_side.revenue, None);
        assert_eq!(sell_side.completeness, OpportunityCompleteness::Incomplete);
    }

    #[test]
    fn eiv_basis_lists_only_materials_absent_from_the_adjusted_price_map() {
        let materials = vec![
            crate::CapturedRecipeLine {
                type_id: 34,
                type_name: "Tritanium".to_string(),
                quantity_per_run: 100,
                sort_order: 0,
            },
            crate::CapturedRecipeLine {
                type_id: 35,
                type_name: "Pyerite".to_string(),
                quantity_per_run: 50,
                sort_order: 1,
            },
        ];
        let mut adjusted_prices = BTreeMap::new();
        adjusted_prices.insert(35, Decimal::ZERO);

        let basis = project_eiv_basis(&materials, &adjusted_prices);

        assert!(!basis.complete);
        assert_eq!(basis.required_material_count, 2);
        assert_eq!(basis.observed_material_count, 1);
        assert_eq!(
            basis.missing_materials,
            vec![OpportunityMissingMaterial {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }]
        );
    }

    #[test]
    fn complete_economic_evidence_is_not_invalidated_by_non_blocking_warnings() {
        assert_eq!(
            opportunity_completeness(Some(money("100")), Some(money("10")), Some(money("150")), 1,),
            OpportunityCompleteness::Complete
        );
    }

    #[test]
    fn recently_revalidated_old_snapshot_classifies_as_fresh() {
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
        let mut book = two_sided_book(
            5_876,
            "Rifter",
            &[("140000000", 12)],
            &[("80000000", 12)],
            now - chrono::Duration::hours(2),
        );
        // ESI confirmed the same book 5m ago via a 304.
        book.revalidated_at = Some(now - chrono::Duration::minutes(5));

        let evidence = project_output_market_evidence(&book, 2, now, chrono::Duration::minutes(15));

        assert_eq!(evidence.status.state, OpportunityEvidenceState::Fresh);
        // Provenance is untouched: the status timestamp is the effective
        // freshness, but the physical book observation is still 2h old.
        assert_eq!(book.observed_at, now - chrono::Duration::hours(2));
    }

    #[test]
    fn old_snapshot_without_revalidation_stays_stale() {
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
        let book = two_sided_book(
            5_876,
            "Rifter",
            &[("140000000", 12)],
            &[("80000000", 12)],
            now - chrono::Duration::hours(2),
        );
        assert!(book.revalidated_at.is_none());

        let evidence = project_output_market_evidence(&book, 2, now, chrono::Duration::minutes(15));

        assert_eq!(evidence.status.state, OpportunityEvidenceState::Stale);
    }
}
