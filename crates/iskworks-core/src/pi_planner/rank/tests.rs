use super::super::fixtures::*;
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

fn planet(planet_id: i64, type_id: i64, densities: &[(i64, DensityEntry)]) -> CandidatePlanet {
    CandidatePlanet {
        planet_id,
        type_id,
        radius_m: Some(5_000_000),
        jumps: 0,
        band: SecurityBand::Null,
        customs_rate: None,
        contested: false,
        densities: densities.iter().copied().collect(),
    }
}

/// Q-3HS5's Lava, Barren and Plasma planets with the densities read in game.
fn q3hs5() -> Vec<CandidatePlanet> {
    vec![
        planet(
            LAVA_I,
            LAVA,
            &[
                (BASE_METALS, scanned(40.0)),
                (HEAVY_METALS, scanned(50.0)),
                (NON_CS_CRYSTALS, scanned(44.0)),
            ],
        ),
        planet(
            BARREN_II,
            BARREN,
            &[
                (BASE_METALS, scanned(58.0)),
                (NOBLE_METALS, scanned(47.0)),
                (AQUEOUS_LIQUIDS, scanned(50.0)),
            ],
        ),
        planet(
            BARREN_IV,
            BARREN,
            &[
                (BASE_METALS, scanned(62.0)),
                (
                    NOBLE_METALS,
                    DensityEntry {
                        density_pct: None,
                        head_value: Some(160.0),
                    },
                ),
            ],
        ),
        planet(
            PLASMA_IX,
            PLASMA,
            &[
                (BASE_METALS, scanned(35.0)),
                (HEAVY_METALS, scanned(61.0)),
                (NOBLE_METALS, scanned(28.0)),
                (NON_CS_CRYSTALS, scanned(57.0)),
            ],
        ),
    ]
}

fn scenario(interplanetary_consolidation: u8, customs_rate: f64) -> RankScenario {
    RankScenario {
        characters: vec![PlannerCharacter {
            interplanetary_consolidation,
            command_center_upgrades: 4,
        }],
        effort: EffortSettings {
            reset_days: 3,
            haul_days: 3,
            uplift_at_login: true,
        },
        default_customs_rate: customs_rate,
        buy_intermediates: false,
        max_colonies: None,
        estimate_unscanned: true,
    }
}

fn prices() -> Prices {
    let best_buy = [
        (WATER, 600.0),
        (REACTIVE_METALS, 500.0),
        (PRECIOUS_METALS, 550.0),
        (TOXIC_METALS, 450.0),
        (CHIRAL_STRUCTURES, 520.0),
        (MECHANICAL_PARTS, 9_000.0),
        (CONSUMER_ELECTRONICS, 9_500.0),
        (ROBOTICS, 90_000.0),
    ];
    Prices {
        best_buy: best_buy.into_iter().collect(),
        best_sell: best_buy
            .into_iter()
            .map(|(id, price)| (id, price * 1.1))
            .collect(),
    }
}

fn ranked(ranking: &Ranking, type_id: i64) -> &RankedProduct {
    ranking
        .ranked
        .iter()
        .find(|product| product.type_id == type_id)
        .unwrap_or_else(|| panic!("{type_id} not ranked: {:?}", ranking.out_of_reach))
}

fn reason(ranking: &Ranking, type_id: i64) -> Option<OutOfReach> {
    ranking
        .out_of_reach
        .iter()
        .find(|(id, _, _)| *id == type_id)
        .map(|(_, _, reason)| *reason)
}

#[test]
fn robotics_is_built_from_the_best_planet_for_each_resource() {
    let ranking = rank(&catalogue(), &scenario(4, 0.1), &q3hs5(), &prices());
    let robotics = ranked(&ranking, ROBOTICS);

    let sources = robotics
        .inputs
        .iter()
        .map(|input| (input.resource_type_id, input.planet_id))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        sources,
        BTreeMap::from([
            (BASE_METALS, BARREN_IV),
            (NOBLE_METALS, BARREN_IV),
            (HEAVY_METALS, PLASMA_IX),
            (NON_CS_CRYSTALS, PLASMA_IX),
        ])
    );
    // Two extraction colonies and a factory; five colonies fit one chain.
    assert_eq!((robotics.colonies, robotics.chains), (3, 1));
    assert!(robotics.isk_per_month > 0.0);
    assert!(robotics.units_per_hour > 0.0);
    // One measured head value, three scanned densities.
    assert_eq!(robotics.confidence, (1.0 + 0.5 * 3.0) / 4.0);
    assert_eq!(robotics.haul_constrained_days, None);
    assert!((robotics.effort.logins_per_week_by_character[0] - 7.0 / 3.0).abs() < 1e-9);
    assert_eq!(robotics.effort.stops_per_run, 3);
    // Highest ISK first.
    assert!(ranking
        .ranked
        .windows(2)
        .all(|pair| pair[0].isk_per_month >= pair[1].isk_per_month));
}

#[test]
fn a_missing_raw_resource_puts_the_product_out_of_reach() {
    let barren_only = q3hs5()
        .into_iter()
        .filter(|planet| planet.type_id == BARREN)
        .collect::<Vec<_>>();
    let ranking = rank(&catalogue(), &scenario(4, 0.1), &barren_only, &prices());
    assert_eq!(
        reason(&ranking, ROBOTICS),
        Some(OutOfReach::MissingResource {
            type_id: HEAVY_METALS
        })
    );
    assert!(ranking
        .ranked
        .iter()
        .any(|product| product.type_id == MECHANICAL_PARTS));
}

#[test]
fn unscanned_resources_are_left_out_unless_estimated() {
    // Nobody has read Heavy Metals on the Lava or the Plasma planet.
    let mut planets = q3hs5();
    for planet in &mut planets {
        planet.densities.remove(&HEAVY_METALS);
    }
    let scanned_only = RankScenario {
        estimate_unscanned: false,
        ..scenario(4, 0.1)
    };
    let ranking = rank(&catalogue(), &scanned_only, &planets, &prices());
    assert_eq!(
        reason(&ranking, ROBOTICS),
        Some(OutOfReach::NotScanned {
            type_id: HEAVY_METALS
        })
    );
    assert!(ranking
        .ranked
        .iter()
        .flat_map(|product| &product.inputs)
        .all(|input| input.source != YieldSource::Default));

    let ranking = rank(&catalogue(), &scenario(4, 0.1), &planets, &prices());
    let robotics = ranked(&ranking, ROBOTICS);
    assert!(robotics.inputs.iter().any(
        |input| input.resource_type_id == HEAVY_METALS && input.source == YieldSource::Default
    ));
}

#[test]
fn a_missing_p1_can_be_bought_instead() {
    let barren_only = q3hs5()
        .into_iter()
        .filter(|planet| planet.type_id == BARREN)
        .collect::<Vec<_>>();
    let buying = RankScenario {
        buy_intermediates: true,
        ..scenario(4, 0.1)
    };
    let ranking = rank(&catalogue(), &buying, &barren_only, &prices());
    let robotics = ranked(&ranking, ROBOTICS);
    assert_eq!(robotics.bought, vec![TOXIC_METALS, CHIRAL_STRUCTURES]);
}

#[test]
fn a_product_without_a_price_is_out_of_reach() {
    let mut prices = prices();
    prices.best_buy.remove(&ROBOTICS);
    let ranking = rank(&catalogue(), &scenario(4, 0.1), &q3hs5(), &prices);
    assert_eq!(reason(&ranking, ROBOTICS), Some(OutOfReach::NoPrice));
}

#[test]
fn a_chain_needing_more_colonies_than_the_characters_have_is_out_of_reach() {
    let ranking = rank(&catalogue(), &scenario(0, 0.1), &q3hs5(), &prices());
    assert_eq!(
        reason(&ranking, ROBOTICS),
        Some(OutOfReach::NeedsColonies {
            needed: 3,
            available: 1
        })
    );
    // A P1 needs a single colony.
    assert_eq!(ranked(&ranking, WATER).colonies, 1);
}

#[test]
fn customs_tax_lowers_isk_by_exactly_the_export_cost() {
    let catalogue = catalogue();
    let free = rank(&catalogue, &scenario(0, 0.0), &q3hs5(), &prices());
    let taxed = rank(&catalogue, &scenario(0, 0.1), &q3hs5(), &prices());
    let (free, taxed) = (ranked(&free, WATER), ranked(&taxed, WATER));
    assert_eq!(free.units_per_hour, taxed.units_per_hour);
    let export_cost = 720.0 * free.units_per_hour * catalogue.export_tax(WATER, 0.1);
    assert!((free.isk_per_month - taxed.isk_per_month - export_cost).abs() < 1e-6);
}

#[test]
fn a_fortnightly_haul_that_overflows_a_skyhook_is_shortened() {
    let rich = q3hs5()
        .into_iter()
        .map(|mut planet| {
            for entry in planet.densities.values_mut() {
                entry.head_value = Some(2_000.0);
            }
            planet
        })
        .collect::<Vec<_>>();
    let fortnightly = RankScenario {
        effort: EffortSettings {
            reset_days: 1,
            haul_days: 14,
            uplift_at_login: true,
        },
        ..scenario(4, 0.1)
    };
    let ranking = rank(&catalogue(), &fortnightly, &rich, &prices());
    // The factory's P1 inputs overflow 35 000 m³ over 14 days but not 7.
    let robotics = ranked(&ranking, ROBOTICS);
    assert_eq!(robotics.haul_constrained_days, Some(7));
    assert_eq!(robotics.effort.haul_runs_per_week, 1.0);
    // An extraction colony's P1 still fits a fortnight.
    assert_eq!(ranked(&ranking, WATER).haul_constrained_days, None);
}
