//! A planned colony as an EVE colony template: the JSON Planetary Industry
//! imports through Templates › Import & Export › Save from Clipboard.
//!
//! The format, read from player exports: pins `P` (1-based in links and
//! routes) carry a structure type `T`, a product `S` (a factory's output or an
//! ECU's resource), heads `H` and a position (`La` colatitude, `Lo`
//! longitude, radians). Links `L` join pins `S`–`D`; routes `R` move `Q` of
//! `T` along a path of linked pins. The command center is not a pin.

use iskworks_sde::PlanetPinKind;
use serde_json::{json, Value};

use super::capacity::MAX_HEADS_PER_ECU;
use super::catalogue::{command_center_output, decimal_f64, PiCatalogue};
use super::rank::CandidatePlanet;
use super::solve::PlannedColony;

/// Angle between neighbouring structures, radians. Community templates use
/// about this on both small and large planets.
pub const TEMPLATE_PITCH_RAD: f64 = 0.012;

/// Used when a planet's radius is unknown.
const DEFAULT_RADIUS_KM: f64 = 5_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct TemplatePin {
    pub kind: PlanetPinKind,
    pub type_id: i64,
    /// A factory's output or an ECU's resource.
    pub product: Option<i64>,
    pub heads: u32,
    pub colatitude: f64,
    pub longitude: f64,
    /// Offset from the hub on a flat projection, km; for drawing.
    pub x_km: f64,
    pub y_km: f64,
}

/// Pins by position in [`ColonyTemplate::pins`].
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateLink {
    pub from: usize,
    pub to: usize,
    pub length_km: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRoute {
    pub path: Vec<usize>,
    pub type_id: i64,
    /// Units per cycle.
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColonyTemplate {
    pub planet_type_id: i64,
    pub diameter_km: f64,
    pub command_center_level: u8,
    pub comment: String,
    /// The hub launchpad is first.
    pub pins: Vec<TemplatePin>,
    pub links: Vec<TemplateLink>,
    pub routes: Vec<TemplateRoute>,
    /// Measured load with these link lengths, and the command center's output.
    pub cpu: f64,
    pub power: f64,
    pub cpu_output: f64,
    pub power_output: f64,
}

impl ColonyTemplate {
    #[must_use]
    pub fn fits(&self) -> bool {
        self.cpu <= self.cpu_output && self.power <= self.power_output
    }

    /// The template as EVE reads it. `serde_json` sorts keys, which is the
    /// order player exports use.
    #[must_use]
    pub fn to_json(&self) -> String {
        let round = |value: f64| (value * 100_000.0).round() / 100_000.0;
        let template: Value = json!({
            "CmdCtrLv": self.command_center_level,
            "Cmt": self.comment,
            "Diam": self.diameter_km.round(),
            "L": self.links.iter().map(|link| json!({
                "S": link.from + 1,
                "D": link.to + 1,
                "Lv": 0,
            })).collect::<Vec<_>>(),
            "P": self.pins.iter().map(|pin| json!({
                "H": pin.heads,
                "La": round(pin.colatitude),
                "Lo": round(pin.longitude),
                "S": pin.product,
                "T": pin.type_id,
            })).collect::<Vec<_>>(),
            "Pln": self.planet_type_id,
            "R": self.routes.iter().map(|route| json!({
                "P": route.path.iter().map(|pin| pin + 1).collect::<Vec<_>>(),
                "Q": route.quantity,
                "T": route.type_id,
            })).collect::<Vec<_>>(),
        });
        template.to_string()
    }
}

/// Hexagonal rings around the origin, nearest first, in pitch units.
fn ring_slots() -> impl Iterator<Item = (f64, f64)> {
    const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
    (1..).flat_map(|ring: i32| {
        // Start at the corner `ring` steps along direction 4, then walk the
        // six sides.
        let (mut q, mut r) = (DIRECTIONS[4].0 * ring, DIRECTIONS[4].1 * ring);
        let mut slots = Vec::new();
        for direction in DIRECTIONS {
            for _ in 0..ring {
                slots.push((
                    f64::from(q) + f64::from(r) / 2.0,
                    f64::from(r) * 3f64.sqrt() / 2.0,
                ));
                q += direction.0;
                r += direction.1;
            }
        }
        slots
    })
}

/// Great-circle angle between two (colatitude, longitude) points, radians.
fn angle_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat_a, lat_b) = (
        std::f64::consts::FRAC_PI_2 - a.0,
        std::f64::consts::FRAC_PI_2 - b.0,
    );
    let half_lat = (lat_b - lat_a) / 2.0;
    let half_lon = (b.1 - a.1) / 2.0;
    let h = half_lat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_lon.sin().powi(2);
    2.0 * h.sqrt().min(1.0).asin()
}

/// `colony` as a template on `planet`: one launchpad hub every other structure
/// links to, a factory per recipe, ECUs outermost. Routes run through the hub.
/// `program_hours` sizes the ECU routes. `None` when the planet type has no
/// launchpad.
#[must_use]
pub fn colony_template(
    catalogue: &PiCatalogue,
    planet: &CandidatePlanet,
    colony: &PlannedColony,
    command_center_level: u8,
    program_hours: f64,
    comment: String,
) -> Option<ColonyTemplate> {
    let planet_type = planet.type_id;
    let launchpad = catalogue.pin_on(PlanetPinKind::Launchpad, planet_type)?;
    let radius_km = planet
        .radius_m
        .map_or(DEFAULT_RADIUS_KM, |radius| radius as f64 / 1_000.0);

    // Structures other than the hub, nearest first: (kind, type, product, heads).
    let mut others: Vec<(PlanetPinKind, i64, Option<i64>, u32)> = Vec::new();
    for (output, factories) in &colony.recipes {
        let Some(recipe) = catalogue.recipe(*output) else {
            continue;
        };
        let Some(processor) = catalogue.processor_for(recipe, planet_type) else {
            continue;
        };
        let count = (factories - 1e-6).ceil().max(0.0) as u32;
        for _ in 0..count {
            others.push((
                PlanetPinKind::Processor,
                processor.type_id,
                Some(*output),
                0,
            ));
        }
    }
    for _ in 1..colony.layout.launchpads {
        others.push((PlanetPinKind::Launchpad, launchpad.type_id, None, 0));
    }
    if let Some(storage) = catalogue.pin_on(PlanetPinKind::Storage, planet_type) {
        for _ in 0..colony.layout.extra_storage {
            others.push((PlanetPinKind::Storage, storage.type_id, None, 0));
        }
    }
    let ecu = catalogue.pin_on(PlanetPinKind::ExtractorControlUnit, planet_type);
    let ecus = if ecu.is_some() { colony.layout.ecus } else { 0 };
    let mut ecu_rates = Vec::new();
    for index in 0..ecus {
        let resource = (!colony.extracts.is_empty())
            .then(|| colony.extracts[index as usize % colony.extracts.len()]);
        let sharing = colony.extracts.len().max(1) as u32;
        let on_resource = ecus / sharing + u32::from(index % sharing < ecus % sharing);
        let heads = (colony.layout.heads / ecus + u32::from(index < colony.layout.heads % ecus))
            .min(MAX_HEADS_PER_ECU);
        others.push((
            PlanetPinKind::ExtractorControlUnit,
            ecu.map_or(0, |pin| pin.type_id),
            resource.map(|(resource, _)| resource),
            heads,
        ));
        ecu_rates.push(resource.map(|(_, rate)| rate / f64::from(on_resource.max(1))));
    }

    let hub = (std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    let mut pins = vec![TemplatePin {
        kind: PlanetPinKind::Launchpad,
        type_id: launchpad.type_id,
        product: None,
        heads: 0,
        colatitude: hub.0,
        longitude: hub.1,
        x_km: 0.0,
        y_km: 0.0,
    }];
    for ((kind, type_id, product, heads), (x, y)) in others.into_iter().zip(ring_slots()) {
        let colatitude = hub.0 - y * TEMPLATE_PITCH_RAD;
        pins.push(TemplatePin {
            kind,
            type_id,
            product,
            heads,
            colatitude,
            longitude: hub.1 + x * TEMPLATE_PITCH_RAD / colatitude.sin(),
            x_km: x * TEMPLATE_PITCH_RAD * radius_km,
            y_km: y * TEMPLATE_PITCH_RAD * radius_km,
        });
    }
    let links = (1..pins.len())
        .map(|index| TemplateLink {
            from: index,
            to: 0,
            length_km: angle_between((pins[index].colatitude, pins[index].longitude), hub)
                * radius_km,
        })
        .collect::<Vec<_>>();

    let mut routes = Vec::new();
    let mut ecu_index = 0;
    for (index, pin) in pins.iter().enumerate().skip(1) {
        match (pin.kind, pin.product) {
            (PlanetPinKind::Processor, Some(output)) => {
                let Some(recipe) = catalogue.recipe(output) else {
                    continue;
                };
                for (input, quantity) in &recipe.inputs {
                    routes.push(TemplateRoute {
                        path: vec![0, index],
                        type_id: *input,
                        quantity: quantity.round() as i64,
                    });
                }
                routes.push(TemplateRoute {
                    path: vec![index, 0],
                    type_id: output,
                    quantity: recipe.output_quantity.round() as i64,
                });
            }
            (PlanetPinKind::ExtractorControlUnit, product) => {
                if let (Some(resource), Some(Some(rate))) = (product, ecu_rates.get(ecu_index)) {
                    routes.push(TemplateRoute {
                        path: vec![index, 0],
                        type_id: resource,
                        quantity: (rate * program_hours).round() as i64,
                    });
                }
                ecu_index += 1;
            }
            _ => {}
        }
    }

    let (link_cpu, link_power, cpu_per_km, power_per_km) = catalogue.link_cost();
    let mut cpu = 0.0;
    let mut power = 0.0;
    for pin in &pins {
        if let Some(structure) = catalogue.pin(pin.type_id) {
            cpu += decimal_f64(structure.cpu_load)
                + f64::from(pin.heads) * decimal_f64(structure.ecu_head_cpu);
            power += decimal_f64(structure.power_load)
                + f64::from(pin.heads) * decimal_f64(structure.ecu_head_power);
        }
    }
    for link in &links {
        cpu += link_cpu + cpu_per_km * link.length_km;
        power += link_power + power_per_km * link.length_km;
    }
    let (cpu_output, power_output) = command_center_output(command_center_level);

    Some(ColonyTemplate {
        planet_type_id: planet_type,
        diameter_km: radius_km * 2.0,
        command_center_level: command_center_level.min(5),
        comment,
        pins,
        links,
        routes,
        cpu,
        power,
        cpu_output,
        power_output,
    })
}

#[cfg(test)]
mod tests;
