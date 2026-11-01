//! What-ifs around a solved plan: what each character, each jump of radius
//! and each reset/haul cadence is worth, and which readings to scan first.
//! Every figure is a full re-solve; they run in parallel.

use std::thread;

use super::catalogue::PiCatalogue;
use super::effort::EffortSettings;
use super::rank::{CandidatePlanet, Prices, RankScenario};
use super::solve::{solve, PiPlan};
use super::yields::{DensityEntry, YieldSource, HEAD_VALUE_PER_DENSITY_PCT};

/// The profit vs effort grid's reset and haul intervals, in days.
pub const GRID_RESET_DAYS: [u32; 3] = [1, 3, 7];
pub const GRID_HAUL_DAYS: [u32; 4] = [1, 3, 7, 14];
/// How far a reading drops in the sensitivity re-solves, in density points.
pub const SENSITIVITY_POINTS: f64 = 10.0;
/// Readings in the "scan these first" list.
pub const SCAN_FIRST: usize = 5;
/// Unscanned readings re-solved one at a time, to price scanning each.
const UNSCANNED_SOLVES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffortCell {
    pub reset_days: u32,
    pub haul_days: u32,
    pub isk_per_month: f64,
    /// Averaged across characters.
    pub logins_per_week: f64,
}

/// One reading worth scanning or checking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanFirst {
    pub planet_id: i64,
    pub resource_type_id: i64,
    pub source: YieldSource,
    /// Unscanned: what the reading alone adds to the plan of entered
    /// readings, at its default density. Entered: what a reading
    /// `SENSITIVITY_POINTS` lower costs.
    pub isk_per_month: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhatIfs {
    /// ISK/month with the first 1..=N characters.
    pub per_character: Vec<f64>,
    /// ISK/month at each radius from 0 up to the scenario's, in jumps.
    pub per_radius: Vec<(u8, f64)>,
    pub grid: Vec<EffortCell>,
    /// Unscanned readings first, then entered ones; most ISK first.
    pub scan_first: Vec<ScanFirst>,
}

/// Runs every job on a few threads; `None` where a solve failed.
fn solve_all(
    catalogue: &PiCatalogue,
    prices: &Prices,
    jobs: Vec<(RankScenario, Vec<CandidatePlanet>)>,
) -> Vec<Option<PiPlan>> {
    let workers = thread::available_parallelism()
        .map_or(4, |count| count.get())
        .max(1);
    let chunk = jobs.len().div_ceil(workers).max(1);
    thread::scope(|scope| {
        let handles = jobs
            .chunks(chunk)
            .map(|batch| {
                scope.spawn(move || {
                    batch
                        .iter()
                        .map(|(scenario, planets)| solve(catalogue, scenario, planets, prices).ok())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

/// The reading `SENSITIVITY_POINTS` density points worse.
fn worse(entry: Option<&DensityEntry>, planet: &CandidatePlanet) -> DensityEntry {
    match entry.copied().unwrap_or_default() {
        DensityEntry {
            head_value: Some(head_value),
            ..
        } => DensityEntry {
            density_pct: None,
            head_value: Some(
                (head_value - SENSITIVITY_POINTS * HEAD_VALUE_PER_DENSITY_PCT).max(0.1),
            ),
        },
        entry => DensityEntry {
            density_pct: Some(
                (entry
                    .density_pct
                    .unwrap_or(planet.band.default_density_pct())
                    - SENSITIVITY_POINTS)
                    .max(0.0),
            ),
            head_value: None,
        },
    }
}

/// `planets` with one reading replaced.
fn with_reading(
    planets: &[CandidatePlanet],
    planet_id: i64,
    resource: i64,
    entry: impl Fn(&CandidatePlanet) -> DensityEntry,
) -> Vec<CandidatePlanet> {
    planets
        .iter()
        .map(|planet| {
            let mut planet = planet.clone();
            if planet.planet_id == planet_id {
                let replaced = entry(&planet);
                planet.densities.insert(resource, replaced);
            }
            planet
        })
        .collect()
}

fn isk(plan: Option<&PiPlan>) -> f64 {
    plan.map_or(0.0, |plan| plan.isk_per_month)
}

/// What-ifs for `plan`, solved from `scenario` over `planets` (those within
/// `radius` jumps).
#[must_use]
pub fn what_ifs(
    catalogue: &PiCatalogue,
    scenario: &RankScenario,
    planets: &[CandidatePlanet],
    prices: &Prices,
    radius: u8,
    plan: &PiPlan,
) -> WhatIfs {
    let mut jobs = Vec::new();
    let characters = scenario.characters.len();
    for count in 1..characters {
        let mut fewer = scenario.clone();
        fewer.characters.truncate(count);
        jobs.push((fewer, planets.to_vec()));
    }
    for jumps in 0..radius {
        let nearer = planets
            .iter()
            .filter(|planet| planet.jumps <= jumps)
            .cloned()
            .collect();
        jobs.push((scenario.clone(), nearer));
    }
    for reset_days in GRID_RESET_DAYS {
        for haul_days in GRID_HAUL_DAYS {
            let mut cadence = scenario.clone();
            cadence.effort = EffortSettings {
                reset_days,
                haul_days,
                ..scenario.effort
            };
            jobs.push((cadence, planets.to_vec()));
        }
    }
    let entered = plan
        .inputs
        .iter()
        .filter(|input| input.source != YieldSource::Default)
        .collect::<Vec<_>>();
    for input in &entered {
        let lower = with_reading(planets, input.planet_id, input.resource_type_id, |planet| {
            worse(planet.densities.get(&input.resource_type_id), planet)
        });
        jobs.push((scenario.clone(), lower));
    }
    // Unscanned readings come from the plan with estimates on, and are
    // priced against the plan of entered readings only.
    let estimating = RankScenario {
        estimate_unscanned: true,
        ..scenario.clone()
    };
    let entered_only = RankScenario {
        estimate_unscanned: false,
        ..scenario.clone()
    };
    jobs.push(if scenario.estimate_unscanned {
        (entered_only.clone(), planets.to_vec())
    } else {
        (estimating, planets.to_vec())
    });

    let mut results = solve_all(catalogue, prices, jobs).into_iter();
    let mut per_character = (1..characters)
        .map(|_| isk(results.next().flatten().as_ref()))
        .collect::<Vec<_>>();
    per_character.push(plan.isk_per_month);
    let mut per_radius = (0..radius)
        .map(|jumps| (jumps, isk(results.next().flatten().as_ref())))
        .collect::<Vec<_>>();
    per_radius.push((radius, plan.isk_per_month));
    let mut grid = Vec::new();
    for reset_days in GRID_RESET_DAYS {
        for haul_days in GRID_HAUL_DAYS {
            let solved = results.next().flatten();
            let logins = solved.as_ref().map_or(0.0, |plan| {
                let by_character = &plan.effort.logins_per_week_by_character;
                by_character.iter().sum::<f64>() / by_character.len().max(1) as f64
            });
            grid.push(EffortCell {
                reset_days,
                haul_days,
                isk_per_month: isk(solved.as_ref()),
                logins_per_week: logins,
            });
        }
    }
    let mut scan_first = entered
        .iter()
        .map(|input| ScanFirst {
            planet_id: input.planet_id,
            resource_type_id: input.resource_type_id,
            source: input.source,
            isk_per_month: (plan.isk_per_month - isk(results.next().flatten().as_ref())).max(0.0),
        })
        .collect::<Vec<_>>();
    let other = results.next().flatten();
    let (estimated, base) = if scenario.estimate_unscanned {
        (Some(plan), other.as_ref())
    } else {
        (other.as_ref(), Some(plan))
    };
    if let Some(estimated) = estimated {
        let unscanned = estimated
            .inputs
            .iter()
            .filter(|input| input.source == YieldSource::Default)
            .take(UNSCANNED_SOLVES)
            .collect::<Vec<_>>();
        let alone = unscanned
            .iter()
            .map(|input| {
                let planets =
                    with_reading(planets, input.planet_id, input.resource_type_id, |planet| {
                        DensityEntry {
                            density_pct: Some(planet.band.default_density_pct()),
                            head_value: None,
                        }
                    });
                (entered_only.clone(), planets)
            })
            .collect();
        let base = isk(base);
        scan_first.extend(
            unscanned
                .iter()
                .zip(solve_all(catalogue, prices, alone))
                .map(|(input, solved)| ScanFirst {
                    planet_id: input.planet_id,
                    resource_type_id: input.resource_type_id,
                    source: input.source,
                    isk_per_month: (isk(solved.as_ref()) - base).max(0.0),
                }),
        );
    }
    scan_first.sort_by(|a, b| {
        (b.source == YieldSource::Default)
            .cmp(&(a.source == YieldSource::Default))
            .then(b.isk_per_month.total_cmp(&a.isk_per_month))
            .then(a.planet_id.cmp(&b.planet_id))
            .then(a.resource_type_id.cmp(&b.resource_type_id))
    });
    scan_first.truncate(SCAN_FIRST);

    WhatIfs {
        per_character,
        per_radius,
        grid,
        scan_first,
    }
}

#[cfg(test)]
mod tests;
