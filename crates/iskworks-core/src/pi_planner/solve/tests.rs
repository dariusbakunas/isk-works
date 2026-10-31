use std::collections::BTreeMap;

use super::super::effort::EffortSettings;
use super::super::fixtures::*;
use super::super::rank::{rank, PlannerCharacter};
use super::super::yields::{DensityEntry, SecurityBand};
use super::*;

const LAVA_I: i64 = 40_000_001;
const BARREN_II: i64 = 40_000_002;
const BARREN_IV: i64 = 40_000_004;
const PLASMA_IX: i64 = 40_000_009;

fn scanned(pct: f64) -> DensityEntry {
    DensityEntry {
        density_pct: Some(pct),
        head_value: None,
    }
}

fn planet(planet_id: i64, type_id: i64, densities: &[(i64, f64)]) -> CandidatePlanet {
    CandidatePlanet {
        planet_id,
        type_id,
        radius_m: Some(5_000_000),
        jumps: 0,
        band: SecurityBand::Null,
        customs_rate: None,
        contested: false,
        densities: densities
            .iter()
            .map(|(resource, pct)| (*resource, scanned(*pct)))
            .collect(),
    }
}

/// Q-3HS5's Lava, Barren and Plasma planets with the densities read in game.
fn q3hs5() -> Vec<CandidatePlanet> {
    vec![
        planet(
            LAVA_I,
            LAVA,
            &[
                (BASE_METALS, 40.0),
                (HEAVY_METALS, 50.0),
                (NON_CS_CRYSTALS, 44.0),
            ],
        ),
        planet(
            BARREN_II,
            BARREN,
            &[
                (BASE_METALS, 58.0),
                (NOBLE_METALS, 47.0),
                (AQUEOUS_LIQUIDS, 50.0),
            ],
        ),
        planet(
            BARREN_IV,
            BARREN,
            &[(BASE_METALS, 62.0), (NOBLE_METALS, 60.0)],
        ),
        planet(
            PLASMA_IX,
            PLASMA,
            &[
                (BASE_METALS, 35.0),
                (HEAVY_METALS, 61.0),
                (NOBLE_METALS, 28.0),
                (NON_CS_CRYSTALS, 57.0),
            ],
        ),
    ]
}

fn character(interplanetary_consolidation: u8) -> PlannerCharacter {
    PlannerCharacter {
        interplanetary_consolidation,
        command_center_upgrades: 4,
    }
}

fn scenario(characters: Vec<PlannerCharacter>) -> RankScenario {
    RankScenario {
        characters,
        effort: EffortSettings {
            reset_days: 3,
            haul_days: 3,
            uplift_at_login: true,
        },
        default_customs_rate: 0.1,
        buy_intermediates: false,
        max_colonies: None,
    }
}

fn prices(best_buy: &[(i64, f64)]) -> Prices {
    Prices {
        best_buy: best_buy.iter().copied().collect(),
        best_sell: best_buy
            .iter()
            .map(|(id, price)| (*id, price * 1.1))
            .collect(),
    }
}

fn market() -> Prices {
    prices(&[
        (WATER, 600.0),
        (REACTIVE_METALS, 500.0),
        (PRECIOUS_METALS, 550.0),
        (TOXIC_METALS, 450.0),
        (CHIRAL_STRUCTURES, 520.0),
        (MECHANICAL_PARTS, 9_000.0),
        (CONSUMER_ELECTRONICS, 9_500.0),
        (ROBOTICS, 90_000.0),
    ])
}

fn plan(scenario: &RankScenario, planets: &[CandidatePlanet], prices: &Prices) -> PiPlan {
    solve(&catalogue(), scenario, planets, prices).expect("solves")
}

fn sold(plan: &PiPlan, type_id: i64) -> f64 {
    plan.flows
        .iter()
        .filter(|flow| flow.to == FlowEnd::Market && flow.type_id == type_id)
        .map(|flow| flow.units_per_hour)
        .sum()
}

#[test]
fn one_colony_matches_the_best_reference_chain() {
    // The ranking stacks every chain on the best planet; with one colony
    // both models build the same single chain.
    let scenario = scenario(vec![character(0)]);
    let result = plan(&scenario, &q3hs5(), &market());
    let ranking = rank(&catalogue(), &scenario, &q3hs5(), &market());
    let best = ranking.ranked.first().expect("something ranks");

    assert!(
        result.isk_per_month >= best.isk_per_month * 0.97,
        "plan {} vs ranked {} {}",
        result.isk_per_month,
        best.type_id,
        best.isk_per_month
    );
    assert_eq!(result.colonies_used(), 1);
    assert!(result.customs_tax_per_month > 0.0);
    // Every colony fits its command center.
    for colony in &result.colonies {
        assert!(colony.cpu_used <= 1.0 + 1e-6, "{colony:?}");
        assert!(colony.power_used <= 1.0 + 1e-6, "{colony:?}");
    }
}

#[test]
fn robotics_needs_a_factory_and_every_p1_flows_to_it() {
    // Only Robotics pays, so the plan must build it.
    let only_robotics = prices(&[(ROBOTICS, 200_000.0)]);
    let result = plan(&scenario(vec![character(5)]), &q3hs5(), &only_robotics);

    assert!(sold(&result, ROBOTICS) > 0.0, "{result:#?}");
    let factories = result
        .colonies
        .iter()
        .filter(|colony| colony.role == ColonyRole::Factory)
        .count();
    assert!(factories >= 1);
    // Nothing is sold but Robotics.
    assert!(result
        .flows
        .iter()
        .filter(|flow| flow.to == FlowEnd::Market)
        .all(|flow| flow.type_id == ROBOTICS));
    assert!(result.isk_per_month > 0.0);
    // The P1s arrive at the factory.
    for p1 in [
        REACTIVE_METALS,
        PRECIOUS_METALS,
        TOXIC_METALS,
        CHIRAL_STRUCTURES,
    ] {
        assert!(
            result.colonies.iter().any(
                |colony| colony.role == ColonyRole::Factory && colony.imports.contains_key(&p1)
            ),
            "{p1} never reaches a factory"
        );
    }
}

#[test]
fn another_character_never_lowers_isk() {
    let one = plan(&scenario(vec![character(2)]), &q3hs5(), &market());
    let two = plan(
        &scenario(vec![character(2), character(2)]),
        &q3hs5(),
        &market(),
    );
    assert!(two.isk_per_month >= one.isk_per_month - 1.0);
    assert!(two.colonies_used() > one.colonies_used());
}

#[test]
fn longer_hauls_never_pay_more() {
    let mut short = scenario(vec![character(4)]);
    short.effort.haul_days = 1;
    let mut long = short.clone();
    long.effort.haul_days = 14;
    let short_plan = plan(&short, &q3hs5(), &market());
    let long_plan = plan(&long, &q3hs5(), &market());
    assert!(long_plan.isk_per_month <= short_plan.isk_per_month + 1.0);
    assert!(long_plan.effort.haul_runs_per_week < short_plan.effort.haul_runs_per_week);
}

#[test]
fn the_colony_cap_holds() {
    let mut capped = scenario(vec![character(5), character(5)]);
    capped.max_colonies = Some(3);
    let result = plan(&capped, &q3hs5(), &market());
    assert!(result.colonies_used() <= 3);
    assert!(result.colonies_used() >= 1);
}

#[test]
fn flows_between_characters_are_flagged() {
    // Three one-colony characters: a P2 chain needs all three.
    let only_parts = prices(&[(MECHANICAL_PARTS, 60_000.0)]);
    let result = plan(
        &scenario(vec![character(0), character(0), character(0)]),
        &q3hs5(),
        &only_parts,
    );
    assert!(sold(&result, MECHANICAL_PARTS) > 0.0, "{result:#?}");
    assert!(result.flows.iter().any(|flow| flow.cross_character));
    // Each character holds at most one colony.
    let mut per_character = BTreeMap::new();
    for colony in &result.colonies {
        *per_character.entry(colony.character).or_insert(0) += 1;
    }
    assert!(per_character.values().all(|count| *count <= 1));
}

#[test]
fn two_characters_on_one_hotspot_are_warned() {
    // One planet, one paying P1: both characters extract it there.
    let planets = vec![planet(BARREN_IV, BARREN, &[(BASE_METALS, 80.0)])];
    let result = plan(
        &scenario(vec![character(0), character(0)]),
        &planets,
        &prices(&[(REACTIVE_METALS, 5_000.0)]),
    );
    assert_eq!(result.colonies_used(), 2);
    assert_eq!(
        result.shared_hotspots,
        vec![SharedHotspot {
            planet_id: BARREN_IV,
            resource_type_id: BASE_METALS,
            characters: 2,
        }]
    );
}

#[test]
fn nothing_priced_means_an_empty_plan() {
    let result = plan(&scenario(vec![character(4)]), &q3hs5(), &Prices::default());
    assert!(result.colonies.is_empty());
    assert_eq!(result.isk_per_month, 0.0);
}
