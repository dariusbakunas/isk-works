//! The optimiser: which planets each character colonises, in which role,
//! and how goods move between colonies, for the most ISK/month after customs
//! tax.
//!
//! A mixed-integer program over precomputed colony patterns:
//! - an extraction pattern is one character group's colony on one planet
//!   extracting one raw resource (one or two ECUs) and refining it to P1,
//!   sized for capacity and buffers like the ranking does;
//! - a factory site is a colony on one planet with its CPU and power left
//!   for processors after a launchpad.
//!
//! Characters with the same skills form a group, so the solver never
//! branches on which of two identical characters gets a colony. Factory
//! capacity is pooled across sites; the plan then deals recipes out to the
//! chosen sites and prices every transfer at its own planet's tax.

use std::collections::{BTreeMap, BTreeSet};

use good_lp::{
    default_solver, variable, Expression, ProblemVariables, Solution, SolverModel, Variable,
};

use super::capacity::{ColonyBudget, ColonyLayout};
use super::catalogue::{PiCatalogue, Recipe};
use super::effort::{buffers, effort, ColonyVisit, Effort, LAUNCHPAD_M3, SKYHOOK_M3};
use super::rank::{
    customs_rate, size_extraction, CandidatePlanet, Prices, RankScenario, RankedInput,
    HOURS_PER_MONTH,
};
use super::yields::resolve_yield;

/// Extraction planets kept per raw resource and group, best output first.
const EXTRACTION_CANDIDATES: usize = 5;
/// Factory planets kept per group, lowest tax and nearest first.
const FACTORY_CANDIDATES: usize = 6;
/// ISK/hour charged per colony, so the solver leaves idle colonies out.
const COLONY_PENALTY: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColonyRole {
    /// Extracts one raw resource and refines it to P1.
    Extraction,
    /// Builds P2 and above from imports.
    Factory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedColony {
    pub character: usize,
    pub planet_id: i64,
    pub role: ColonyRole,
    pub layout: ColonyLayout,
    /// Raw resources extracted, units per hour each (one or two).
    pub extracts: Vec<(i64, f64)>,
    /// Units per hour by type id, crossing the customs office.
    pub imports: BTreeMap<i64, f64>,
    pub exports: BTreeMap<i64, f64>,
    /// Factories by output type id; fractional when shared.
    pub recipes: BTreeMap<i64, f64>,
    pub tax_per_month: f64,
    /// Share of the command center's CPU and power in use.
    pub cpu_used: f64,
    pub power_used: f64,
    /// m³ held on the planet between logins, and at the skyhook between hauls.
    pub on_planet_m3: f64,
    pub skyhook_m3: f64,
}

/// Where a flow starts or ends: a colony (by index) or the market.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowEnd {
    Colony(usize),
    Market,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFlow {
    pub from: FlowEnd,
    pub to: FlowEnd,
    pub type_id: i64,
    pub units_per_hour: f64,
    pub tax_per_month: f64,
    /// Between colonies of different characters, so it needs hauling
    /// between them.
    pub cross_character: bool,
}

/// Two or more characters extract the same resource on one planet: real
/// yields drop, since heads on the same hotspot deplete it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedHotspot {
    pub planet_id: i64,
    pub resource_type_id: i64,
    pub characters: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PiPlan {
    pub isk_per_month: f64,
    pub sales_per_month: f64,
    pub purchases_per_month: f64,
    pub customs_tax_per_month: f64,
    pub colonies: Vec<PlannedColony>,
    pub flows: Vec<PlannedFlow>,
    pub effort: Effort,
    /// Each (planet, resource) the plan extracts, with its reading's source.
    pub inputs: Vec<RankedInput>,
    pub shared_hotspots: Vec<SharedHotspot>,
}

impl PiPlan {
    #[must_use]
    pub fn colonies_used(&self) -> u32 {
        self.colonies.len() as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SolveError {
    #[error("the PI plan solver failed: {0}")]
    Solver(String),
}

/// Characters with the same skill levels, by index into the scenario.
struct Group {
    characters: Vec<usize>,
    colonies_each: u32,
    level: u8,
}

fn groups(scenario: &RankScenario) -> Vec<Group> {
    let mut by_skills: BTreeMap<(u8, u8), Vec<usize>> = BTreeMap::new();
    for (index, character) in scenario.characters.iter().enumerate() {
        by_skills
            .entry((
                character.interplanetary_consolidation.min(5),
                character.command_center_upgrades.min(5),
            ))
            .or_default()
            .push(index);
    }
    by_skills
        .into_iter()
        .map(|((ic, ccu), characters)| Group {
            characters,
            colonies_each: 1 + u32::from(ic),
            level: ccu,
        })
        .collect()
}

/// One raw resource of an extraction pattern and the P1 it refines into.
struct Extracted {
    resource: i64,
    p1: i64,
    /// P1 per hour of one colony.
    rate: f64,
}

struct ExtractionPattern {
    group: usize,
    planet: usize,
    /// One resource (on one or two ECUs) or two (one ECU each).
    outputs: Vec<Extracted>,
    layout: ColonyLayout,
}

impl ExtractionPattern {
    fn total(&self) -> f64 {
        self.outputs.iter().map(|output| output.rate).sum()
    }
}

struct FactorySite {
    group: usize,
    planet: usize,
    budget: ColonyBudget,
    /// CPU and power left after the launchpad.
    room: (f64, f64),
    high_tech: bool,
}

/// The P1 each raw resource refines into.
fn p1_by_resource(catalogue: &PiCatalogue) -> BTreeMap<i64, i64> {
    catalogue
        .recipes()
        .filter(|recipe| recipe.tier == 1)
        .filter_map(|recipe| {
            recipe
                .inputs
                .first()
                .map(|(resource, _)| (*resource, recipe.output_type_id))
        })
        .collect()
}

/// Sizes one colony extracting `spec` (P1, resource) per ECU; `None` when
/// it doesn't fit or its output overflows the skyhook.
fn sized(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planet: &CandidatePlanet,
    spec: &[(i64, i64)],
    level: u8,
    program_hours: f64,
) -> Option<(Vec<Extracted>, ColonyLayout)> {
    let colony = size_extraction(catalogue, scenario, planet, spec, level, program_hours).ok()?;
    let mut outputs: Vec<Extracted> = Vec::new();
    for ((p1, resource), (_, made)) in spec.iter().zip(&colony.output) {
        match outputs.iter_mut().find(|output| output.p1 == *p1) {
            Some(output) => output.rate += made,
            None => outputs.push(Extracted {
                resource: *resource,
                p1: *p1,
                rate: *made,
            }),
        }
    }
    let m3 = outputs
        .iter()
        .map(|output| output.rate * catalogue.volume(output.p1))
        .sum::<f64>();
    let fits = buffers(m3, scenario.effort.login_days(true), scenario.effort).skyhook_fits();
    (fits && outputs.iter().all(|output| output.rate > 0.0)).then_some((outputs, colony.layout))
}

fn extraction_patterns(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    groups: &[Group],
) -> Vec<ExtractionPattern> {
    let program_hours = f64::from(scenario.effort.reset_days.max(1)) * 24.0;
    let p1s = p1_by_resource(catalogue);
    let mut patterns = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        let mut singles: BTreeMap<i64, Vec<ExtractionPattern>> = BTreeMap::new();
        let mut pairs = Vec::new();
        for (planet_index, planet) in planets.iter().enumerate() {
            if !catalogue.can_colonise(planet.type_id) {
                continue;
            }
            let available = catalogue
                .resources(planet.type_id)
                .filter_map(|resource| p1s.get(&resource).map(|p1| (*p1, resource)))
                .collect::<Vec<_>>();
            let pattern = |(outputs, layout): (Vec<Extracted>, ColonyLayout)| ExtractionPattern {
                group: group_index,
                planet: planet_index,
                outputs,
                layout,
            };
            for one in &available {
                // One ECU, or two on the same resource when that fits more.
                let best = [vec![*one], vec![*one, *one]]
                    .iter()
                    .filter_map(|spec| {
                        sized(
                            catalogue,
                            scenario,
                            planet,
                            spec,
                            group.level,
                            program_hours,
                        )
                    })
                    .map(pattern)
                    .max_by(|a, b| a.total().total_cmp(&b.total()));
                if let Some(best) = best {
                    singles.entry(one.1).or_default().push(best);
                }
            }
            for (index, first) in available.iter().enumerate() {
                for second in &available[index + 1..] {
                    if let Some(sized) = sized(
                        catalogue,
                        scenario,
                        planet,
                        &[*first, *second],
                        group.level,
                        program_hours,
                    ) {
                        pairs.push(pattern(sized));
                    }
                }
            }
        }
        // The best planets per resource, and pairs only on planets that are
        // among the best for both of their resources.
        let mut kept: BTreeMap<i64, BTreeSet<usize>> = BTreeMap::new();
        for (resource, mut candidates) in singles {
            candidates.sort_by(|a, b| {
                b.total()
                    .total_cmp(&a.total())
                    .then(planets[a.planet].jumps.cmp(&planets[b.planet].jumps))
                    .then(
                        planets[a.planet]
                            .planet_id
                            .cmp(&planets[b.planet].planet_id),
                    )
            });
            for candidate in candidates.into_iter().take(EXTRACTION_CANDIDATES) {
                kept.entry(resource).or_default().insert(candidate.planet);
                patterns.push(candidate);
            }
        }
        patterns.extend(pairs.into_iter().filter(|pair| {
            pair.outputs.iter().all(|output| {
                kept.get(&output.resource)
                    .is_some_and(|planets| planets.contains(&pair.planet))
            })
        }));
    }
    patterns
}

/// The CPU and power one more structure of `pin_type_id` costs, link included.
fn marginal_cost(
    catalogue: &PiCatalogue,
    budget: &ColonyBudget,
    base: &ColonyLayout,
    pin_type_id: i64,
) -> (f64, f64) {
    let mut with = base.clone();
    *with.processors.entry(pin_type_id).or_default() += 1;
    let (cpu, power) = budget.usage(catalogue, &with);
    let (base_cpu, base_power) = budget.usage(catalogue, base);
    (cpu - base_cpu, power - base_power)
}

fn factory_base() -> ColonyLayout {
    ColonyLayout {
        launchpads: 1,
        ..ColonyLayout::default()
    }
}

fn factory_sites(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    groups: &[Group],
    upper: &[&Recipe],
) -> Vec<FactorySite> {
    let base = factory_base();
    let mut sites = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        let mut candidates = planets
            .iter()
            .enumerate()
            .filter_map(|(index, planet)| {
                let budget =
                    ColonyBudget::new(catalogue, planet.type_id, planet.radius_m, group.level)?;
                let advanced = upper
                    .iter()
                    .filter(|recipe| recipe.tier < 4)
                    .any(|recipe| catalogue.processor_for(recipe, planet.type_id).is_some());
                if !advanced {
                    return None;
                }
                let (cpu, power) = budget.output();
                let (used_cpu, used_power) = budget.usage(catalogue, &base);
                let high_tech = upper
                    .iter()
                    .filter(|recipe| recipe.tier == 4)
                    .any(|recipe| catalogue.processor_for(recipe, planet.type_id).is_some());
                Some(FactorySite {
                    group: group_index,
                    planet: index,
                    budget,
                    room: (cpu - used_cpu, power - used_power),
                    high_tech,
                })
            })
            .collect::<Vec<_>>();
        let rate = |site: &FactorySite| customs_rate(scenario, &planets[site.planet]);
        candidates.sort_by(|a, b| {
            rate(a)
                .total_cmp(&rate(b))
                .then(planets[a.planet].jumps.cmp(&planets[b.planet].jumps))
                .then(
                    planets[a.planet]
                        .planet_id
                        .cmp(&planets[b.planet].planet_id),
                )
        });
        // The cheapest sites, plus the cheapest high-tech ones for P4.
        let mut kept = BTreeSet::new();
        for (position, site) in candidates.iter().enumerate() {
            if kept.len() < FACTORY_CANDIDATES {
                kept.insert(position);
            }
            if site.high_tech
                && kept
                    .iter()
                    .filter(|kept_position| candidates[**kept_position].high_tech)
                    .count()
                    < 2
            {
                kept.insert(position);
            }
        }
        sites.extend(
            candidates
                .into_iter()
                .enumerate()
                .filter(|(position, _)| kept.contains(position))
                .map(|(_, site)| site),
        );
    }
    sites
}

/// Per factory of `recipe`: the CPU and power it costs (the dearest across
/// the sites that can run it), or `None` when no site can.
fn recipe_cost(
    catalogue: &PiCatalogue,
    planets: &[CandidatePlanet],
    sites: &[FactorySite],
    recipe: &Recipe,
) -> Option<(f64, f64)> {
    let base = factory_base();
    sites
        .iter()
        .filter_map(|site| {
            let pin = catalogue.processor_for(recipe, planets[site.planet].type_id)?;
            Some(marginal_cost(catalogue, &site.budget, &base, pin.type_id))
        })
        .reduce(|a, b| (a.0.max(b.0), a.1.max(b.1)))
}

/// m³ per hour through a factory per unit of output per hour: the output
/// plus its inputs.
fn recipe_m3(catalogue: &PiCatalogue, recipe: &Recipe) -> f64 {
    catalogue.volume(recipe.output_type_id)
        + recipe
            .inputs
            .iter()
            .map(|(input, quantity)| quantity / recipe.output_quantity * catalogue.volume(*input))
            .sum::<f64>()
}

/// Solves the scenario. A plan with no colonies means nothing pays.
pub fn solve(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    prices: &Prices,
) -> Result<PiPlan, SolveError> {
    let groups = groups(scenario);
    let upper = catalogue
        .recipes()
        .filter(|recipe| recipe.tier >= 2)
        .collect::<Vec<_>>();
    let patterns = extraction_patterns(catalogue, scenario, planets, &groups);
    let sites = factory_sites(catalogue, scenario, planets, &groups, &upper);
    let costs: BTreeMap<i64, (f64, f64)> = upper
        .iter()
        .filter_map(|recipe| {
            recipe_cost(catalogue, planets, &sites, recipe)
                .map(|cost| (recipe.output_type_id, cost))
        })
        .collect();
    let upper = upper
        .into_iter()
        .filter(|recipe| costs.contains_key(&recipe.output_type_id))
        .collect::<Vec<_>>();

    let mut vars = ProblemVariables::new();
    let colony_count = |vars: &mut ProblemVariables, group: &Group| {
        vars.add(
            variable()
                .integer()
                .min(0)
                .max(group.characters.len() as f64),
        )
    };
    let x: Vec<Variable> = patterns
        .iter()
        .map(|pattern| colony_count(&mut vars, &groups[pattern.group]))
        .collect();
    let extracted: Vec<Vec<Variable>> = patterns
        .iter()
        .map(|pattern| {
            pattern
                .outputs
                .iter()
                .map(|_| vars.add(variable().min(0)))
                .collect()
        })
        .collect();
    let y: Vec<Variable> = sites
        .iter()
        .map(|site| colony_count(&mut vars, &groups[site.group]))
        .collect();
    let made: BTreeMap<i64, Variable> = upper
        .iter()
        .map(|recipe| (recipe.output_type_id, vars.add(variable().min(0))))
        .collect();
    let traded = catalogue
        .recipes()
        .map(|recipe| recipe.output_type_id)
        .collect::<BTreeSet<_>>();
    let sold: BTreeMap<i64, Variable> = traded
        .iter()
        .filter(|type_id| prices.best_buy.contains_key(type_id))
        .map(|type_id| (*type_id, vars.add(variable().min(0))))
        .collect();
    let bought: BTreeMap<i64, Variable> = if scenario.buy_intermediates {
        traded
            .iter()
            .filter(|type_id| catalogue.tier(**type_id).is_some_and(|tier| tier <= 3))
            .filter(|type_id| prices.best_sell.contains_key(type_id))
            .map(|type_id| (*type_id, vars.add(variable().min(0))))
            .collect()
    } else {
        BTreeMap::new()
    };

    // Objective, per hour, with factory-side tax at the default rate; the
    // plan reprices every transfer at its own planet afterwards.
    let default_rate = scenario.default_customs_rate;
    let mut objective = Expression::from(0.0);
    for (type_id, var) in &sold {
        let mut value = prices.best_buy[type_id];
        if catalogue.tier(*type_id).is_some_and(|tier| tier >= 2) {
            value -= catalogue.export_tax(*type_id, default_rate);
        }
        objective += value * *var;
    }
    for (type_id, var) in &bought {
        objective -=
            (prices.best_sell[type_id] + catalogue.import_tax(*type_id, default_rate)) * *var;
    }
    for (pattern, vars) in patterns.iter().zip(&extracted) {
        let rate = customs_rate(scenario, &planets[pattern.planet]);
        for (output, var) in pattern.outputs.iter().zip(vars) {
            objective -= catalogue.export_tax(output.p1, rate) * *var;
        }
    }
    for recipe in &upper {
        let per_unit_tax = recipe
            .inputs
            .iter()
            .filter(|(input, _)| catalogue.tier(*input) == Some(1))
            .map(|(input, quantity)| {
                quantity / recipe.output_quantity * catalogue.import_tax(*input, default_rate)
            })
            .sum::<f64>();
        objective -= per_unit_tax * made[&recipe.output_type_id];
    }
    for var in x.iter().chain(&y) {
        objective -= COLONY_PENALTY * *var;
    }

    let mut problem = vars.maximise(objective).using(default_solver);

    // Extraction up to the colonies' output.
    for ((pattern, count), vars) in patterns.iter().zip(&x).zip(&extracted) {
        for (output, var) in pattern.outputs.iter().zip(vars) {
            problem = problem.with((Expression::from(*var) - output.rate * *count).leq(0.0));
        }
    }
    // Colonies per group, one per character per planet, and the cap.
    for (group_index, group) in groups.iter().enumerate() {
        let mut total = Expression::from(0.0);
        let mut per_planet: BTreeMap<usize, Expression> = BTreeMap::new();
        for (pattern, var) in patterns.iter().zip(&x) {
            if pattern.group == group_index {
                total += *var;
                *per_planet.entry(pattern.planet).or_default() += *var;
            }
        }
        for (site, var) in sites.iter().zip(&y) {
            if site.group == group_index {
                total += *var;
                *per_planet.entry(site.planet).or_default() += *var;
            }
        }
        let characters = group.characters.len() as f64;
        problem = problem.with(total.leq(characters * f64::from(group.colonies_each)));
        for expression in per_planet.into_values() {
            problem = problem.with(expression.leq(characters));
        }
    }
    if let Some(cap) = scenario.max_colonies {
        let total = x
            .iter()
            .chain(&y)
            .fold(Expression::from(0.0), |sum, var| sum + *var);
        problem = problem.with(total.leq(f64::from(cap)));
    }
    // Recipe balance for every traded commodity.
    for type_id in &traded {
        let mut balance = Expression::from(0.0);
        for (pattern, vars) in patterns.iter().zip(&extracted) {
            for (output, var) in pattern.outputs.iter().zip(vars) {
                if output.p1 == *type_id {
                    balance += *var;
                }
            }
        }
        if let Some(var) = made.get(type_id) {
            balance += *var;
        }
        if let Some(var) = bought.get(type_id) {
            balance += *var;
        }
        for recipe in &upper {
            if let Some((_, quantity)) = recipe.inputs.iter().find(|(input, _)| input == type_id) {
                balance -= quantity / recipe.output_quantity * made[&recipe.output_type_id];
            }
        }
        if let Some(var) = sold.get(type_id) {
            balance -= *var;
        }
        problem = problem.with(balance.eq(0.0));
    }
    // Factory capacity: all upper recipes on every site, P4 on high-tech ones.
    for (pool_high_tech, axis) in [(false, 0), (false, 1), (true, 0), (true, 1)] {
        let mut used = Expression::from(0.0);
        for recipe in &upper {
            if pool_high_tech && recipe.tier < 4 {
                continue;
            }
            let cost = costs[&recipe.output_type_id];
            let per_factory = if axis == 0 { cost.0 } else { cost.1 };
            used += per_factory / recipe.output_per_hour() * made[&recipe.output_type_id];
        }
        let mut room = Expression::from(0.0);
        for (site, var) in sites.iter().zip(&y) {
            if pool_high_tech && !site.high_tech {
                continue;
            }
            room += (if axis == 0 { site.room.0 } else { site.room.1 }) * *var;
        }
        problem = problem.with((used - room).leq(0.0));
    }
    // Factory buffers: the launchpad between logins, the skyhook between hauls.
    let held = buffers(1.0, scenario.effort.login_days(false), scenario.effort);
    let mut through = Expression::from(0.0);
    for recipe in &upper {
        through += recipe_m3(catalogue, recipe) * made[&recipe.output_type_id];
    }
    let factories = y.iter().fold(Expression::from(0.0), |sum, var| sum + *var);
    problem = problem
        .with((held.on_planet_m3 * through.clone() - LAUNCHPAD_M3 * factories.clone()).leq(0.0));
    if held.skyhook_m3 > 0.0 {
        problem = problem.with((held.skyhook_m3 * through - SKYHOOK_M3 * factories).leq(0.0));
    }

    let solution = problem
        .solve()
        .map_err(|error| SolveError::Solver(error.to_string()))?;
    let value = |var: Variable| solution.value(var).max(0.0);
    let count = |var: Variable| value(var).round() as u32;

    Ok(build_plan(
        catalogue,
        scenario,
        planets,
        prices,
        &groups,
        PlanValues {
            extraction: patterns
                .iter()
                .zip(&x)
                .zip(&extracted)
                .filter(|((_, var), _)| count(**var) > 0)
                .map(|((pattern, var), made)| {
                    (
                        pattern,
                        count(*var),
                        made.iter().map(|var| value(*var)).collect(),
                    )
                })
                .collect(),
            factories: sites
                .iter()
                .zip(&y)
                .filter(|(_, var)| count(**var) > 0)
                .map(|(site, var)| (site, count(*var)))
                .collect(),
            made: made
                .iter()
                .map(|(type_id, var)| (*type_id, value(*var)))
                .filter(|(_, rate)| *rate > 1e-6)
                .collect(),
            sold: sold
                .iter()
                .map(|(type_id, var)| (*type_id, value(*var)))
                .filter(|(_, rate)| *rate > 1e-6)
                .collect(),
            bought: bought
                .iter()
                .map(|(type_id, var)| (*type_id, value(*var)))
                .filter(|(_, rate)| *rate > 1e-6)
                .collect(),
            costs,
        },
    ))
}

struct PlanValues<'a> {
    /// Each pattern used, its colony count, and P1 per hour per output.
    extraction: Vec<(&'a ExtractionPattern, u32, Vec<f64>)>,
    factories: Vec<(&'a FactorySite, u32)>,
    made: BTreeMap<i64, f64>,
    sold: BTreeMap<i64, f64>,
    bought: BTreeMap<i64, f64>,
    costs: BTreeMap<i64, (f64, f64)>,
}

/// Deals each group's colonies to its characters: fewest colonies first,
/// never two on one planet.
fn assign(groups: &[Group], wanted: &[(usize, usize)]) -> Vec<usize> {
    let mut held: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    wanted
        .iter()
        .map(|(group, planet)| {
            let characters = &groups[*group].characters;
            let character = characters
                .iter()
                .copied()
                .filter(|character| {
                    !held
                        .get(character)
                        .is_some_and(|planets| planets.contains(planet))
                })
                .min_by_key(|character| held.get(character).map_or(0, Vec::len))
                .unwrap_or(characters[0]);
            held.entry(character).or_default().push(*planet);
            character
        })
        .collect()
}

fn build_plan(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    prices: &Prices,
    groups: &[Group],
    values: PlanValues<'_>,
) -> PiPlan {
    // Concrete colonies.
    let mut wanted = Vec::new();
    for (pattern, count, _) in &values.extraction {
        for _ in 0..*count {
            wanted.push((pattern.group, pattern.planet));
        }
    }
    for (site, count) in &values.factories {
        for _ in 0..*count {
            wanted.push((site.group, site.planet));
        }
    }
    let characters = assign(groups, &wanted);
    let mut colonies = Vec::new();
    let mut slot = characters.into_iter();
    for (pattern, count, made) in &values.extraction {
        let planet = &planets[pattern.planet];
        let mut extracts = Vec::new();
        let mut exports = BTreeMap::new();
        let mut recipes = BTreeMap::new();
        for (output, total) in pattern.outputs.iter().zip(made) {
            let each = total / f64::from(*count);
            let Some(recipe) = catalogue.recipe(output.p1) else {
                continue;
            };
            let ratio = recipe
                .inputs
                .first()
                .map_or(1.0, |(_, quantity)| recipe.output_quantity / quantity);
            extracts.push((output.resource, each / ratio));
            *exports.entry(output.p1).or_default() += each;
            *recipes.entry(output.p1).or_default() += each / recipe.output_per_hour();
        }
        for _ in 0..*count {
            colonies.push(PlannedColony {
                character: slot.next().unwrap_or(0),
                planet_id: planet.planet_id,
                role: ColonyRole::Extraction,
                layout: pattern.layout.clone(),
                extracts: extracts.clone(),
                imports: BTreeMap::new(),
                exports: exports.clone(),
                recipes: recipes.clone(),
                tax_per_month: 0.0,
                cpu_used: 0.0,
                power_used: 0.0,
                on_planet_m3: 0.0,
                skyhook_m3: 0.0,
            });
        }
    }
    let factory_start = colonies.len();
    let mut factory_sites = Vec::new();
    for (site, count) in &values.factories {
        for _ in 0..*count {
            factory_sites.push(*site);
            colonies.push(PlannedColony {
                character: slot.next().unwrap_or(0),
                planet_id: planets[site.planet].planet_id,
                role: ColonyRole::Factory,
                layout: factory_base(),
                extracts: Vec::new(),
                imports: BTreeMap::new(),
                exports: BTreeMap::new(),
                recipes: BTreeMap::new(),
                tax_per_month: 0.0,
                cpu_used: 0.0,
                power_used: 0.0,
                on_planet_m3: 0.0,
                skyhook_m3: 0.0,
            });
        }
    }

    // Deal recipes to factory colonies by CPU: P4 to high-tech sites first,
    // then everything else by what room is left.
    let mut room: Vec<f64> = factory_sites.iter().map(|site| site.room.0).collect();
    let mut share: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    for high_tech in [true, false] {
        let recipes = values
            .made
            .keys()
            .filter(|type_id| (catalogue.tier(**type_id) == Some(4)) == high_tech)
            .copied()
            .collect::<Vec<_>>();
        let eligible = |index: usize| !high_tech || factory_sites[index].high_tech;
        let total = (0..factory_sites.len())
            .filter(|index| eligible(*index))
            .map(|index| room[index].max(0.0))
            .sum::<f64>();
        let weights = (0..factory_sites.len())
            .map(|index| {
                if eligible(index) && total > 0.0 {
                    room[index].max(0.0) / total
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>();
        for type_id in recipes {
            let recipe = catalogue.recipe(type_id);
            let cpu_per_unit = recipe.map_or(0.0, |recipe| {
                values.costs.get(&type_id).map_or(0.0, |cost| cost.0) / recipe.output_per_hour()
            });
            for (index, weight) in weights.iter().enumerate() {
                room[index] -= weight * values.made[&type_id] * cpu_per_unit;
            }
            share.insert(type_id, weights.clone());
        }
    }

    // Per-colony production and consumption of every upper recipe.
    let mut produced: Vec<BTreeMap<i64, f64>> = vec![BTreeMap::new(); colonies.len()];
    let mut consumed: Vec<BTreeMap<i64, f64>> = vec![BTreeMap::new(); colonies.len()];
    for (offset, _) in factory_sites.iter().enumerate() {
        let index = factory_start + offset;
        for (type_id, rate) in &values.made {
            let mine = rate * share[type_id][offset];
            if mine <= 1e-9 {
                continue;
            }
            let Some(recipe) = catalogue.recipe(*type_id) else {
                continue;
            };
            *produced[index].entry(*type_id).or_default() += mine;
            for (input, quantity) in &recipe.inputs {
                *consumed[index].entry(*input).or_default() +=
                    mine * quantity / recipe.output_quantity;
            }
            colonies[index]
                .recipes
                .insert(*type_id, mine / recipe.output_per_hour());
            if let Some(pin) =
                catalogue.processor_for(recipe, planets[factory_sites[offset].planet].type_id)
            {
                *colonies[index]
                    .layout
                    .processors
                    .entry(pin.type_id)
                    .or_default() += (mine / recipe.output_per_hour()).ceil() as u32;
            }
        }
    }
    for (index, colony) in colonies.iter().enumerate() {
        for (type_id, rate) in &colony.exports {
            *produced[index].entry(*type_id).or_default() += rate;
        }
    }
    for colony in &mut colonies {
        colony.exports.clear();
    }

    // Net each colony's own use, then match supply to demand by share.
    let mut flows = Vec::new();
    let types = produced
        .iter()
        .chain(&consumed)
        .flat_map(BTreeMap::keys)
        .chain(values.sold.keys())
        .chain(values.bought.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for type_id in types {
        let mut supply: Vec<(FlowEnd, f64)> = Vec::new();
        let mut demand: Vec<(FlowEnd, f64)> = Vec::new();
        for index in 0..colonies.len() {
            let made = produced[index].get(&type_id).copied().unwrap_or(0.0);
            let used = consumed[index].get(&type_id).copied().unwrap_or(0.0);
            if made > used + 1e-9 {
                supply.push((FlowEnd::Colony(index), made - used));
            } else if used > made + 1e-9 {
                demand.push((FlowEnd::Colony(index), used - made));
            }
        }
        if let Some(rate) = values.bought.get(&type_id) {
            supply.push((FlowEnd::Market, *rate));
        }
        if let Some(rate) = values.sold.get(&type_id) {
            demand.push((FlowEnd::Market, *rate));
        }
        let total_supply = supply.iter().map(|(_, rate)| rate).sum::<f64>();
        let total_demand = demand.iter().map(|(_, rate)| rate).sum::<f64>();
        let total = total_supply.max(total_demand);
        if total <= 1e-9 {
            continue;
        }
        for (from, offered) in &supply {
            for (to, wanted) in &demand {
                let rate = offered * wanted / total;
                if rate <= 1e-9 || from == to {
                    continue;
                }
                flows.push(PlannedFlow {
                    from: *from,
                    to: *to,
                    type_id,
                    units_per_hour: rate,
                    tax_per_month: 0.0,
                    cross_character: matches!(
                        (from, to),
                        (FlowEnd::Colony(a), FlowEnd::Colony(b))
                            if colonies[*a].character != colonies[*b].character
                    ),
                });
            }
        }
    }

    // Tax every transfer at its own planet, and tally each colony.
    let rate_of = |index: usize| {
        let planet_id = colonies[index].planet_id;
        planets
            .iter()
            .find(|planet| planet.planet_id == planet_id)
            .map_or(scenario.default_customs_rate, |planet| {
                customs_rate(scenario, planet)
            })
    };
    let mut customs_tax = 0.0;
    let mut tax_by_colony = vec![0.0; colonies.len()];
    for flow in &mut flows {
        let mut tax = 0.0;
        if let FlowEnd::Colony(from) = flow.from {
            let export = flow.units_per_hour * catalogue.export_tax(flow.type_id, rate_of(from));
            tax_by_colony[from] += export;
            tax += export;
        }
        if let FlowEnd::Colony(to) = flow.to {
            let import = flow.units_per_hour * catalogue.import_tax(flow.type_id, rate_of(to));
            tax_by_colony[to] += import;
            tax += import;
        }
        flow.tax_per_month = tax * HOURS_PER_MONTH;
        customs_tax += tax;
    }
    for flow in &flows {
        if let FlowEnd::Colony(from) = flow.from {
            *colonies[from].exports.entry(flow.type_id).or_default() += flow.units_per_hour;
        }
        if let FlowEnd::Colony(to) = flow.to {
            *colonies[to].imports.entry(flow.type_id).or_default() += flow.units_per_hour;
        }
    }

    // Buffers, capacity and effort per colony.
    let mut visits = Vec::new();
    for (index, colony) in colonies.iter_mut().enumerate() {
        colony.tax_per_month = tax_by_colony[index] * HOURS_PER_MONTH;
        let m3_per_hour = colony
            .imports
            .iter()
            .chain(&colony.exports)
            .map(|(type_id, rate)| rate * catalogue.volume(*type_id))
            .sum::<f64>();
        let extracts = colony.role == ColonyRole::Extraction;
        let held = buffers(
            m3_per_hour,
            scenario.effort.login_days(extracts),
            scenario.effort,
        );
        colony.on_planet_m3 = held.on_planet_m3;
        colony.skyhook_m3 = held.skyhook_m3;
        let planet = planets
            .iter()
            .find(|planet| planet.planet_id == colony.planet_id);
        let level = scenario
            .characters
            .get(colony.character)
            .map_or(0, |character| character.command_center_upgrades);
        if let Some(budget) = planet
            .and_then(|planet| ColonyBudget::new(catalogue, planet.type_id, planet.radius_m, level))
        {
            let (cpu, power) = budget.output();
            let (used_cpu, used_power) = budget.usage(catalogue, &colony.layout);
            colony.cpu_used = used_cpu / cpu;
            colony.power_used = used_power / power;
        }
        visits.push(ColonyVisit {
            character: colony.character,
            extracts,
            m3_per_hour,
        });
    }
    let effort = effort(&visits, scenario.characters.len(), scenario.effort);

    let sales = values
        .sold
        .iter()
        .map(|(type_id, rate)| rate * prices.best_buy[type_id])
        .sum::<f64>();
    let purchases = values
        .bought
        .iter()
        .map(|(type_id, rate)| rate * prices.best_sell[type_id])
        .sum::<f64>();

    // The readings the plan rests on, and hotspots shared between characters.
    let program_hours = f64::from(scenario.effort.reset_days.max(1)) * 24.0;
    let mut inputs = Vec::new();
    let mut extractors: BTreeMap<(i64, i64), BTreeSet<usize>> = BTreeMap::new();
    for (colony, (resource, _)) in colonies.iter().flat_map(|colony| {
        colony
            .extracts
            .iter()
            .map(move |extract| (colony, *extract))
    }) {
        extractors
            .entry((colony.planet_id, resource))
            .or_default()
            .insert(colony.character);
        if inputs.iter().any(|input: &RankedInput| {
            input.planet_id == colony.planet_id && input.resource_type_id == resource
        }) {
            continue;
        }
        if let Some(planet) = planets
            .iter()
            .find(|planet| planet.planet_id == colony.planet_id)
        {
            inputs.push(RankedInput {
                resource_type_id: resource,
                planet_id: planet.planet_id,
                source: resolve_yield(planet.densities.get(&resource), planet.band, program_hours)
                    .source,
                contested: planet.contested,
            });
        }
    }
    let shared_hotspots = extractors
        .into_iter()
        .filter(|(_, characters)| characters.len() > 1)
        .map(
            |((planet_id, resource_type_id), characters)| SharedHotspot {
                planet_id,
                resource_type_id,
                characters: characters.len() as u32,
            },
        )
        .collect();

    PiPlan {
        isk_per_month: (sales - purchases - customs_tax) * HOURS_PER_MONTH,
        sales_per_month: sales * HOURS_PER_MONTH,
        purchases_per_month: purchases * HOURS_PER_MONTH,
        customs_tax_per_month: customs_tax * HOURS_PER_MONTH,
        colonies,
        flows,
        effort,
        inputs,
        shared_hotspots,
    }
}

#[cfg(test)]
mod tests;
