use super::*;

#[test]
fn public_region_is_the_region_for_stations_and_region_wide_scopes_only() {
    let region_wide = MarketScope {
        region_id: 10_000_002,
        location_id: None,
    };
    assert_eq!(region_wide.public_region(None), Some(10_000_002));
    assert_eq!(
        DEFAULT_MARKET_SCOPE.public_region(Some(MarketLocationClassification::NpcStation)),
        Some(10_000_002)
    );
    assert_eq!(
        DEFAULT_MARKET_SCOPE.public_region(Some(MarketLocationClassification::Structure {
            solar_system_id: 30_000_142
        })),
        None,
        "structure books are private"
    );
    assert_eq!(
        DEFAULT_MARKET_SCOPE.public_region(Some(MarketLocationClassification::Unknown)),
        None
    );
    assert_eq!(
        DEFAULT_MARKET_SCOPE.public_region(None),
        None,
        "an unclassified location is never assumed public"
    );
}

#[test]
fn market_scope_round_trips_through_json_with_and_without_a_location() {
    let region_wide = MarketScope {
        region_id: 10_000_002,
        location_id: None,
    };
    let json = serde_json::to_string(&region_wide).unwrap();
    assert_eq!(json, r#"{"regionId":10000002,"locationId":null}"#);
    assert_eq!(
        serde_json::from_str::<MarketScope>(&json).unwrap(),
        region_wide
    );

    let jita = MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    };
    let json = serde_json::to_string(&jita).unwrap();
    assert_eq!(json, r#"{"regionId":10000002,"locationId":60003760}"#);
    assert_eq!(serde_json::from_str::<MarketScope>(&json).unwrap(), jita);
}

#[test]
fn market_item_filter_rejects_zero_page_and_out_of_range_page_size() {
    let base = MarketItemFilter {
        scope: MarketScope {
            region_id: 10_000_002,
            location_id: None,
        },
        market_group_id: None,
        search: Some("  tritanium  ".to_string()),
        page: 1,
        page_size: 50,
    };

    assert!(matches!(
        MarketItemFilter {
            page: 0,
            ..base.clone()
        }
        .validate(),
        Err(MarketError::Validation(_))
    ));
    assert!(matches!(
        MarketItemFilter {
            page_size: 0,
            ..base.clone()
        }
        .validate(),
        Err(MarketError::Validation(_))
    ));
    assert!(matches!(
        MarketItemFilter {
            page_size: MAX_MARKET_ITEM_PAGE_SIZE + 1,
            ..base.clone()
        }
        .validate(),
        Err(MarketError::Validation(_))
    ));

    // Search is trimmed; an all-whitespace search becomes `None`, not
    // an empty-string filter that would (via `search=''`) match every
    // row in the SQL layer.
    let validated = base.clone().validate().unwrap();
    assert_eq!(validated.search.as_deref(), Some("tritanium"));
    let blank = MarketItemFilter {
        search: Some("   ".to_string()),
        ..base
    }
    .validate()
    .unwrap();
    assert_eq!(blank.search, None);
}

#[test]
fn build_market_category_tree_nests_multiple_levels_and_sorts_deterministically() {
    let flat = vec![
        iskworks_sde::MarketGroupNode {
            market_group_id: 4,
            name: "Ships".to_string(),
            parent_group_id: None,
            item_count: 0,
        },
        iskworks_sde::MarketGroupNode {
            market_group_id: 1361,
            name: "Frigates".to_string(),
            parent_group_id: Some(4),
            item_count: 2,
        },
        iskworks_sde::MarketGroupNode {
            market_group_id: 1362,
            name: "Destroyers".to_string(),
            parent_group_id: Some(4),
            item_count: 5,
        },
        iskworks_sde::MarketGroupNode {
            market_group_id: 2_000,
            name: "Rifter Variants".to_string(),
            parent_group_id: Some(1361),
            item_count: 3,
        },
        iskworks_sde::MarketGroupNode {
            market_group_id: 9,
            name: "Modules".to_string(),
            parent_group_id: None,
            item_count: 7,
        },
    ];

    let tree = build_market_category_tree(flat);

    // Two roots, sorted by name: Modules before Ships.
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].name, "Modules");
    assert_eq!(tree[0].market_group_id, 9);
    assert!(tree[0].children.is_empty());
    assert_eq!(tree[0].item_count, 7);

    let ships = &tree[1];
    assert_eq!(ships.name, "Ships");
    assert_eq!(ships.market_group_id, 4);
    // Two children, sorted by name: Destroyers before Frigates.
    assert_eq!(ships.children.len(), 2);
    assert_eq!(ships.children[0].name, "Destroyers");
    assert!(ships.children[0].children.is_empty());
    assert_eq!(ships.children[1].name, "Frigates");
    // Rolled up: own direct count (0) plus every descendant's --
    // Destroyers (5) + Frigates (2 direct + 3 from its own child) = 10.
    assert_eq!(ships.item_count, 10);

    // A grandchild is nested under its own parent, not flattened.
    let frigates = &ships.children[1];
    assert_eq!(frigates.children.len(), 1);
    assert_eq!(frigates.children[0].name, "Rifter Variants");
    assert_eq!(frigates.children[0].market_group_id, 2_000);
    assert_eq!(frigates.children[0].item_count, 3);
    assert_eq!(frigates.item_count, 5);
}

#[test]
fn build_market_category_tree_of_an_empty_list_is_empty() {
    assert!(build_market_category_tree(Vec::new()).is_empty());
}

#[test]
fn esi_market_order_validation_rejects_invalid_identity_and_location() {
    let valid = EsiMarketOrder::new(
        7_386_855_683,
        MarketOrderSide::Sell,
        "4.2500",
        100_000,
        250_000,
        1,
        "station".to_string(),
        Utc::now(),
        90,
        60_003_760,
        30_000_142,
        Some(60_003_760),
    )
    .unwrap();
    assert_eq!(valid.price, Money::parse("4.2500").unwrap());

    for (order_id, price, location_id) in [
        (0, "4.2500", 60_003_760),
        (1, "-1", 60_003_760),
        (1, "4.2500", 60_003_761),
    ] {
        assert!(EsiMarketOrder::new(
            order_id,
            MarketOrderSide::Sell,
            price,
            1,
            1,
            1,
            "station".to_string(),
            Utc::now(),
            90,
            location_id,
            30_000_142,
            Some(60_003_760),
        )
        .is_err());
    }

    // A region-wide fetch (`expected_location_id: None`) has no single
    // location to enforce -- any real location/order is valid.
    assert!(EsiMarketOrder::new(
        7_386_855_683,
        MarketOrderSide::Sell,
        "4.2500",
        100_000,
        250_000,
        1,
        "station".to_string(),
        Utc::now(),
        90,
        60_003_761,
        30_000_143,
        None,
    )
    .is_ok());
}

#[test]
fn esi_market_order_validation_rejects_non_positive_duration() {
    // Real ESI has been observed sending duration=0 for at least one
    // live order (a Thrasher sell order) -- the storage layer's
    // duration_days > 0 check constraint would otherwise reject the
    // whole INSERT, and since orders are persisted one item's worth per
    // transaction, that took down every other valid order for the same
    // item too. Reject it here instead, at construction, before it ever
    // reaches persistence.
    assert!(EsiMarketOrder::new(
        7_386_855_683,
        MarketOrderSide::Sell,
        "4.2500",
        100_000,
        250_000,
        1,
        "station".to_string(),
        Utc::now(),
        0,
        60_003_760,
        30_000_142,
        Some(60_003_760),
    )
    .is_err());
}

fn view(observed_at: DateTime<Utc>, revalidated_at: Option<DateTime<Utc>>) -> MarketOrderView {
    MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id: 1,
        type_id: 34,
        type_name: "Tritanium".to_string(),
        side: MarketOrderSide::Sell,
        price: crate::Money::parse("4.25").unwrap(),
        remaining_volume: 1,
        entered_volume: 1,
        minimum_volume: 1,
        order_range: -1,
        issued_at: observed_at,
        duration_days: 90,
        observed_at,
        revalidated_at,
        location_id: 60_003_760,
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        jumps: 0,
    }
}

#[test]
fn effective_observed_at_is_the_later_of_observed_and_revalidated() {
    let t1 = Utc::now() - chrono::Duration::hours(5);
    let t2 = t1 + chrono::Duration::hours(2);

    // No revalidation -> effective == observed (import orders, legacy rows).
    assert_eq!(view(t1, None).effective_observed_at(), t1);
    // 304 later -> effective == revalidation time.
    assert_eq!(view(t1, Some(t2)).effective_observed_at(), t2);
    // A malformed earlier revalidation can never make data look older.
    assert_eq!(view(t2, Some(t1)).effective_observed_at(), t2);

    let mut book = MarketOrderBook {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        location_id: 60_003_760,
        location_name: "Jita 4-4".to_string(),
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        observed_at: t1,
        revalidated_at: None,
        observation_batch_id: MarketObservationBatchId::new(),
        import_batch_id: None,
        imported_file_id: None,
        buy_order_count: 0,
        sell_order_count: 0,
        total_buy_volume: 0,
        total_sell_volume: 0,
        lowest_sell: None,
        highest_buy: None,
        orders: Vec::new(),
    };
    assert_eq!(book.effective_observed_at(), t1);
    book.revalidated_at = Some(t2);
    assert_eq!(book.effective_observed_at(), t2);
}

#[test]
fn an_esi_revalidation_does_not_make_an_unrelated_import_order_look_revalidated() {
    let observed = Utc::now() - chrono::Duration::hours(6);
    let esi = view(observed, Some(observed + chrono::Duration::hours(3)));
    let import = view(observed, None); // import orders always carry None

    assert_eq!(
        esi.effective_observed_at(),
        observed + chrono::Duration::hours(3)
    );
    assert_eq!(import.effective_observed_at(), observed);
    // The freshest *effective* time across a mixed scope is per-order,
    // so an old import is not dragged forward by the ESI 304.
    assert_eq!(
        [esi, import]
            .iter()
            .map(MarketOrderView::effective_observed_at)
            .max()
            .unwrap(),
        observed + chrono::Duration::hours(3)
    );
}
