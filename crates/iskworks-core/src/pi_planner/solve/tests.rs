use std::collections::BTreeMap;

use super::super::effort::EffortSettings;
use super::super::effort::{storage_needed, STORAGE_M3};
use super::super::fixtures::*;
use super::super::rank::{rank, PlannerCharacter};
use super::super::yields::YieldSource;
use super::*;

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
        max_systems_per_character: None,
        estimate_unscanned: true,
        target: None,
    }
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
fn without_estimates_only_entered_readings_are_planned() {
    // A Barren next door that nobody has scanned.
    let mut planets = q3hs5();
    planets.push(planet(40_000_101, BARREN, &[]));
    let scanned_only = RankScenario {
        estimate_unscanned: false,
        ..scenario(vec![character(5), character(5)])
    };
    let result = plan(&scanned_only, &planets, &market());
    assert!(result.isk_per_month > 0.0);
    assert!(result
        .inputs
        .iter()
        .all(|input| input.source != YieldSource::Default));
    assert!(result
        .colonies
        .iter()
        .filter(|colony| colony.planet_id == 40_000_101)
        .all(|colony| colony.extracts.is_empty()));
}

#[test]
fn a_target_sells_only_that_product_and_its_intermediates() {
    let targeted = |product: i64| RankScenario {
        target: Some(product),
        ..scenario(vec![character(5), character(5)])
    };
    let market_types = |plan: &PiPlan| {
        plan.flows
            .iter()
            .filter(|flow| flow.to == FlowEnd::Market)
            .map(|flow| flow.type_id)
            .collect::<std::collections::BTreeSet<_>>()
    };

    let reactive = plan(&targeted(REACTIVE_METALS), &q3hs5(), &market());
    assert!(reactive.isk_per_month > 0.0);
    assert_eq!(market_types(&reactive), [REACTIVE_METALS].into());

    let robotics = plan(&targeted(ROBOTICS), &q3hs5(), &market());
    assert!(sold(&robotics, ROBOTICS) > 0.0);
    let chain = super::super::chain::expand(&catalogue(), ROBOTICS, 1.0).unwrap();
    assert!(market_types(&robotics)
        .iter()
        .all(|type_id| chain.per_hour.contains_key(type_id)));
    // The best mix earns at least as much as any one product.
    let best = plan(
        &scenario(vec![character(5), character(5)]),
        &q3hs5(),
        &market(),
    );
    assert!(best.isk_per_month >= robotics.isk_per_month - 1.0);
    assert!(best.isk_per_month >= reactive.isk_per_month - 1.0);
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
fn a_factory_colony_adds_storage_for_what_its_launchpad_cannot_hold() {
    let only_robotics = prices(&[(ROBOTICS, 200_000.0)]);
    let mut long_hauls = scenario(vec![character(5)]);
    long_hauls.effort.haul_days = 14;
    let result = plan(&long_hauls, &q3hs5(), &only_robotics);

    let factory = result
        .colonies
        .iter()
        .find(|colony| colony.role == ColonyRole::Factory)
        .expect("a factory colony");
    assert!(factory.layout.extra_storage > 0, "{factory:#?}");
    assert!(
        factory.on_planet_m3 <= LAUNCHPAD_M3 + STORAGE_M3 * f64::from(factory.layout.extra_storage),
        "{factory:#?}"
    );
    // The storage is paid for: the colony still fits its command center.
    assert!(
        factory.cpu_used <= 1.0 && factory.power_used <= 1.0,
        "{factory:#?}"
    );
}

#[test]
fn a_factory_colony_without_much_to_hold_adds_no_storage() {
    let only_robotics = prices(&[(ROBOTICS, 200_000.0)]);
    let mut short_hauls = scenario(vec![character(5)]);
    short_hauls.effort.haul_days = 1;
    let result = plan(&short_hauls, &q3hs5(), &only_robotics);

    for factory in result
        .colonies
        .iter()
        .filter(|colony| colony.role == ColonyRole::Factory)
    {
        assert_eq!(
            factory.layout.extra_storage,
            storage_needed(factory.on_planet_m3),
            "{factory:#?}"
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

fn pair(colonies_each: u32) -> Vec<Group> {
    vec![Group {
        characters: vec![0, 1],
        colonies_each,
        level: 4,
    }]
}

/// Planets by index: `systems[i]` is planet i's system.
fn spread(systems: &[i64]) -> Vec<CandidatePlanet> {
    systems
        .iter()
        .enumerate()
        .map(|(index, system)| in_system(planet(40_000_000 + index as i64, BARREN, &[]), *system))
        .collect()
}

fn systems_of(
    planets: &[CandidatePlanet],
    wanted: &[(usize, usize)],
    held: &[usize],
    character: usize,
) -> BTreeSet<i64> {
    wanted
        .iter()
        .zip(held)
        .filter(|(_, owner)| **owner == character)
        .map(|((_, planet), _)| planets[*planet].system_id)
        .collect()
}

#[test]
fn colonies_are_handed_out_one_system_at_a_time() {
    // Three planets in system 1, then three in 2: dealt by fewest colonies
    // first, both characters would land in both.
    let planets = spread(&[1, 1, 1, 2, 2, 2]);
    let wanted = (0..6).map(|planet| (0, planet)).collect::<Vec<_>>();
    let held = assign(&pair(6), &planets, &wanted, None);

    assert_eq!(systems_of(&planets, &wanted, &held, 0).len(), 1, "{held:?}");
    assert_eq!(systems_of(&planets, &wanted, &held, 1).len(), 1, "{held:?}");
    assert_eq!(assign(&pair(6), &planets, &wanted, None), held, "stable");
}

#[test]
fn the_handout_keeps_each_character_within_its_colonies() {
    let planets = spread(&[1, 1, 1, 1]);
    let wanted = (0..4).map(|planet| (0, planet)).collect::<Vec<_>>();
    let held = assign(&pair(2), &planets, &wanted, None);

    for character in [0, 1] {
        assert_eq!(
            held.iter().filter(|owner| **owner == character).count(),
            2,
            "{held:?}"
        );
    }
}

#[test]
fn the_handout_never_puts_two_colonies_of_one_character_on_a_planet() {
    let planets = spread(&[1, 1]);
    let wanted = vec![(0, 0), (0, 0), (0, 1)];
    let held = assign(&pair(6), &planets, &wanted, None);

    assert_ne!(held[0], held[1], "{held:?}");
}

#[test]
fn the_plan_counts_each_characters_systems() {
    let planets = q3hs5()
        .into_iter()
        .enumerate()
        .map(|(index, planet)| in_system(planet, index as i64 + 1))
        .collect::<Vec<_>>();
    let result = plan(
        &scenario(vec![character(5), character(5)]),
        &planets,
        &market(),
    );

    assert_eq!(result.systems_by_character.len(), 2);
    for (character, count) in result.systems_by_character.iter().enumerate() {
        // Every planet is its own system here.
        let systems = result
            .colonies
            .iter()
            .filter(|colony| colony.character == character)
            .map(|colony| colony.planet_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(*count as usize, systems.len(), "{result:?}");
    }
}

#[test]
fn an_unused_character_visits_no_systems() {
    let result = plan(&scenario(vec![character(4)]), &q3hs5(), &Prices::default());
    assert_eq!(result.systems_by_character, vec![0]);
}

/// Q-3HS5's planets spread over systems: `systems[i]` is planet i's.
fn q3hs5_over(systems: [i64; 4]) -> Vec<CandidatePlanet> {
    q3hs5()
        .into_iter()
        .zip(systems)
        .map(|(planet, system)| in_system(planet, system))
        .collect()
}

fn limited(characters: Vec<PlannerCharacter>, systems: u32) -> RankScenario {
    RankScenario {
        max_systems_per_character: Some(systems),
        ..scenario(characters)
    }
}

#[test]
fn one_system_per_character_keeps_a_character_in_one_system() {
    let planets = q3hs5_over([1, 1, 2, 2]);
    let free = plan(&scenario(vec![character(5)]), &planets, &market());
    assert_eq!(
        free.systems_by_character,
        vec![2],
        "the unlimited plan spreads: {free:#?}"
    );

    let one = plan(&limited(vec![character(5)], 1), &planets, &market());
    assert_eq!(one.systems_by_character, vec![1], "{one:#?}");
    assert!(!one.systems_over_limit);
    assert!(!free.systems_over_limit, "no limit, nothing to exceed");
    assert!(one.isk_per_month > 0.0);
    assert!(one.isk_per_month <= free.isk_per_month + 1.0);
}

#[test]
fn one_system_per_character_holds_across_groups() {
    let planets = q3hs5_over([1, 1, 2, 2]);
    let characters = vec![character(5), character(3), character(3)];
    let result = plan(&limited(characters, 1), &planets, &market());
    assert!(result.isk_per_month > 0.0);
    assert!(
        result.systems_by_character.iter().all(|count| *count <= 1),
        "{result:#?}"
    );
}

#[test]
fn two_systems_per_character_holds_over_three_systems() {
    let planets = q3hs5_over([1, 2, 2, 3]);
    let characters = vec![
        character(5),
        character(5),
        character(4),
        PlannerCharacter {
            interplanetary_consolidation: 5,
            command_center_upgrades: 3,
        },
    ];
    let result = plan(&limited(characters, 2), &planets, &market());
    assert!(result.isk_per_month > 0.0);
    assert!(
        result.systems_by_character.iter().all(|count| *count <= 2),
        "{result:#?}"
    );
}

#[test]
fn a_limited_plan_still_buys_factory_storage() {
    let only_robotics = prices(&[(ROBOTICS, 200_000.0)]);
    let mut long_hauls = limited(vec![character(5)], 1);
    long_hauls.effort.haul_days = 14;
    let result = plan(&long_hauls, &q3hs5(), &only_robotics);

    let factory = result
        .colonies
        .iter()
        .find(|colony| colony.role == ColonyRole::Factory)
        .expect("a factory colony");
    assert!(factory.layout.extra_storage > 0, "{factory:#?}");
}

#[test]
fn a_handout_that_cannot_meet_the_limit_still_places_every_colony() {
    // One character, colonies wanted in two systems, a limit of one.
    let planets = spread(&[1, 2]);
    let solo = vec![Group {
        characters: vec![0],
        colonies_each: 6,
        level: 4,
    }];
    let held = assign(&solo, &planets, &[(0, 0), (0, 1)], Some(1));
    assert_eq!(held, vec![0, 0]);
    assert!(over_limit(&[2], Some(1)));
    assert!(!over_limit(&[1, 0], Some(1)));
    assert!(!over_limit(&[3], None));
}
