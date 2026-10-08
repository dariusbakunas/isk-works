//! Shared fixtures for the opportunity submodule tests.
#![cfg(test)]

use chrono::Utc;
use uuid::Uuid;

use iskworks_sde::{CandidateRecipeIdentity, ManufacturableCandidateRecipe};

use super::*;

pub(crate) fn jita_scope() -> crate::MarketScope {
    crate::MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    }
}

pub(crate) fn valid_command() -> EvaluateOpportunitiesCommand {
    EvaluateOpportunitiesCommand {
        scope_id: ProfitabilityScopeId::T1Frigates,
        facility_profile_id: crate::FacilityProfileId(Uuid::from_u128(1)),
        material_efficiency: Some(10),
        time_efficiency: Some(20),
        market_scope: jita_scope(),
    }
}

pub(crate) fn money(value: &str) -> crate::Money {
    crate::Money::parse(value).unwrap()
}

pub(crate) fn market_source(now: chrono::DateTime<Utc>) -> crate::MarketPriceSource {
    crate::MarketPriceSource {
        id: crate::PriceSourceId(Uuid::from_u128(2)),
        workspace_id: crate::WorkspaceId(Uuid::from_u128(3)),
        name: "Jita".to_string(),
        description: String::new(),
        kind: crate::PriceSourceKind::EsiMarketOrders,
        revision: 4,
        item_count: 0,
        config: crate::MarketPriceSourceConfig {
            price_source_id: crate::PriceSourceId(Uuid::from_u128(2)),
            workspace_id: crate::WorkspaceId(Uuid::from_u128(3)),
            location_id: 60_003_760,
            solar_system_id: 30_000_142,
            region_id: 10_000_002,
            location_alias: "Jita 4-4".to_string(),
            pricing_policy: crate::MarketPricingPolicy::LowestSell,
            coverage_policy: crate::MarketCoveragePolicy::AllowPartialWithWarning,
            observation_mode: crate::MarketObservationSetMode::LatestCompatibleImport,
            pinned_batch_id: None,
            fresh_after_hours: 6,
            stale_after_hours: 24,
            archived_at: None,
            last_snapshot_at: Some(now),
        },
        created_at: now,
        updated_at: now,
    }
}

pub(crate) fn sell_book(
    type_id: i64,
    type_name: &str,
    levels: &[(&str, u64)],
    now: chrono::DateTime<Utc>,
) -> crate::MarketOrderBook {
    let orders = levels
        .iter()
        .enumerate()
        .map(|(index, (price, volume))| crate::MarketOrderView {
            observation_id: None,
            import_batch_id: None,
            imported_file_id: None,
            order_id: i64::try_from(index + 1).unwrap(),
            type_id,
            type_name: type_name.to_string(),
            side: crate::MarketOrderSide::Sell,
            price: money(price),
            remaining_volume: *volume,
            entered_volume: *volume,
            minimum_volume: 1,
            order_range: -1,
            issued_at: now,
            duration_days: 90,
            observed_at: now,
            revalidated_at: None,
            location_id: 60_003_760,
            solar_system_id: 30_000_142,
            region_id: 10_000_002,
            jumps: 0,
        })
        .collect::<Vec<_>>();
    crate::MarketOrderBook {
        type_id,
        type_name: type_name.to_string(),
        location_id: 60_003_760,
        location_name: "Jita 4-4".to_string(),
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        observed_at: now,
        revalidated_at: None,
        observation_batch_id: crate::MarketObservationBatchId(Uuid::from_u128(
            u128::try_from(type_id).unwrap(),
        )),
        import_batch_id: None,
        imported_file_id: None,
        buy_order_count: 0,
        sell_order_count: u64::try_from(orders.len()).unwrap(),
        total_buy_volume: 0,
        total_sell_volume: orders.iter().map(|order| order.remaining_volume).sum(),
        lowest_sell: orders.first().map(|order| order.price),
        highest_buy: None,
        orders,
    }
}

pub(crate) fn two_sided_book(
    type_id: i64,
    type_name: &str,
    sell_levels: &[(&str, u64)],
    buy_levels: &[(&str, u64)],
    now: chrono::DateTime<Utc>,
) -> crate::MarketOrderBook {
    let side_orders = |side: crate::MarketOrderSide, levels: &[(&str, u64)], offset: usize| {
        levels
            .iter()
            .enumerate()
            .map(|(index, (price, volume))| crate::MarketOrderView {
                observation_id: None,
                import_batch_id: None,
                imported_file_id: None,
                order_id: i64::try_from(offset + index + 1).unwrap(),
                type_id,
                type_name: type_name.to_string(),
                side,
                price: money(price),
                remaining_volume: *volume,
                entered_volume: *volume,
                minimum_volume: 1,
                order_range: -1,
                issued_at: now,
                duration_days: 90,
                observed_at: now,
                revalidated_at: None,
                location_id: 60_003_760,
                solar_system_id: 30_000_142,
                region_id: 10_000_002,
                jumps: 0,
            })
            .collect::<Vec<_>>()
    };
    let mut orders = side_orders(crate::MarketOrderSide::Sell, sell_levels, 0);
    orders.extend(side_orders(
        crate::MarketOrderSide::Buy,
        buy_levels,
        sell_levels.len(),
    ));
    crate::MarketOrderBook {
        type_id,
        type_name: type_name.to_string(),
        location_id: 60_003_760,
        location_name: "Jita 4-4".to_string(),
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        observed_at: now,
        revalidated_at: None,
        observation_batch_id: crate::MarketObservationBatchId(Uuid::from_u128(
            u128::try_from(type_id).unwrap(),
        )),
        import_batch_id: None,
        imported_file_id: None,
        buy_order_count: u64::try_from(buy_levels.len()).unwrap(),
        sell_order_count: u64::try_from(sell_levels.len()).unwrap(),
        total_buy_volume: buy_levels.iter().map(|(_, volume)| volume).sum(),
        total_sell_volume: sell_levels.iter().map(|(_, volume)| volume).sum(),
        lowest_sell: sell_levels.first().map(|(price, _)| money(price)),
        highest_buy: buy_levels.first().map(|(price, _)| money(price)),
        orders,
    }
}

pub(crate) fn candidate_recipe(identity: CandidateRecipeIdentity) -> ManufacturableCandidateRecipe {
    ManufacturableCandidateRecipe {
        import_id: Uuid::from_u128(9),
        source_version: "2026.08".to_string(),
        identity,
        recipe_name: "Rifter Blueprint".to_string(),
        duration_seconds: Some(6_000),
        materials: vec![iskworks_sde::RecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity: 2_111,
        }],
        products: vec![iskworks_sde::RecipeLine {
            type_id: 5_876,
            type_name: "Rifter".to_string(),
            quantity: 1,
        }],
        primary_product_type_id: 5_876,
        primary_product_published: true,
        recipe_type_published: true,
        classification: iskworks_sde::CandidateProductClassification {
            category_id: Some(6),
            category_name: Some("Ship".to_string()),
            group_id: Some(25),
            group_name: Some("Frigate".to_string()),
            meta_group_id: Some(1),
            meta_group_name: Some("Tech I".to_string()),
            market_group_id: None,
            market_group_name: None,
            market_group_ancestry: Vec::new(),
        },
        has_additional_products: false,
    }
}

pub(crate) fn reaction_candidate_recipe(
    identity: CandidateRecipeIdentity,
) -> ManufacturableCandidateRecipe {
    // Real reaction-formula products carry no meta_group_id at all in
    // the SDE (verified against the local database) -- this fixture
    // matches that exactly rather than fabricating a tag that would
    // never occur, so the scope's own "unrestricted" filter is what's
    // actually under test, not an artificially convenient fixture.
    ManufacturableCandidateRecipe {
        import_id: Uuid::from_u128(9),
        source_version: "2026.08".to_string(),
        identity,
        recipe_name: "Fernite Alloy Reaction Formula".to_string(),
        duration_seconds: Some(3_600),
        materials: vec![iskworks_sde::RecipeLine {
            type_id: 16_633,
            type_name: "Titanium Chromide".to_string(),
            quantity: 10,
        }],
        products: vec![iskworks_sde::RecipeLine {
            type_id: 16_656,
            type_name: "Fernite Alloy".to_string(),
            // Kept small relative to the shared `output_book` fixture's
            // 100-unit visible depth so this candidate reads as a clean
            // "Strong" case in the end-to-end evaluation test below,
            // rather than tripping the (legitimate) thin-output-book
            // warning -- real reaction batches are larger in practice.
            quantity: 1,
        }],
        primary_product_type_id: 16_656,
        primary_product_published: true,
        recipe_type_published: true,
        classification: iskworks_sde::CandidateProductClassification {
            category_id: Some(4),
            category_name: Some("Material".to_string()),
            group_id: Some(429),
            group_name: Some("Composite".to_string()),
            meta_group_id: None,
            meta_group_name: None,
            market_group_id: None,
            market_group_name: None,
            market_group_ancestry: Vec::new(),
        },
        has_additional_products: false,
    }
}
