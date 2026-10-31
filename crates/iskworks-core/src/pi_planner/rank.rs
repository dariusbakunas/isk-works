//! Ranks every PI product the planets in range can support, by ISK/month of
//! one reference chain per character budget. No optimiser: each raw resource
//! comes from its best planet, refined to P1 where it is extracted, and one
//! factory colony builds everything above P1.

use std::collections::BTreeMap;

use super::capacity::{ColonyBudget, ColonyLayout, MAX_HEADS_PER_ECU};
use super::catalogue::PiCatalogue;
use super::chain::expand;
use super::effort::{
    buffers, effort, storage_needed, ColonyVisit, Effort, EffortSettings, INTERVAL_DAYS,
};
use super::yields::{resolve_yield, DensityEntry, SecurityBand, YieldSource};

const HOURS_PER_MONTH: f64 = 720.0;
/// One colony runs at most two extractor control units.
const MAX_ECUS_PER_COLONY: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannerCharacter {
    /// Interplanetary Consolidation: colonies beyond the first.
    pub interplanetary_consolidation: u8,
    /// Command Center Upgrades: the command center level.
    pub command_center_upgrades: u8,
}

impl PlannerCharacter {
    #[must_use]
    pub fn colonies(&self) -> u32 {
        1 + u32::from(self.interplanetary_consolidation.min(5))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankScenario {
    pub characters: Vec<PlannerCharacter>,
    pub effort: EffortSettings,
    /// Customs rate for planets without an override (0.1 = 10 %).
    pub default_customs_rate: f64,
    /// A P1 no planet in range yields may be bought at best sell.
    pub buy_intermediates: bool,
    pub max_colonies: Option<u32>,
}

impl RankScenario {
    #[must_use]
    pub fn available_colonies(&self) -> u32 {
        let total = self.characters.iter().map(PlannerCharacter::colonies).sum();
        self.max_colonies.map_or(total, |cap| cap.min(total))
    }

    fn command_center_level(&self) -> u8 {
        self.characters
            .iter()
            .map(|character| character.command_center_upgrades)
            .max()
            .unwrap_or(0)
    }
}

/// A planet in range with what the player entered for it.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidatePlanet {
    pub planet_id: i64,
    pub type_id: i64,
    pub radius_m: Option<i64>,
    pub jumps: u8,
    pub band: SecurityBand,
    pub customs_rate: Option<f64>,
    /// Other players extract here, so density readings are unreliable.
    pub contested: bool,
    /// By raw resource type id.
    pub densities: BTreeMap<i64, DensityEntry>,
}

/// Unit prices by type id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prices {
    /// What a product sells for (instant sell).
    pub best_buy: BTreeMap<i64, f64>,
    /// What a bought input costs (instant buy).
    pub best_sell: BTreeMap<i64, f64>,
}

/// Where one raw resource of a ranked chain comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedInput {
    pub resource_type_id: i64,
    pub planet_id: i64,
    pub source: YieldSource,
    pub contested: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedProduct {
    pub type_id: i64,
    pub tier: u8,
    pub isk_per_month: f64,
    /// Product per hour across all chains.
    pub units_per_hour: f64,
    /// Colonies across all chains.
    pub colonies: u32,
    pub chains: u32,
    pub inputs: Vec<RankedInput>,
    /// P1 bought instead of extracted.
    pub bought: Vec<i64>,
    /// 0–1: how much of the chain rests on measured head values (1) or
    /// scanned densities (0.5) rather than defaults (0).
    pub confidence: f64,
    pub effort: Effort,
    /// The chain only fits its skyhook buffers when hauled this often,
    /// sooner than the scenario's haul interval.
    pub haul_constrained_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfReach {
    /// No planet in range yields this raw resource.
    MissingResource {
        type_id: i64,
    },
    /// No planet in range has the factories the product needs.
    NoFactoryPlanet,
    /// The chain needs more colonies than the characters have.
    NeedsColonies {
        needed: u32,
        available: u32,
    },
    /// Even daily hauls overflow a skyhook.
    OverflowsSkyhook,
    /// A colony can't fit even one head or factory.
    NoRoom,
    NoPrice,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ranking {
    /// Highest ISK/month first.
    pub ranked: Vec<RankedProduct>,
    /// By type id.
    pub out_of_reach: Vec<(i64, u8, OutOfReach)>,
}

/// An extraction colony of a reference chain: the P1 it refines and how
/// much, per hour.
struct ExtractionColony<'a> {
    planet: &'a CandidatePlanet,
    output: Vec<(i64, f64)>,
}

/// Ranks every recipe output (P1–P4).
#[must_use]
pub fn rank(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    prices: &Prices,
) -> Ranking {
    let mut ranking = Ranking {
        ranked: Vec::new(),
        out_of_reach: Vec::new(),
    };
    for recipe in catalogue.recipes() {
        match rank_product(catalogue, scenario, planets, prices, recipe.output_type_id) {
            Ok(product) => ranking.ranked.push(product),
            Err(reason) => ranking
                .out_of_reach
                .push((recipe.output_type_id, recipe.tier, reason)),
        }
    }
    ranking.ranked.sort_by(|a, b| {
        b.isk_per_month
            .total_cmp(&a.isk_per_month)
            .then(a.type_id.cmp(&b.type_id))
    });
    ranking
}

fn customs_rate(scenario: &RankScenario, planet: &CandidatePlanet) -> f64 {
    planet.customs_rate.unwrap_or(scenario.default_customs_rate)
}

fn rank_product(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    prices: &Prices,
    product: i64,
) -> Result<RankedProduct, OutOfReach> {
    let product_price = *prices.best_buy.get(&product).ok_or(OutOfReach::NoPrice)?;
    let tier = catalogue.tier(product).unwrap_or(0);
    let unit_chain = expand(catalogue, product, 1.0).ok_or(OutOfReach::NoPrice)?;
    let program_hours = f64::from(scenario.effort.reset_days.max(1)) * 24.0;
    let level = scenario.command_center_level();

    // The best planet for each P1's raw resource.
    let needed_p1 = unit_chain
        .per_hour
        .keys()
        .copied()
        .filter(|type_id| catalogue.tier(*type_id) == Some(1))
        .collect::<Vec<_>>();
    let mut sources: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut bought = Vec::new();
    for p1 in &needed_p1 {
        let Some(resource) = catalogue
            .recipe(*p1)
            .and_then(|recipe| recipe.inputs.first().map(|(input, _)| *input))
        else {
            return Err(OutOfReach::NoPrice);
        };
        let best = planets
            .iter()
            .filter(|planet| catalogue.can_colonise(planet.type_id))
            .filter(|planet| catalogue.resources(planet.type_id).any(|r| r == resource))
            .map(|planet| {
                let estimate =
                    resolve_yield(planet.densities.get(&resource), planet.band, program_hours);
                (planet, estimate)
            })
            .max_by(|(a, a_yield), (b, b_yield)| {
                a_yield
                    .units_per_hour_per_head
                    .total_cmp(&b_yield.units_per_hour_per_head)
                    .then(b.jumps.cmp(&a.jumps))
                    .then(b.planet_id.cmp(&a.planet_id))
            });
        match best {
            Some((planet, estimate)) => {
                sources
                    .entry(planet.planet_id)
                    .or_default()
                    .push((*p1, resource));
                inputs.push(RankedInput {
                    resource_type_id: resource,
                    planet_id: planet.planet_id,
                    source: estimate.source,
                    contested: planet.contested,
                });
            }
            None if scenario.buy_intermediates && prices.best_sell.contains_key(p1) => {
                bought.push(*p1)
            }
            None => return Err(OutOfReach::MissingResource { type_id: resource }),
        }
    }
    if inputs.is_empty() {
        // Nothing extracted: buying every input isn't PI.
        return Err(OutOfReach::MissingResource {
            type_id: unit_chain
                .raw_resources(catalogue)
                .keys()
                .next()
                .copied()
                .unwrap_or(product),
        });
    }

    // Size the extraction colonies, at most two resources each.
    let mut extraction = Vec::new();
    for (planet_id, outputs) in &sources {
        let planet = planets
            .iter()
            .find(|planet| planet.planet_id == *planet_id)
            .ok_or(OutOfReach::NoRoom)?;
        for group in outputs.chunks(MAX_ECUS_PER_COLONY) {
            extraction.push(size_extraction(
                catalogue,
                scenario,
                planet,
                group,
                level,
                program_hours,
            )?);
        }
    }

    // How much product the extracted P1 supports.
    let mut supply: BTreeMap<i64, f64> = BTreeMap::new();
    for colony in &extraction {
        for (p1, rate) in &colony.output {
            *supply.entry(*p1).or_default() += rate;
        }
    }
    let mut rate = needed_p1
        .iter()
        .filter_map(|p1| supply.get(p1).map(|have| have / unit_chain.per_hour[p1]))
        .fold(f64::INFINITY, f64::min);

    // One factory colony builds everything above P1.
    let factory = if tier >= 2 {
        let (planet, fitted_rate) =
            place_factory(catalogue, scenario, planets, product, rate, level)?;
        rate = fitted_rate;
        Some(planet)
    } else {
        None
    };
    if !(rate.is_finite() && rate > 0.0) {
        return Err(OutOfReach::NoRoom);
    }

    let per_chain = extraction.len() as u32 + u32::from(factory.is_some());
    let available = scenario.available_colonies();
    if per_chain > available {
        return Err(OutOfReach::NeedsColonies {
            needed: per_chain,
            available,
        });
    }
    let chains = (available / per_chain).max(1);

    // ISK per hour of one chain.
    let used = expand(catalogue, product, rate).ok_or(OutOfReach::NoPrice)?;
    let mut net = rate * product_price;
    let factory_rate = factory.map(|planet| customs_rate(scenario, planet));
    for colony in &extraction {
        let export_rate = customs_rate(scenario, colony.planet);
        for (p1, made) in &colony.output {
            if tier == 1 {
                net -= made * catalogue.export_tax(*p1, export_rate);
                continue;
            }
            // What the factory needs moves there; any surplus is sold.
            let share = made / supply[p1];
            let to_factory = (used.per_hour.get(p1).copied().unwrap_or(0.0) * share).min(*made);
            let surplus = made - to_factory;
            net -= made * catalogue.export_tax(*p1, export_rate);
            net -= to_factory * catalogue.import_tax(*p1, factory_rate.unwrap_or(export_rate));
            net += surplus * prices.best_buy.get(p1).copied().unwrap_or(0.0);
        }
    }
    for p1 in &bought {
        let needed = used.per_hour.get(p1).copied().unwrap_or(0.0);
        net -= needed * prices.best_sell[p1];
        net -= needed * catalogue.import_tax(*p1, factory_rate.unwrap_or(0.0));
    }
    if let Some(rate_at_factory) = factory_rate {
        net -= rate * catalogue.export_tax(product, rate_at_factory);
    }

    // Effort across every chain, colonies dealt to characters in turn.
    let mut visits = Vec::new();
    let mut slots = scenario
        .characters
        .iter()
        .enumerate()
        .flat_map(|(index, character)| {
            std::iter::repeat(index).take(character.colonies() as usize)
        });
    for _ in 0..chains {
        for colony in &extraction {
            visits.push(ColonyVisit {
                character: slots.next().unwrap_or(0),
                extracts: true,
                m3_per_hour: colony
                    .output
                    .iter()
                    .map(|(p1, made)| made * catalogue.volume(*p1))
                    .sum(),
            });
        }
        if factory.is_some() {
            let inputs_m3 = used
                .per_hour
                .iter()
                .filter(|(type_id, _)| catalogue.tier(**type_id) == Some(1))
                .map(|(type_id, needed)| needed * catalogue.volume(*type_id))
                .sum::<f64>();
            visits.push(ColonyVisit {
                character: slots.next().unwrap_or(0),
                extracts: false,
                m3_per_hour: inputs_m3 + rate * catalogue.volume(product),
            });
        }
    }
    let fits = |haul_days: u32| {
        let settings = EffortSettings {
            haul_days,
            ..scenario.effort
        };
        visits.iter().all(|visit| {
            buffers(
                visit.m3_per_hour,
                settings.login_days(visit.extracts),
                settings,
            )
            .skyhook_fits()
        })
    };
    let haul_constrained_days = if fits(scenario.effort.haul_days) {
        None
    } else {
        Some(
            INTERVAL_DAYS
                .iter()
                .rev()
                .copied()
                .filter(|days| *days < scenario.effort.haul_days)
                .find(|days| fits(*days))
                .ok_or(OutOfReach::OverflowsSkyhook)?,
        )
    };
    let effort = effort(
        &visits,
        scenario.characters.len(),
        EffortSettings {
            haul_days: haul_constrained_days.unwrap_or(scenario.effort.haul_days),
            ..scenario.effort
        },
    );

    let weights = inputs
        .iter()
        .map(|input| match input.source {
            YieldSource::HeadValue => 1.0,
            YieldSource::Scanned if !input.contested => 0.5,
            _ => 0.0,
        })
        .chain(bought.iter().map(|_| 1.0))
        .collect::<Vec<f64>>();

    Ok(RankedProduct {
        type_id: product,
        tier,
        isk_per_month: net * HOURS_PER_MONTH * f64::from(chains),
        units_per_hour: rate * f64::from(chains),
        colonies: per_chain * chains,
        chains,
        inputs,
        bought,
        confidence: weights.iter().sum::<f64>() / weights.len() as f64,
        effort,
        haul_constrained_days,
    })
}

/// The most heads (split evenly across one ECU per resource) the colony fits
/// with basic factories refining everything and storage for its buffers.
fn size_extraction<'a>(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planet: &'a CandidatePlanet,
    group: &[(i64, i64)],
    level: u8,
    program_hours: f64,
) -> Result<ExtractionColony<'a>, OutOfReach> {
    let budget = ColonyBudget::new(catalogue, planet.type_id, planet.radius_m, level)
        .ok_or(OutOfReach::NoRoom)?;
    let ecus = group.len() as u32;
    let per_head = group
        .iter()
        .map(|(_, resource)| {
            resolve_yield(planet.densities.get(resource), planet.band, program_hours)
                .units_per_hour_per_head
        })
        .collect::<Vec<_>>();
    for heads in (1..=ecus * MAX_HEADS_PER_ECU).rev() {
        let heads_each = f64::from(heads) / f64::from(ecus);
        let output = group
            .iter()
            .zip(&per_head)
            .map(|((p1, _), yield_per_head)| {
                let recipe = catalogue.recipe(*p1);
                let ratio = recipe.map_or(0.0, |recipe| {
                    recipe.output_quantity
                        / recipe.inputs.first().map_or(f64::INFINITY, |(_, q)| *q)
                });
                (*p1, heads_each * yield_per_head * ratio)
            })
            .collect::<Vec<_>>();
        let mut layout = ColonyLayout {
            ecus,
            heads,
            launchpads: 1,
            ..ColonyLayout::default()
        };
        for ((p1, _), (_, made)) in group.iter().zip(&output) {
            let recipe = catalogue.recipe(*p1).ok_or(OutOfReach::NoRoom)?;
            let pin = catalogue
                .processor_for(recipe, planet.type_id)
                .ok_or(OutOfReach::NoRoom)?;
            let factories = (made / recipe.output_per_hour()).ceil() as u32;
            *layout.processors.entry(pin.type_id).or_default() += factories;
        }
        let m3_per_hour = output
            .iter()
            .map(|(p1, made)| made * catalogue.volume(*p1))
            .sum::<f64>();
        let held = buffers(
            m3_per_hour,
            scenario.effort.login_days(true),
            scenario.effort,
        );
        layout.extra_storage = storage_needed(held.on_planet_m3);
        if budget.fits(catalogue, &layout) {
            return Ok(ExtractionColony { planet, output });
        }
    }
    Err(OutOfReach::NoRoom)
}

/// The nearest planet that can build every recipe above P1, and the highest
/// product rate up to `rate` its factory colony fits.
fn place_factory<'a>(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &'a [CandidatePlanet],
    product: i64,
    rate: f64,
    level: u8,
) -> Result<(&'a CandidatePlanet, f64), OutOfReach> {
    let upper = expand(catalogue, product, 1.0).ok_or(OutOfReach::NoPrice)?;
    let recipes = upper
        .factories
        .keys()
        .filter_map(|output| catalogue.recipe(*output))
        .filter(|recipe| recipe.tier >= 2)
        .collect::<Vec<_>>();
    let mut candidates = planets
        .iter()
        .filter(|planet| {
            recipes
                .iter()
                .all(|recipe| catalogue.processor_for(recipe, planet.type_id).is_some())
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|planet| (planet.jumps, planet.planet_id));
    let planet = *candidates.first().ok_or(OutOfReach::NoFactoryPlanet)?;
    let budget = ColonyBudget::new(catalogue, planet.type_id, planet.radius_m, level)
        .ok_or(OutOfReach::NoRoom)?;
    let fits = |rate: f64| {
        let Some(chain) = expand(catalogue, product, rate) else {
            return false;
        };
        let mut layout = ColonyLayout {
            launchpads: 1,
            ..ColonyLayout::default()
        };
        let mut m3_per_hour = rate * catalogue.volume(product);
        for (output, count) in chain.factory_counts() {
            let Some(recipe) = catalogue.recipe(output) else {
                return false;
            };
            if recipe.tier < 2 {
                m3_per_hour += chain.per_hour[&output] * catalogue.volume(output);
                continue;
            }
            let Some(pin) = catalogue.processor_for(recipe, planet.type_id) else {
                return false;
            };
            *layout.processors.entry(pin.type_id).or_default() += count;
        }
        let held = buffers(
            m3_per_hour,
            scenario.effort.login_days(false),
            scenario.effort,
        );
        layout.extra_storage = storage_needed(held.on_planet_m3);
        budget.fits(catalogue, &layout)
    };
    if fits(rate) {
        return Ok((planet, rate));
    }
    let (mut low, mut high) = (0.0, rate);
    for _ in 0..40 {
        let mid = (low + high) / 2.0;
        if fits(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    if low <= 0.0 {
        return Err(OutOfReach::NoRoom);
    }
    Ok((planet, low))
}

#[cfg(test)]
mod tests;
