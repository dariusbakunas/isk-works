use chrono::TimeZone;
use uuid::Uuid;

use super::*;

const OWNER: OwnerId = OwnerId(Uuid::from_u128(1));
const MINERAL: i64 = 11399; // Morphite
const MOON: i64 = 16650;
const COMPOSITE: i64 = 16683;

fn order(n: u128) -> OrderId {
    OrderId(Uuid::from_u128(1000 + n))
}

fn epic(n: u128, day: u32) -> RunwayEpic {
    RunwayEpic {
        order_id: order(n),
        owner_id: OWNER,
        display_name: format!("Epic {n}"),
        created_at: Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap(),
    }
}

fn isk(value: &str) -> Money {
    Money::parse(value).unwrap()
}

fn req(
    n: u128,
    type_id: i64,
    required: u64,
    reserved: u64,
    consumed: u64,
    unit: Option<&str>,
) -> RunwayRequirement {
    RunwayRequirement {
        order_id: order(n),
        type_id,
        required,
        reserved,
        consumed,
        unit_cost: unit.map(isk),
    }
}

fn pool(type_id: i64, on_hand: u64, active_reserved: u64) -> RunwayStockPool {
    RunwayStockPool {
        owner_id: OWNER,
        type_id,
        on_hand,
        active_reserved,
    }
}

fn types() -> Vec<RunwayTypeFact> {
    vec![
        RunwayTypeFact {
            type_id: MINERAL,
            name: "Morphite".into(),
            category: MaterialCategory::Minerals,
        },
        RunwayTypeFact {
            type_id: MOON,
            name: "Promethium".into(),
            category: MaterialCategory::MoonMaterials,
        },
        RunwayTypeFact {
            type_id: COMPOSITE,
            name: "Photonic Metamaterials".into(),
            category: MaterialCategory::ReactionIntermediates,
        },
    ]
}

fn run(facts: RunwayFacts) -> InventoryRunway {
    build_runway(&facts, &RunwayFilter::default())
}

fn material(runway: &InventoryRunway, type_id: i64) -> &RunwayMaterial {
    runway
        .materials
        .iter()
        .find(|m| m.type_id == type_id)
        .expect("material row")
}

#[test]
fn fully_consumed_requirement_is_not_demand() {
    // Stage: Photonic Metamaterials 4,597 required and all of it consumed,
    // 512 left on hand. Summing `required` would report it short.
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![req(1, COMPOSITE, 4_597, 0, 4_597, Some("6000"))],
        pools: vec![pool(COMPOSITE, 512, 0)],
        types: types(),
        ..RunwayFacts::default()
    });
    assert!(runway.materials.is_empty());
    assert_eq!(runway.kpis.shortfall_value, Money::zero());
    assert!(!runway.epics[0].blocked);
}

#[test]
fn partly_consumed_requirement_only_needs_the_rest() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![req(1, COMPOSITE, 100, 0, 60, Some("10"))],
        pools: vec![pool(COMPOSITE, 40, 0)],
        types: types(),
        ..RunwayFacts::default()
    });
    let row = material(&runway, COMPOSITE);
    assert_eq!((row.needed, row.on_hand, row.short), (40, 40, 0));
    assert_eq!(row.coverage_percent, 100.0);
    assert_eq!(row.epics[0].consumed, 60);
    assert_eq!(row.epics[0].outstanding, 40);
}

#[test]
fn free_stock_goes_to_the_oldest_epic_first() {
    let runway = run(RunwayFacts {
        // Listed newest first on purpose: order comes from created_at.
        epics: vec![epic(2, 5), epic(1, 1)],
        requirements: vec![
            req(1, MINERAL, 100, 0, 0, Some("10")),
            req(2, MINERAL, 100, 0, 0, Some("10")),
        ],
        pools: vec![pool(MINERAL, 150, 0)],
        types: types(),
        ..RunwayFacts::default()
    });
    let row = material(&runway, MINERAL);
    assert_eq!((row.needed, row.short), (200, 50));
    assert_eq!(row.coverage_percent, 75.0);
    // Largest shortfall first: the newer Epic.
    assert_eq!(row.epics[0].order_id, order(2));
    assert_eq!(row.epics[0].short, 50);
    assert_eq!(row.epics[1].short, 0);

    assert_eq!(runway.epics[0].order_id, order(1));
    assert!(!runway.epics[0].blocked);
    assert_eq!(runway.epics[0].covered_percent, Some(100.0));
    assert!(runway.epics[1].blocked);
    assert_eq!(runway.epics[1].covered_percent, Some(50.0));
    assert_eq!(runway.epics[1].short_value, isk("500"));
    assert_eq!(runway.kpis.epics_blocked, 1);
    assert_eq!(runway.kpis.epics_total, 2);
    assert_eq!(runway.kpis.shortfall_value, isk("500"));
    assert_eq!(runway.kpis.demand_covered_percent, Some(75.0));
}

#[test]
fn reserved_stock_is_not_subtracted_twice() {
    // 60 held for this Epic, and those 60 are the only stock. The Epic still
    // needs 40 more; the prototype's `needed - (onHand - reserved)` said 100.
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![req(1, MOON, 100, 60, 0, Some("1"))],
        pools: vec![pool(MOON, 60, 60)],
        types: types(),
        ..RunwayFacts::default()
    });
    let row = material(&runway, MOON);
    assert_eq!(
        (row.needed, row.on_hand, row.reserved, row.short),
        (100, 60, 60, 40)
    );
}

#[test]
fn stock_reserved_by_another_epic_is_not_free() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1), epic(2, 2)],
        requirements: vec![
            req(1, MOON, 50, 50, 0, Some("1")),
            req(2, MOON, 30, 0, 0, Some("1")),
        ],
        pools: vec![pool(MOON, 60, 50)],
        types: types(),
        ..RunwayFacts::default()
    });
    let row = material(&runway, MOON);
    assert_eq!((row.needed, row.reserved, row.short), (80, 50, 20));
}

#[test]
fn ticket_consumption_covers_epics_recorded_before_consume_on_record() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![req(1, MINERAL, 100, 0, 0, Some("1"))],
        event_consumption: vec![RunwayEventConsumption {
            order_id: order(1),
            type_id: MINERAL,
            quantity: 70,
        }],
        pools: vec![],
        types: types(),
    });
    let row = material(&runway, MINERAL);
    assert_eq!((row.needed, row.short), (30, 30));
    assert_eq!(row.epics[0].consumed, 70);
}

#[test]
fn split_requirements_of_one_type_sum_with_a_weighted_unit_cost() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, MINERAL, 100, 0, 0, Some("10")),
            req(1, MINERAL, 300, 0, 0, Some("20")),
        ],
        types: types(),
        ..RunwayFacts::default()
    });
    let row = material(&runway, MINERAL);
    assert_eq!(row.short, 400);
    assert_eq!(row.unit_cost, Some(isk("17.5")));
    assert_eq!(row.short_value, Some(isk("7000")));
}

#[test]
fn unpriced_shortfall_is_counted_and_left_out_of_value() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, MINERAL, 10, 0, 0, None),
            req(1, MOON, 10, 0, 0, Some("5")),
        ],
        types: types(),
        ..RunwayFacts::default()
    });
    assert_eq!(runway.missing_price_count, 1);
    assert_eq!(material(&runway, MINERAL).short_value, None);
    assert_eq!(runway.kpis.shortfall_value, isk("50"));
    assert_eq!(runway.kpis.materials_short, 2);
}

#[test]
fn category_filter_scopes_rows_epics_and_kpis() {
    let facts = RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, MINERAL, 10, 0, 0, Some("100")),
            req(1, MOON, 10, 0, 0, Some("1")),
        ],
        types: types(),
        ..RunwayFacts::default()
    };
    let runway = build_runway(
        &facts,
        &RunwayFilter {
            category: Some(MaterialCategory::MoonMaterials),
        },
    );
    assert_eq!(runway.materials.len(), 1);
    assert_eq!(runway.materials[0].type_id, MOON);
    assert_eq!(runway.kpis.materials_total, 1);
    assert_eq!(runway.kpis.shortfall_value, isk("10"));
    assert_eq!(runway.epics[0].short_value, isk("10"));
}

#[test]
fn epic_readiness_is_by_value_not_quantity() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, MINERAL, 10, 0, 0, Some("100")),
            req(1, MOON, 10, 0, 0, Some("900")),
        ],
        pools: vec![pool(MINERAL, 10, 0)],
        types: types(),
        ..RunwayFacts::default()
    });
    assert_eq!(runway.epics[0].covered_percent, Some(10.0));
    assert_eq!(runway.epics[0].short_value, isk("9000"));
}

#[test]
fn rows_sort_by_category_then_short_value() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, COMPOSITE, 1, 0, 0, Some("1")),
            req(1, MINERAL, 1, 0, 0, Some("1")),
            req(1, MOON, 1, 0, 0, Some("1")),
        ],
        types: types(),
        ..RunwayFacts::default()
    });
    let order: Vec<i64> = runway.materials.iter().map(|m| m.type_id).collect();
    assert_eq!(order, vec![MOON, MINERAL, COMPOSITE]);
}

#[test]
fn requirements_of_unknown_epics_and_unknown_types_are_handled() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![
            req(1, 999, 5, 0, 0, Some("1")),
            req(9, MINERAL, 5, 0, 0, Some("1")),
        ],
        types: types(),
        ..RunwayFacts::default()
    });
    assert_eq!(runway.materials.len(), 1);
    assert_eq!(runway.materials[0].name, "Type 999");
    assert_eq!(runway.materials[0].category, MaterialCategory::Other);
}

#[test]
fn no_epics_is_an_empty_report() {
    let runway = run(RunwayFacts::default());
    assert!(runway.materials.is_empty() && runway.epics.is_empty());
    assert_eq!(runway.kpis.demand_covered_percent, None);
    assert_eq!(runway.kpis.shortfall_value, Money::zero());
}

#[test]
fn serializes_camel_case_with_category_labels() {
    let runway = run(RunwayFacts {
        epics: vec![epic(1, 1)],
        requirements: vec![req(1, MINERAL, 10, 0, 0, Some("1"))],
        types: types(),
        ..RunwayFacts::default()
    });
    let json = serde_json::to_value(&runway).unwrap();
    assert_eq!(json["materials"][0]["category"], "Minerals");
    assert_eq!(json["materials"][0]["shortValue"], "10.0000");
    assert_eq!(json["kpis"]["materialsShort"], 1);
    assert_eq!(json["missingPriceCount"], 0);
}
