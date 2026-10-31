use super::super::effort::EffortSettings;
use super::super::fixtures::*;
use super::super::rank::PlannerCharacter;
use super::super::solve::solve;
use super::super::yields::DensityEntry;
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
    }
}

/// Q-3HS5 at 0 jumps, plus a Barren one jump out that nobody has scanned.
fn neighbourhood() -> Vec<CandidatePlanet> {
    let mut planets = q3hs5();
    let mut next_door = planet(40_000_101, BARREN, &[]);
    next_door.jumps = 1;
    planets.push(next_door);
    planets
}

fn run(scenario: &RankScenario, planets: &[CandidatePlanet]) -> (PiPlan, WhatIfs) {
    let catalogue = catalogue();
    let plan = solve(&catalogue, scenario, planets, &market()).expect("solves");
    let what_ifs = what_ifs(&catalogue, scenario, planets, &market(), 1, &plan);
    (plan, what_ifs)
}

#[test]
fn every_character_and_jump_adds_or_keeps_isk() {
    let (plan, what_ifs) = run(
        &scenario(vec![character(1), character(1), character(1)]),
        &neighbourhood(),
    );
    assert_eq!(what_ifs.per_character.len(), 3);
    assert_eq!(*what_ifs.per_character.last().unwrap(), plan.isk_per_month);
    assert!(what_ifs
        .per_character
        .windows(2)
        .all(|pair| pair[1] >= pair[0] - 1.0));
    assert_eq!(
        what_ifs
            .per_radius
            .iter()
            .map(|(jumps, _)| *jumps)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(what_ifs.per_radius[1].1 >= what_ifs.per_radius[0].1 - 1.0);
}

#[test]
fn the_grid_covers_every_cadence_and_matches_the_plan() {
    let scenario = scenario(vec![character(4)]);
    let (plan, what_ifs) = run(&scenario, &neighbourhood());
    assert_eq!(what_ifs.grid.len(), 12);
    let current = what_ifs
        .grid
        .iter()
        .find(|cell| cell.reset_days == 3 && cell.haul_days == 3)
        .unwrap();
    assert!((current.isk_per_month - plan.isk_per_month).abs() < 1.0);
    // Daily resets mean daily logins.
    let daily = what_ifs
        .grid
        .iter()
        .find(|cell| cell.reset_days == 1 && cell.haul_days == 1)
        .unwrap();
    assert!((daily.logins_per_week - 7.0).abs() < 1e-6);
    assert!(daily.logins_per_week > current.logins_per_week);
}

#[test]
fn scan_first_puts_unscanned_readings_first() {
    // The unscanned Barren one jump out holds the best Aqueous Liquids by
    // default, so the plan rests on a guess there.
    let mut planets = neighbourhood();
    planets[1].densities.remove(&AQUEOUS_LIQUIDS);
    planets[4].densities.insert(
        NOBLE_METALS,
        DensityEntry {
            density_pct: None,
            head_value: Some(200.0),
        },
    );
    let (plan, what_ifs) = run(&scenario(vec![character(5), character(5)]), &planets);
    assert!(!what_ifs.scan_first.is_empty());
    assert!(what_ifs.scan_first.len() <= SCAN_FIRST);
    let defaults = what_ifs
        .scan_first
        .iter()
        .take_while(|entry| entry.source == YieldSource::Default)
        .count();
    let all_defaults = plan
        .inputs
        .iter()
        .filter(|input| input.source == YieldSource::Default)
        .count()
        .min(SCAN_FIRST);
    assert_eq!(defaults, all_defaults, "{:?}", what_ifs.scan_first);
    // Within each kind, the bigger loss comes first.
    for pair in what_ifs.scan_first[defaults..].windows(2) {
        assert!(pair[0].isk_lost_per_month >= pair[1].isk_lost_per_month);
    }
    assert!(what_ifs
        .scan_first
        .iter()
        .all(|entry| entry.isk_lost_per_month >= 0.0));
}
