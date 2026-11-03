use std::collections::{BTreeMap, BTreeSet};

use iskworks_sde::PlanetPinKind;

use super::super::capacity::ColonyLayout;
use super::super::fixtures::*;
use super::super::solve::{ColonyRole, PlannedColony};
use super::*;

fn colony(role: ColonyRole, layout: ColonyLayout, recipes: &[(i64, f64)]) -> PlannedColony {
    PlannedColony {
        character: 0,
        planet_id: BARREN_IV,
        role,
        layout,
        extracts: Vec::new(),
        imports: BTreeMap::new(),
        exports: BTreeMap::new(),
        recipes: recipes.iter().copied().collect(),
        tax_per_month: 0.0,
        cpu_used: 0.0,
        power_used: 0.0,
        on_planet_m3: 0.0,
        skyhook_m3: 0.0,
    }
}

fn barren() -> CandidatePlanet {
    planet(
        BARREN_IV,
        BARREN,
        &[(BASE_METALS, 62.0), (NOBLE_METALS, 60.0)],
    )
}

fn extraction() -> PlannedColony {
    PlannedColony {
        extracts: vec![(BASE_METALS, 9_000.0), (NOBLE_METALS, 6_000.0)],
        ..colony(
            ColonyRole::Extraction,
            ColonyLayout {
                ecus: 2,
                heads: 13,
                processors: [(2473, 5)].into(),
                launchpads: 1,
                extra_storage: 1,
            },
            &[(REACTIVE_METALS, 2.4), (PRECIOUS_METALS, 1.6)],
        )
    }
}

fn template(colony: &PlannedColony, level: u8) -> ColonyTemplate {
    colony_template(&catalogue(), &barren(), colony, level, 72.0, "test".into()).expect("template")
}

fn kinds(template: &ColonyTemplate, kind: PlanetPinKind) -> Vec<&TemplatePin> {
    template
        .pins
        .iter()
        .filter(|pin| pin.kind == kind)
        .collect()
}

#[test]
fn an_extraction_colony_has_a_hub_factories_and_ecus() {
    let result = template(&extraction(), 5);
    assert_eq!(result.pins[0].kind, PlanetPinKind::Launchpad);
    assert_eq!(result.pins[0].type_id, 2544);
    assert_eq!(kinds(&result, PlanetPinKind::Launchpad).len(), 1);
    assert_eq!(kinds(&result, PlanetPinKind::Storage).len(), 1);

    // ceil(2.4) + ceil(1.6) basic factories, each labelled with its P1.
    let factories = kinds(&result, PlanetPinKind::Processor);
    assert_eq!(factories.len(), 5);
    assert_eq!(
        factories
            .iter()
            .filter(|pin| pin.product == Some(REACTIVE_METALS))
            .count(),
        3
    );
    assert!(factories.iter().all(|pin| pin.type_id == 2473));

    // One ECU per resource; 13 heads split 7 + 6.
    let ecus = kinds(&result, PlanetPinKind::ExtractorControlUnit);
    assert_eq!(
        ecus.iter()
            .map(|pin| (pin.product, pin.heads))
            .collect::<Vec<_>>(),
        vec![(Some(BASE_METALS), 7), (Some(NOBLE_METALS), 6)]
    );
    // ECUs sit furthest out.
    let distance = |pin: &TemplatePin| pin.x_km.hypot(pin.y_km);
    let nearest_ecu = ecus
        .iter()
        .map(|pin| distance(pin))
        .fold(f64::MAX, f64::min);
    assert!(factories
        .iter()
        .all(|pin| distance(pin) <= nearest_ecu + 1e-9));
}

#[test]
fn every_pin_links_to_the_hub_and_routes_follow_links() {
    let result = template(&extraction(), 5);
    assert_eq!(result.links.len(), result.pins.len() - 1);
    assert!(result
        .links
        .iter()
        .all(|link| link.to == 0 && link.length_km > 0.0));
    let linked = result
        .links
        .iter()
        .flat_map(|link| [(link.from, link.to), (link.to, link.from)])
        .collect::<BTreeSet<_>>();
    for route in &result.routes {
        for hop in route.path.windows(2) {
            assert!(linked.contains(&(hop[0], hop[1])), "{route:?}");
        }
    }

    // The ECU ships the whole program's output; factories cycle by recipe.
    let base = result
        .routes
        .iter()
        .find(|route| route.type_id == BASE_METALS && route.path[1] == 0)
        .expect("ECU route");
    assert_eq!(base.quantity, 9_000 * 72);
    let recipe = catalogue().recipe(REACTIVE_METALS).cloned().unwrap();
    assert_eq!(
        result
            .routes
            .iter()
            .filter(|route| route.type_id == REACTIVE_METALS && route.path[1] == 0)
            .count(),
        3
    );
    assert!(result
        .routes
        .iter()
        .filter(|route| route.type_id == BASE_METALS && route.path[0] == 0)
        .all(|route| route.quantity == recipe.inputs[0].1.round() as i64));
}

#[test]
fn neighbours_keep_their_pitch() {
    let result = template(&extraction(), 5);
    // Ring one sits one pitch from the hub: 0.012 rad of a 5,000 km radius.
    let shortest = result
        .links
        .iter()
        .map(|link| link.length_km)
        .fold(f64::MAX, f64::min);
    assert!((shortest - 60.0).abs() < 0.5, "{shortest}");
    let positions = result
        .pins
        .iter()
        .map(|pin| {
            (
                (pin.x_km * 10.0).round() as i64,
                (pin.y_km * 10.0).round() as i64,
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(positions.len(), result.pins.len(), "two pins share a spot");
}

#[test]
fn a_factory_colony_cycles_each_recipe_through_the_hub() {
    let factory = colony(
        ColonyRole::Factory,
        ColonyLayout {
            launchpads: 1,
            ..ColonyLayout::default()
        },
        &[(MECHANICAL_PARTS, 2.0)],
    );
    let result = template(&factory, 5);
    let factories = kinds(&result, PlanetPinKind::Processor);
    assert_eq!(factories.len(), 2);
    assert!(factories
        .iter()
        .all(|pin| pin.type_id == 2474 && pin.product == Some(MECHANICAL_PARTS)));
    let recipe = catalogue().recipe(MECHANICAL_PARTS).cloned().unwrap();
    assert_eq!(result.routes.len(), 2 * (recipe.inputs.len() + 1));
    assert!(kinds(&result, PlanetPinKind::ExtractorControlUnit).is_empty());
}

#[test]
fn the_json_reads_like_a_player_export() {
    let result = template(&extraction(), 5);
    let json = result.to_json();
    assert!(
        json.starts_with(r#"{"CmdCtrLv":5,"Cmt":"test","Diam":10000.0,"L":[{"D":1,"Lv":0,"S":2}"#),
        "{json}"
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["Pln"], BARREN);
    assert_eq!(value["P"].as_array().unwrap().len(), result.pins.len());
    assert_eq!(value["P"][0]["S"], serde_json::Value::Null);
    // Five decimals, as player exports write them.
    assert_eq!(value["P"][0]["La"].to_string(), "1.5708");
    assert_eq!(value["R"][0]["P"][0], 1);
}

#[test]
fn the_load_is_measured_against_the_command_center() {
    let full = template(&extraction(), 5);
    assert!(full.fits(), "{} / {}", full.cpu, full.cpu_output);
    assert!(full.cpu > 0.0 && full.power > 0.0);
    // The same colony is far too big for an unupgraded command center.
    let bare = template(&extraction(), 0);
    assert!(!bare.fits());
    assert_eq!(bare.cpu, full.cpu);
}

#[test]
fn planets_without_a_launchpad_have_no_template() {
    let shattered = planet(40_000_099, SHATTERED, &[]);
    assert!(colony_template(
        &catalogue(),
        &shattered,
        &extraction(),
        5,
        72.0,
        String::new()
    )
    .is_none());
}
