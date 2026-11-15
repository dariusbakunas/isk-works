//! What fits on one colony: a command center's CPU and power against the
//! structures, extractor heads and links built on it.

use std::collections::BTreeMap;

use iskworks_sde::PlanetPinKind;

use super::catalogue::{command_center_output, decimal_f64, PiCatalogue};

/// Angle between neighbouring structures, radians. Community templates use
/// about this on both small and large planets.
pub const TEMPLATE_PITCH_RAD: f64 = 0.012;

/// Used when a planet's radius is unknown.
pub(super) const DEFAULT_RADIUS_KM: f64 = 5_000.0;

/// Where a colony's hub launchpad sits: (colatitude, longitude), radians.
pub(super) const HUB: (f64, f64) = (std::f64::consts::FRAC_PI_2, std::f64::consts::PI);

/// Link lengths a budget keeps ready; no colony links this many structures.
const MAX_LINKS: usize = 128;

/// Hexagonal rings around the hub, nearest first, in pitch units: where a
/// colony's structures other than the hub go, in the order they are placed.
pub(super) fn ring_slots() -> impl Iterator<Item = (f64, f64)> {
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

/// A ring slot's (colatitude, longitude), radians.
pub(super) fn slot_position((x, y): (f64, f64)) -> (f64, f64) {
    let colatitude = HUB.0 - y * TEMPLATE_PITCH_RAD;
    (
        colatitude,
        HUB.1 + x * TEMPLATE_PITCH_RAD / colatitude.sin(),
    )
}

/// Great-circle angle between two (colatitude, longitude) points, radians.
pub(super) fn angle_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat_a, lat_b) = (
        std::f64::consts::FRAC_PI_2 - a.0,
        std::f64::consts::FRAC_PI_2 - b.0,
    );
    let half_lat = (lat_b - lat_a) / 2.0;
    let half_lon = (b.1 - a.1) / 2.0;
    let h = half_lat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_lon.sin().powi(2);
    2.0 * h.sqrt().min(1.0).asin()
}

/// Most heads one extractor control unit can carry.
pub const MAX_HEADS_PER_ECU: u32 = 10;

/// What a colony builds besides its command center.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColonyLayout {
    pub ecus: u32,
    pub heads: u32,
    /// Factory structure type id → count.
    pub processors: BTreeMap<i64, u32>,
    pub launchpads: u32,
    pub extra_storage: u32,
}

impl ColonyLayout {
    /// Links to the hub: one per structure other than the hub itself.
    fn links(&self) -> u32 {
        (self.ecus + self.launchpads + self.extra_storage + self.processors.values().sum::<u32>())
            .saturating_sub(1)
    }
}

/// One planet type's structure costs at one command center level.
#[derive(Debug, Clone, PartialEq)]
pub struct ColonyBudget {
    cpu: f64,
    power: f64,
    /// A link's fixed `(cpu, power)` and what each km adds.
    link: (f64, f64),
    link_per_km: (f64, f64),
    /// `link_km[n]`: the total length of the links to the hub's `n` nearest
    /// slots, as a colony template lays them out.
    link_km: Vec<f64>,
    ecu: (f64, f64),
    head: (f64, f64),
    launchpad: (f64, f64),
    storage: (f64, f64),
}

fn load(catalogue: &PiCatalogue, kind: PlanetPinKind, planet_type_id: i64) -> Option<(f64, f64)> {
    catalogue
        .pin_on(kind, planet_type_id)
        .map(|pin| (decimal_f64(pin.cpu_load), decimal_f64(pin.power_load)))
}

impl ColonyBudget {
    /// `None` when the planet type can't be colonised.
    #[must_use]
    pub fn new(
        catalogue: &PiCatalogue,
        planet_type_id: i64,
        radius_m: Option<i64>,
        command_center_level: u8,
    ) -> Option<Self> {
        if !catalogue.can_colonise(planet_type_id) {
            return None;
        }
        let ecu = catalogue.pin_on(PlanetPinKind::ExtractorControlUnit, planet_type_id);
        let (cpu, power) = command_center_output(command_center_level);
        let (link_cpu, link_power, cpu_per_km, power_per_km) = catalogue.link_cost();
        let radius_km = radius_m.map_or(DEFAULT_RADIUS_KM, |radius| radius as f64 / 1_000.0);
        let mut link_km = vec![0.0];
        let mut total = 0.0;
        for slot in ring_slots().take(MAX_LINKS) {
            total += angle_between(slot_position(slot), HUB) * radius_km;
            link_km.push(total);
        }
        Some(Self {
            cpu,
            power,
            link: (link_cpu, link_power),
            link_per_km: (cpu_per_km, power_per_km),
            link_km,
            ecu: ecu.map_or((0.0, 0.0), |pin| {
                (decimal_f64(pin.cpu_load), decimal_f64(pin.power_load))
            }),
            head: ecu.map_or((0.0, 0.0), |pin| {
                (
                    decimal_f64(pin.ecu_head_cpu),
                    decimal_f64(pin.ecu_head_power),
                )
            }),
            launchpad: load(catalogue, PlanetPinKind::Launchpad, planet_type_id)
                .unwrap_or_default(),
            storage: load(catalogue, PlanetPinKind::Storage, planet_type_id).unwrap_or_default(),
        })
    }

    /// The command center's `(cpu, power)` output.
    #[must_use]
    pub fn output(&self) -> (f64, f64) {
        (self.cpu, self.power)
    }

    /// The `(cpu, power)` of `links` links to the hub's nearest slots.
    fn links_load(&self, links: u32) -> (f64, f64) {
        let count = (links as usize).min(self.link_km.len() - 1);
        let km = self.link_km[count];
        let links = f64::from(links);
        (
            self.link.0 * links + self.link_per_km.0 * km,
            self.link.1 * links + self.link_per_km.1 * km,
        )
    }

    /// One link's `(cpu, power)` on a colony of `structures` linked
    /// structures, averaged over them. It grows with the colony, since later
    /// structures sit further from the hub.
    #[must_use]
    pub fn link_share(&self, structures: u32) -> (f64, f64) {
        let structures = structures.max(1);
        let (cpu, power) = self.links_load(structures);
        (cpu / f64::from(structures), power / f64::from(structures))
    }

    /// The launchpad's and a storage facility's own `(cpu, power)`.
    #[must_use]
    pub fn launchpad_load(&self) -> (f64, f64) {
        self.launchpad
    }

    #[must_use]
    pub fn storage_load(&self) -> (f64, f64) {
        self.storage
    }

    /// The layout's `(cpu, power)` load, with the links a colony template
    /// draws for it.
    #[must_use]
    pub fn usage(&self, catalogue: &PiCatalogue, layout: &ColonyLayout) -> (f64, f64) {
        let scaled = |(cpu, power): (f64, f64), count: u32| {
            (cpu * f64::from(count), power * f64::from(count))
        };
        let mut parts = vec![
            scaled(self.ecu, layout.ecus),
            scaled(self.head, layout.heads),
            scaled(self.launchpad, layout.launchpads),
            scaled(self.storage, layout.extra_storage),
            self.links_load(layout.links()),
        ];
        for (pin_type_id, count) in &layout.processors {
            let pin = catalogue.pin(*pin_type_id);
            let each = pin.map_or((0.0, 0.0), |pin| {
                (decimal_f64(pin.cpu_load), decimal_f64(pin.power_load))
            });
            parts.push(scaled(each, *count));
        }
        parts
            .into_iter()
            .fold((0.0, 0.0), |(cpu, power), (more_cpu, more_power)| {
                (cpu + more_cpu, power + more_power)
            })
    }

    #[must_use]
    pub fn fits(&self, catalogue: &PiCatalogue, layout: &ColonyLayout) -> bool {
        let (cpu, power) = self.usage(catalogue, layout);
        cpu <= self.cpu && power <= self.power
    }

    /// Most heads `layout`'s ECUs can carry while the colony still fits; 0
    /// when even the layout without heads doesn't fit.
    #[must_use]
    pub fn max_heads(&self, catalogue: &PiCatalogue, layout: &ColonyLayout) -> u32 {
        (0..=layout.ecus * MAX_HEADS_PER_ECU)
            .rev()
            .find(|heads| {
                self.fits(
                    catalogue,
                    &ColonyLayout {
                        heads: *heads,
                        ..layout.clone()
                    },
                )
            })
            .unwrap_or(0)
    }

    /// Most factories of `pin_type_id` that fit next to `layout`.
    #[must_use]
    pub fn max_processors(
        &self,
        catalogue: &PiCatalogue,
        layout: &ColonyLayout,
        pin_type_id: i64,
    ) -> u32 {
        let with = |count: u32| {
            let mut layout = layout.clone();
            *layout.processors.entry(pin_type_id).or_default() += count;
            layout
        };
        let mut count = 0;
        while count < 200 && self.fits(catalogue, &with(count + 1)) {
            count += 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    const RADIUS: Option<i64> = Some(5_060_000);

    fn extraction(ecus: u32, heads: u32) -> ColonyLayout {
        ColonyLayout {
            ecus,
            heads,
            launchpads: 1,
            ..ColonyLayout::default()
        }
    }

    #[test]
    fn an_unupgraded_command_center_cannot_run_two_full_extractors() {
        let catalogue = catalogue();
        let budget = ColonyBudget::new(&catalogue, BARREN, RADIUS, 0).unwrap();
        assert!(!budget.fits(&catalogue, &extraction(2, 20)));
        // Power binds: 2 600 + 6 × 550 = 5 900 of 6 000 MW.
        let one_ecu = ColonyLayout {
            ecus: 1,
            ..ColonyLayout::default()
        };
        assert_eq!(budget.max_heads(&catalogue, &one_ecu), 6);
    }

    #[test]
    fn a_fully_upgraded_command_center_runs_two_full_extractors_and_a_launchpad() {
        let catalogue = catalogue();
        let budget = ColonyBudget::new(&catalogue, BARREN, RADIUS, 5).unwrap();
        assert!(budget.fits(&catalogue, &extraction(2, 20)));
        assert_eq!(budget.max_heads(&catalogue, &extraction(2, 0)), 20);
    }

    #[test]
    fn capacity_grows_with_the_upgrade_level() {
        let catalogue = catalogue();
        let factories = |level: u8| {
            ColonyBudget::new(&catalogue, BARREN, RADIUS, level)
                .unwrap()
                .max_processors(&catalogue, &extraction(0, 0), 2474)
        };
        let counts = (0..=5).map(factories).collect::<Vec<_>>();
        assert!(
            counts.windows(2).all(|pair| pair[0] <= pair[1]),
            "{counts:?}"
        );
        assert!(counts[5] > counts[3]);
        let heads = |level: u8| {
            ColonyBudget::new(&catalogue, BARREN, RADIUS, level)
                .unwrap()
                .max_heads(&catalogue, &extraction(2, 0))
        };
        assert!(heads(3) < heads(5));
    }

    #[test]
    fn storage_and_a_bigger_planet_never_add_room() {
        let catalogue = catalogue();
        let small = ColonyBudget::new(&catalogue, BARREN, Some(2_000_000), 4).unwrap();
        let large = ColonyBudget::new(&catalogue, BARREN, Some(20_000_000), 4).unwrap();
        let layout = extraction(2, 0);
        assert!(large.max_heads(&catalogue, &layout) <= small.max_heads(&catalogue, &layout));
        let with_storage = ColonyLayout {
            extra_storage: 1,
            ..layout.clone()
        };
        assert!(small.max_heads(&catalogue, &with_storage) <= small.max_heads(&catalogue, &layout));
    }

    #[test]
    fn links_grow_with_the_colony_as_structures_sit_further_out() {
        let catalogue = catalogue();
        let budget = ColonyBudget::new(&catalogue, BARREN, RADIUS, 5).unwrap();
        let factories = |count: u32| ColonyLayout {
            launchpads: 1,
            processors: BTreeMap::from([(2474, count)]),
            ..ColonyLayout::default()
        };
        let load = |count: u32| budget.usage(&catalogue, &factories(count)).0;
        // The hub has no link; the first ring's six links are about one
        // length.
        let first = load(1) - load(0);
        assert!((load(6) - load(5) - first).abs() < 0.01 * first);
        // The second ring is further out, so its links cost more.
        assert!(load(7) - load(6) > first + 1e-6);
        // Averaged over a colony, a link costs more the bigger the colony.
        assert!(budget.link_share(30).0 > budget.link_share(6).0);
    }

    #[test]
    fn planets_without_a_command_center_have_no_budget() {
        assert!(ColonyBudget::new(&catalogue(), SHATTERED, RADIUS, 5).is_none());
    }
}
