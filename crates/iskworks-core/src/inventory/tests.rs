use super::*;

fn key() -> InventoryItemKey {
    InventoryItemKey {
        workspace_id: WorkspaceId(Uuid::new_v4()),
        owner_id: OwnerId(Uuid::new_v4()),
        type_id: 34,
    }
}

fn command(quantity: u64, cost: Option<&str>, quality: CostInputQuality) -> PostInventoryCommand {
    PostInventoryCommand {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        quantity,
        unit_cost: cost.map(str::to_string),
        cost_quality: quality,
        source_reference: String::new(),
        note: String::new(),
        effective_at: Utc::now(),
        expected_revision: 0,
        acknowledge_zero_cost: quality == CostInputQuality::ZeroCost,
    }
}

fn posting(
    balance: &InventoryBalance,
    quantity: u64,
    cost: Option<&str>,
    quality: CostInputQuality,
) -> InventoryPosting {
    let mut command = command(quantity, cost, quality);
    command.expected_revision = balance.revision;
    create_posting(
        balance.key.workspace_id,
        balance.key.owner_id,
        if balance.revision == 0 {
            InventoryEventKind::OpeningBalance
        } else {
            InventoryEventKind::Purchase
        },
        command,
    )
    .unwrap()
}

#[test]
fn exact_weighted_average_uses_total_cost_as_authority() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let first = apply_inventory_event(
        &empty,
        &posting(&empty, 100, Some("10.0000"), CostInputQuality::Known),
    )
    .unwrap();
    let second_posting = posting(&first, 50, Some("16.0000"), CostInputQuality::Known);
    let second = apply_inventory_event(&first, &second_posting).unwrap();
    assert_eq!(second.quantity, 150);
    assert_eq!(
        second.total_historical_cost,
        Money::parse("1800.0000").unwrap()
    );
    assert_eq!(
        second.average_unit_cost,
        Some(Money::parse("12.0000").unwrap())
    );
}

#[test]
fn explicit_zero_cost_is_a_resolvable_zero_average() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let zero = apply_inventory_event(
        &empty,
        &posting(&empty, 10, None, CostInputQuality::ZeroCost),
    )
    .unwrap();
    assert_eq!(zero.quantity, 10);
    assert_eq!(zero.average_unit_cost, Some(Money::zero()));
}

/// Regression: negating an exactly-zero cost naively produces a
/// `Decimal` "-0.0000", which `apply_inventory_event` correctly
/// rejects as an inconsistent projection -- both `consumption_posting`
/// and `adjustment_posting`'s negative direction must guard against
/// this when fully or partially draining a zero-cost balance.
#[test]
fn consuming_a_zero_cost_balance_entirely_does_not_produce_a_negative_zero() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let zero_cost = apply_inventory_event(
        &empty,
        &posting(&empty, 10, None, CostInputQuality::ZeroCost),
    )
    .unwrap();
    let consumption = consumption_posting(
        &zero_cost,
        10,
        "Tritanium".to_string(),
        "build:test".to_string(),
        String::new(),
        CostInputQuality::Known,
        Utc::now(),
    )
    .unwrap();
    let result = apply_inventory_event(&zero_cost, &consumption).unwrap();
    assert_eq!(result.quantity, 0);
    assert_eq!(result.total_historical_cost, Money::zero());
}

#[test]
fn negative_adjustment_on_a_zero_cost_balance_does_not_produce_a_negative_zero() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let zero_cost = apply_inventory_event(
        &empty,
        &posting(&empty, 10, None, CostInputQuality::ZeroCost),
    )
    .unwrap();
    let remove = adjustment_posting(
        &zero_cost,
        -10,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    let result = apply_inventory_event(&zero_cost, &remove).unwrap();
    assert_eq!(result.quantity, 0);
    assert_eq!(result.total_historical_cost, Money::zero());
}

/// One cost model for anything a user enters (posted as `Known`, no
/// warning). `Estimated` only comes from the system -- e.g. a ticket
/// recorded without an actual cost -- so its warning says that, rather
/// than talking about an "estimated opening value".
#[test]
fn only_system_estimated_costs_warn() {
    assert!(warnings_for(CostInputQuality::Known).is_empty());
    let estimated = warnings_for(CostInputQuality::Estimated);
    assert_eq!(estimated.len(), 1);
    assert!(estimated[0].contains("estimated cost"), "{estimated:?}");
    assert!(!estimated[0].contains("opening"), "{estimated:?}");
}

#[test]
fn exact_reversal_restores_previous_balance() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let inbound = posting(&empty, 10, Some("4.2500"), CostInputQuality::Estimated);
    let current = apply_inventory_event(&empty, &inbound).unwrap();
    let event = InventoryEvent {
        id: inbound.id,
        key: inbound.key.clone(),
        type_name: inbound.type_name.clone(),
        kind: inbound.kind,
        quantity_delta: inbound.quantity_delta,
        total_cost_delta: inbound.total_cost_delta,
        unit_cost: inbound.unit_cost,
        cost_quality: inbound.cost_quality,
        source_reference: String::new(),
        note: String::new(),
        effective_at: inbound.effective_at,
        recorded_at: inbound.recorded_at,
        sequence: 1,
        reverses_event_id: None,
        reversed_by_event_id: None,
        resulting_balance: current.clone(),
    };
    let reversal = reversal_posting(&event, 1, "Incorrect quantity".to_string()).unwrap();
    let restored = apply_inventory_event(&current, &reversal).unwrap();
    assert_eq!(restored.quantity, 0);
    assert_eq!(restored.total_historical_cost, Money::zero());
}

#[test]
fn prevents_negative_and_overflowing_balances() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let mut invalid = posting(&empty, 1, Some("1"), CostInputQuality::Known);
    invalid.quantity_delta = -1;
    assert!(matches!(
        apply_inventory_event(&empty, &invalid),
        Err(InventoryError::NegativeBalance)
    ));
}

#[test]
fn rebuild_is_deterministic() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let a = posting(&empty, 100, Some("10"), CostInputQuality::Known);
    let one = apply_inventory_event(&empty, &a).unwrap();
    let b = posting(&one, 50, Some("16"), CostInputQuality::Known);
    let rebuilt =
        rebuild_inventory_balance(empty.key.clone(), empty.type_name.clone(), &[a, b]).unwrap();
    assert_eq!(rebuilt.quantity, 150);
    assert_eq!(rebuilt.average_unit_cost, Some(Money::parse("12").unwrap()));
}

#[test]
fn consumption_uses_the_current_weighted_average() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let first = apply_inventory_event(
        &empty,
        &posting(&empty, 100, Some("10.0000"), CostInputQuality::Known),
    )
    .unwrap();
    let mixed = apply_inventory_event(
        &first,
        &posting(&first, 50, Some("16.0000"), CostInputQuality::Known),
    )
    .unwrap();
    // 150 @ average 12.00 (total 1800).
    let consumption = consumption_posting(
        &mixed,
        50,
        "Tritanium".to_string(),
        "build:test".to_string(),
        String::new(),
        CostInputQuality::Known,
        Utc::now(),
    )
    .unwrap();
    let result = apply_inventory_event(&mixed, &consumption).unwrap();
    assert_eq!(consumption.kind, InventoryEventKind::Consumption);
    assert_eq!(consumption.total_cost_delta.0.to_string(), "-600.0000");
    assert_eq!(result.quantity, 100);
    assert_eq!(
        result.total_historical_cost,
        Money::parse("1200.0000").unwrap()
    );
    assert_eq!(
        result.average_unit_cost,
        Some(Money::parse("12.0000").unwrap())
    );
}

#[test]
fn consuming_the_entire_balance_zeroes_the_cost_pool_exactly() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &posting(&empty, 3, Some("0.3333"), CostInputQuality::Known),
    )
    .unwrap();
    let consumption = consumption_posting(
        &current,
        3,
        "Tritanium".to_string(),
        "build:test".to_string(),
        String::new(),
        CostInputQuality::Known,
        Utc::now(),
    )
    .unwrap();
    let result = apply_inventory_event(&current, &consumption).unwrap();
    assert_eq!(result.quantity, 0);
    assert_eq!(result.total_historical_cost, Money::zero());
}

#[test]
fn build_consumption_cannot_use_generic_reversal() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &posting(&empty, 10, Some("4.0000"), CostInputQuality::Known),
    )
    .unwrap();
    let posting = consumption_posting(
        &current,
        5,
        "Tritanium".to_string(),
        "build:test".to_string(),
        String::new(),
        CostInputQuality::Known,
        Utc::now(),
    )
    .unwrap();
    let resulting = apply_inventory_event(&current, &posting).unwrap();
    let event = InventoryEvent {
        id: posting.id,
        key: posting.key,
        type_name: posting.type_name,
        kind: posting.kind,
        quantity_delta: posting.quantity_delta,
        total_cost_delta: posting.total_cost_delta,
        unit_cost: posting.unit_cost,
        cost_quality: posting.cost_quality,
        source_reference: posting.source_reference,
        note: posting.note,
        effective_at: posting.effective_at,
        recorded_at: posting.recorded_at,
        sequence: 2,
        reverses_event_id: None,
        reversed_by_event_id: None,
        resulting_balance: resulting,
    };
    assert!(matches!(
        reversal_posting(&event, 2, "undo".to_string()),
        Err(InventoryError::BuildConsumptionNotReversible)
    ));
}

#[test]
fn positive_adjustment_without_unit_cost_defaults_to_existing_weighted_average() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            10_000,
            "Tritanium".to_string(),
            Some(Money::parse("4.1200").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();

    let adjustment = adjustment_posting(
        &current,
        2_000,
        "Tritanium".to_string(),
        None,
        "exploration loot".to_string(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(adjustment.cost_quality, CostInputQuality::Known);
    assert_eq!(adjustment.unit_cost, Some(Money::parse("4.1200").unwrap()));
    let result = apply_inventory_event(&current, &adjustment).unwrap();
    assert_eq!(result.quantity, 12_000);
    assert_eq!(
        result.average_unit_cost,
        Some(Money::parse("4.1200").unwrap())
    );
}

#[test]
fn positive_adjustment_without_unit_cost_on_a_new_item_is_rejected() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let result = adjustment_posting(
        &empty,
        500,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    );
    assert!(matches!(result, Err(InventoryError::Validation(_))));
}

#[test]
fn positive_known_adjustment_increases_quantity_and_cost() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let adjustment = adjustment_posting(
        &empty,
        50,
        "Tritanium".to_string(),
        Some(Money::parse("4.0000").unwrap()),
        "manual: corp transfer".to_string(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(adjustment.cost_quality, CostInputQuality::Known);
    let result = apply_inventory_event(&empty, &adjustment).unwrap();
    assert_eq!(result.quantity, 50);
    assert_eq!(
        result.total_historical_cost,
        Money::parse("200.0000").unwrap()
    );
    assert_eq!(
        result.average_unit_cost,
        Some(Money::parse("4.0000").unwrap())
    );
}

#[test]
fn negative_adjustment_draws_at_the_current_weighted_average() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            120,
            "Tritanium".to_string(),
            Some(Money::parse("10.0000").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();

    let remove = adjustment_posting(
        &current,
        -50,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(remove.cost_quality, CostInputQuality::Known);
    assert_eq!(remove.unit_cost, Some(Money::parse("10.0000").unwrap()));
    let result = apply_inventory_event(&current, &remove).unwrap();
    assert_eq!(result.quantity, 70);
    assert_eq!(
        result.total_historical_cost,
        Money::parse("700.0000").unwrap()
    );
}

#[test]
fn negative_adjustment_consuming_the_entire_balance_zeroes_the_cost_pool_exactly() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            3,
            "Tritanium".to_string(),
            Some(Money::parse("0.3333").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    let remove = adjustment_posting(
        &current,
        -3,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    let result = apply_inventory_event(&current, &remove).unwrap();
    assert_eq!(result.quantity, 0);
    // Rounding the average (0.3333 / 3 truncated) and multiplying back
    // out would leave a fractional residue -- the exact-total shortcut
    // in `priced_reduction` takes the real total instead.
    assert_eq!(result.total_historical_cost, Money::zero());
}

#[test]
fn negative_adjustment_larger_than_physical_inventory_is_rejected() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            10,
            "Tritanium".to_string(),
            Some(Money::parse("1.0000").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = adjustment_posting(
        &current,
        -11,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    );
    assert!(matches!(result, Err(InventoryError::NegativeBalance)));
}

#[test]
fn negative_adjustment_cannot_assert_a_unit_cost() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let current = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            10,
            "Tritanium".to_string(),
            Some(Money::parse("1.0000").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = adjustment_posting(
        &current,
        -5,
        "Tritanium".to_string(),
        Some(Money::parse("1.0000").unwrap()),
        String::new(),
        String::new(),
        Utc::now(),
    );
    assert!(matches!(result, Err(InventoryError::Validation(_))));
}

#[test]
fn zero_quantity_adjustment_is_rejected() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let result = adjustment_posting(
        &empty,
        0,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    );
    assert!(matches!(result, Err(InventoryError::Validation(_))));
}

#[test]
fn known_adjustment_blends_into_existing_weighted_average() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let first = apply_inventory_event(
        &empty,
        &adjustment_posting(
            &empty,
            100,
            "Tritanium".to_string(),
            Some(Money::parse("10.0000").unwrap()),
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    let second_adjustment = adjustment_posting(
        &first,
        50,
        "Tritanium".to_string(),
        Some(Money::parse("16.0000").unwrap()),
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    let second = apply_inventory_event(&first, &second_adjustment).unwrap();
    assert_eq!(second.quantity, 150);
    assert_eq!(
        second.total_historical_cost,
        Money::parse("1800.0000").unwrap()
    );
    assert_eq!(
        second.average_unit_cost,
        Some(Money::parse("12.0000").unwrap())
    );
}

#[test]
fn rebuild_replays_an_adjustment_sequence_deterministically() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    let a = adjustment_posting(
        &empty,
        100,
        "Tritanium".to_string(),
        Some(Money::parse("10.0000").unwrap()),
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    let one = apply_inventory_event(&empty, &a).unwrap();
    let b = adjustment_posting(
        &one,
        -20,
        "Tritanium".to_string(),
        None,
        String::new(),
        String::new(),
        Utc::now(),
    )
    .unwrap();
    let direct = apply_inventory_event(&one, &b).unwrap();

    let rebuilt =
        rebuild_inventory_balance(empty.key.clone(), empty.type_name.clone(), &[a, b]).unwrap();
    assert_eq!(rebuilt, direct);
}

#[test]
fn existing_event_kinds_are_unaffected_by_the_new_adjustment_kind() {
    assert_ne!(
        InventoryEventKind::Adjustment,
        InventoryEventKind::OpeningBalance
    );
    assert_ne!(InventoryEventKind::Adjustment, InventoryEventKind::Purchase);
    assert_ne!(
        InventoryEventKind::Adjustment,
        InventoryEventKind::Consumption
    );
    assert_ne!(
        InventoryEventKind::Adjustment,
        InventoryEventKind::ProductionOutput
    );
    assert_ne!(InventoryEventKind::Adjustment, InventoryEventKind::Reversal);
}

/// Explicit statement of the invariant: chains every positive-then-
/// negative event kind in sequence and asserts `average_unit_cost` is
/// resolvable at every step where `quantity > 0`, and only `None` when
/// the balance is fully empty.
#[test]
fn average_unit_cost_is_always_resolvable_whenever_quantity_is_positive() {
    let empty = InventoryBalance::empty(key(), "Tritanium".to_string());
    assert_eq!(empty.average_unit_cost, None);

    let opened = apply_inventory_event(
        &empty,
        &posting(&empty, 100, Some("10.0000"), CostInputQuality::Known),
    )
    .unwrap();
    assert!(opened.average_unit_cost.is_some());

    let purchased = apply_inventory_event(
        &opened,
        &posting(&opened, 50, Some("16.0000"), CostInputQuality::Known),
    )
    .unwrap();
    assert!(purchased.average_unit_cost.is_some());

    let adjusted_up = apply_inventory_event(
        &purchased,
        &adjustment_posting(
            &purchased,
            2_000,
            "Tritanium".to_string(),
            None, // defaults to the current average
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(adjusted_up.average_unit_cost.is_some());

    let consumed = apply_inventory_event(
        &adjusted_up,
        &consumption_posting(
            &adjusted_up,
            500,
            "Tritanium".to_string(),
            "build:test".to_string(),
            String::new(),
            CostInputQuality::Known,
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(consumed.average_unit_cost.is_some());

    let adjusted_down = apply_inventory_event(
        &consumed,
        &adjustment_posting(
            &consumed,
            -(consumed.quantity as i64),
            "Tritanium".to_string(),
            None,
            String::new(),
            String::new(),
            Utc::now(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(adjusted_down.quantity, 0);
    assert_eq!(adjusted_down.average_unit_cost, None);
}
