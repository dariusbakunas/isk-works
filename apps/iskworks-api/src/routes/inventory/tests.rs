use iskworks_core::{
    InventoryItemKey, MarketRefreshState, OwnerId, PriceSourceId, PriceSourceKind, WorkspaceId,
};

use super::*;

fn esi_source(name: &str) -> InventoryPricingSource {
    InventoryPricingSource::Explicit(PriceSource {
        id: PriceSourceId::new(),
        workspace_id: WorkspaceId::new(),
        name: name.to_string(),
        description: String::new(),
        kind: PriceSourceKind::EsiMarketOrders,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

fn coverage(refresh_state: MarketRefreshState) -> MarketCoverageItem {
    MarketCoverageItem {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        refresh_state,
        observed_at: None,
        last_attempted_at: None,
        next_refresh_at: None,
        last_error: None,
        order_count: 0,
        buy_order_count: 0,
        sell_order_count: 0,
        prior_etag: None,
        revalidated_at: None,
    }
}

/// Warnings for a Tritanium balance priced by `source`, with no usable
/// market preview and the given coverage.
fn warnings(source: &InventoryPricingSource, coverage: &MarketCoverageItem) -> Vec<String> {
    let balance = InventoryBalance::empty(
        InventoryItemKey {
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            type_id: 34,
        },
        "Tritanium".to_string(),
    );
    let response = inventory_item_response(
        balance,
        &[],
        0,
        InventoryEnrichment {
            source,
            market_preview: None,
            market_coverage: Some(coverage),
            metadata: None,
            observation: None,
        },
    );
    match response {
        Ok(response) => response.warnings,
        Err(_) => panic!("inventory_item_response failed"),
    }
}

#[test]
fn esi_market_warnings_name_the_price_source_not_jita() {
    let source = esi_source("Amarr VIII");

    assert!(warnings(&source, &coverage(MarketRefreshState::Refreshing)).contains(
        &"Amarr VIII market prices are being refreshed; current value will appear when observations are available."
            .to_string()
    ));
    assert!(
        warnings(&source, &coverage(MarketRefreshState::Failed)).contains(
            &"Amarr VIII market prices could not be refreshed; current value is unavailable."
                .to_string()
        )
    );
    assert!(
        warnings(&source, &coverage(MarketRefreshState::Current)).contains(
            &"No compatible Amarr VIII market orders are available for this item.".to_string()
        )
    );
}
