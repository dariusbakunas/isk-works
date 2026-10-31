//! The optimised plan and its what-ifs, for the Planner's Plan tab.

use std::time::Duration;

use chrono::{DateTime, Utc};
use iskworks_core::pi_planner::{solve, what_ifs, ColonyRole, FlowEnd, PiPlan, ScanFirst, WhatIfs};
use iskworks_core::{MarketScope, WorkspaceId};
use serde::Serialize;

use super::{
    effort_dto, fmt_f64, ranked_input, Catalogue, EffortDto, NamedTypeDto, Neighbourhood,
    PiPlannerError, PiPlannerService, PlannerScenarioDto, Prepared, RankedInputDto, YieldSourceDto,
};

/// A solve and its what-ifs together; past this the request gives up.
pub const SOLVE_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerSolutionDto {
    pub price_observed_at: Option<DateTime<Utc>>,
    pub available_colonies: u32,
    pub plan: PlanDto,
    pub what_ifs: WhatIfsDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub isk_per_month: String,
    pub sales_per_month: String,
    pub purchases_per_month: String,
    pub customs_tax_per_month: String,
    pub effort: EffortDto,
    pub colonies: Vec<ColonyDto>,
    pub flows: Vec<FlowDto>,
    pub inputs: Vec<RankedInputDto>,
    pub shared_hotspots: Vec<SharedHotspotDto>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ColonyRoleDto {
    Extraction,
    Factory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateDto {
    pub type_id: i64,
    pub name: String,
    pub tier: u8,
    pub units_per_hour: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureCountDto {
    pub type_id: i64,
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupDto {
    pub ecus: u32,
    pub heads: u32,
    pub launchpads: u32,
    pub extra_storage: u32,
    pub processors: Vec<StructureCountDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeDto {
    pub type_id: i64,
    pub name: String,
    pub tier: u8,
    /// Fractional when the colony shares a recipe with others.
    pub factories: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColonyDto {
    /// Position in `colonies`; flows refer to it.
    pub index: usize,
    /// Position in the scenario's characters.
    pub character: usize,
    pub planet_id: i64,
    pub planet_name: String,
    pub jumps: u8,
    pub role: ColonyRoleDto,
    pub setup: SetupDto,
    pub extracts: Vec<RateDto>,
    pub imports: Vec<RateDto>,
    pub exports: Vec<RateDto>,
    pub recipes: Vec<RecipeDto>,
    pub tax_per_month: String,
    /// 0–1.
    pub cpu_used: String,
    pub power_used: String,
    pub on_planet_m3: String,
    pub skyhook_m3: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDto {
    /// Colony index; `null` is the market.
    pub from: Option<usize>,
    pub to: Option<usize>,
    pub type_id: i64,
    pub name: String,
    pub tier: u8,
    pub units_per_hour: String,
    pub m3_per_hour: String,
    pub tax_per_month: String,
    pub cross_character: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedHotspotDto {
    pub planet_id: i64,
    pub planet_name: String,
    pub resource_type_id: i64,
    pub resource_name: String,
    pub characters: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusValueDto {
    pub jumps: u8,
    pub isk_per_month: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortCellDto {
    pub reset_days: u32,
    pub haul_days: u32,
    pub isk_per_month: String,
    pub logins_per_week: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFirstDto {
    pub planet_id: i64,
    pub planet_name: String,
    pub resource_type_id: i64,
    pub resource_name: String,
    pub source: YieldSourceDto,
    pub isk_lost_per_month: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatIfsDto {
    /// ISK/month with the first 1..=N characters.
    pub per_character: Vec<String>,
    pub per_radius: Vec<RadiusValueDto>,
    pub grid: Vec<EffortCellDto>,
    pub scan_first: Vec<ScanFirstDto>,
}

fn rates(catalogue: &Catalogue, rates: impl IntoIterator<Item = (i64, f64)>) -> Vec<RateDto> {
    rates
        .into_iter()
        .map(|(type_id, rate)| RateDto {
            type_id,
            name: catalogue.name(type_id),
            tier: catalogue.catalogue.tier(type_id).unwrap_or(0),
            units_per_hour: fmt_f64(rate, 2),
        })
        .collect()
}

fn plan_dto(catalogue: &Catalogue, area: &Neighbourhood, plan: PiPlan) -> PlanDto {
    let planet_name = |planet_id: i64| area.planet_name(planet_id);
    let jumps = |planet_id: i64| {
        area.planets
            .iter()
            .find(|planet| planet.planet_id == planet_id)
            .and_then(|planet| area.system(planet.solar_system_id))
            .map_or(0, |system| system.jumps)
    };
    let colonies = plan
        .colonies
        .iter()
        .enumerate()
        .map(|(index, colony)| ColonyDto {
            index,
            character: colony.character,
            planet_id: colony.planet_id,
            planet_name: planet_name(colony.planet_id),
            jumps: jumps(colony.planet_id),
            role: match colony.role {
                ColonyRole::Extraction => ColonyRoleDto::Extraction,
                ColonyRole::Factory => ColonyRoleDto::Factory,
            },
            setup: SetupDto {
                ecus: colony.layout.ecus,
                heads: colony.layout.heads,
                launchpads: colony.layout.launchpads,
                extra_storage: colony.layout.extra_storage,
                processors: colony
                    .layout
                    .processors
                    .iter()
                    .filter(|(_, count)| **count > 0)
                    .map(|(type_id, count)| StructureCountDto {
                        type_id: *type_id,
                        name: catalogue.name(*type_id),
                        count: *count,
                    })
                    .collect(),
            },
            extracts: rates(catalogue, colony.extracts.iter().copied()),
            imports: rates(
                catalogue,
                colony.imports.iter().map(|(id, rate)| (*id, *rate)),
            ),
            exports: rates(
                catalogue,
                colony.exports.iter().map(|(id, rate)| (*id, *rate)),
            ),
            recipes: colony
                .recipes
                .iter()
                .map(|(type_id, factories)| RecipeDto {
                    type_id: *type_id,
                    name: catalogue.name(*type_id),
                    tier: catalogue.catalogue.tier(*type_id).unwrap_or(0),
                    factories: fmt_f64(*factories, 2),
                })
                .collect(),
            tax_per_month: fmt_f64(colony.tax_per_month, 2),
            cpu_used: fmt_f64(colony.cpu_used, 3),
            power_used: fmt_f64(colony.power_used, 3),
            on_planet_m3: fmt_f64(colony.on_planet_m3, 0),
            skyhook_m3: fmt_f64(colony.skyhook_m3, 0),
        })
        .collect();
    let end = |end: FlowEnd| match end {
        FlowEnd::Colony(index) => Some(index),
        FlowEnd::Market => None,
    };
    let flows = plan
        .flows
        .iter()
        .map(|flow| FlowDto {
            from: end(flow.from),
            to: end(flow.to),
            type_id: flow.type_id,
            name: catalogue.name(flow.type_id),
            tier: catalogue.catalogue.tier(flow.type_id).unwrap_or(0),
            units_per_hour: fmt_f64(flow.units_per_hour, 2),
            m3_per_hour: fmt_f64(
                flow.units_per_hour * catalogue.catalogue.volume(flow.type_id),
                2,
            ),
            tax_per_month: fmt_f64(flow.tax_per_month, 2),
            cross_character: flow.cross_character,
        })
        .collect();
    PlanDto {
        isk_per_month: fmt_f64(plan.isk_per_month, 2),
        sales_per_month: fmt_f64(plan.sales_per_month, 2),
        purchases_per_month: fmt_f64(plan.purchases_per_month, 2),
        customs_tax_per_month: fmt_f64(plan.customs_tax_per_month, 2),
        effort: effort_dto(&plan.effort),
        colonies,
        flows,
        inputs: plan
            .inputs
            .into_iter()
            .map(|input| ranked_input(catalogue, input, &planet_name))
            .collect(),
        shared_hotspots: plan
            .shared_hotspots
            .iter()
            .map(|hotspot| SharedHotspotDto {
                planet_id: hotspot.planet_id,
                planet_name: planet_name(hotspot.planet_id),
                resource_type_id: hotspot.resource_type_id,
                resource_name: catalogue.name(hotspot.resource_type_id),
                characters: hotspot.characters,
            })
            .collect(),
    }
}

fn what_ifs_dto(catalogue: &Catalogue, area: &Neighbourhood, what_ifs: WhatIfs) -> WhatIfsDto {
    WhatIfsDto {
        per_character: what_ifs
            .per_character
            .iter()
            .map(|isk| fmt_f64(*isk, 2))
            .collect(),
        per_radius: what_ifs
            .per_radius
            .iter()
            .map(|(jumps, isk)| RadiusValueDto {
                jumps: *jumps,
                isk_per_month: fmt_f64(*isk, 2),
            })
            .collect(),
        grid: what_ifs
            .grid
            .iter()
            .map(|cell| EffortCellDto {
                reset_days: cell.reset_days,
                haul_days: cell.haul_days,
                isk_per_month: fmt_f64(cell.isk_per_month, 2),
                logins_per_week: fmt_f64(cell.logins_per_week, 2),
            })
            .collect(),
        scan_first: what_ifs
            .scan_first
            .iter()
            .map(|entry: &ScanFirst| ScanFirstDto {
                planet_id: entry.planet_id,
                planet_name: area.planet_name(entry.planet_id),
                resource_type_id: entry.resource_type_id,
                resource_name: catalogue.name(entry.resource_type_id),
                source: entry.source.into(),
                isk_lost_per_month: fmt_f64(entry.isk_lost_per_month, 2),
            })
            .collect(),
    }
}

impl PiPlannerService {
    /// The optimised plan for the scenario, and its what-ifs. CPU-bound: it
    /// runs off the async runtime and gives up after [`SOLVE_DEADLINE`].
    pub async fn solve(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        request: PlannerScenarioDto,
    ) -> Result<PlannerSolutionDto, PiPlannerError> {
        let Prepared {
            scenario,
            catalogue,
            area,
            candidates,
            prices,
            price_observed_at,
        } = self.prepare(workspace_id, scope, &request).await?;
        let available_colonies = scenario.available_colonies();
        let radius = request.radius;
        let work = tokio::task::spawn_blocking(move || {
            let plan = solve(&catalogue.catalogue, &scenario, &candidates, &prices)
                .map_err(|error| PiPlannerError::Solver(error.to_string()))?;
            let what_ifs = what_ifs(
                &catalogue.catalogue,
                &scenario,
                &candidates,
                &prices,
                radius,
                &plan,
            );
            Ok::<_, PiPlannerError>((catalogue, plan, what_ifs))
        });
        let (catalogue, plan, what_ifs) = tokio::time::timeout(SOLVE_DEADLINE, work)
            .await
            .map_err(|_| PiPlannerError::SolveTimeout)?
            .map_err(|error| PiPlannerError::Solver(error.to_string()))??;
        Ok(PlannerSolutionDto {
            price_observed_at,
            available_colonies,
            plan: plan_dto(&catalogue, &area, plan),
            what_ifs: what_ifs_dto(&catalogue, &area, what_ifs),
        })
    }
}

/// Every product the plan sells or buys, for market coverage.
#[must_use]
pub fn traded_types(solution: &PlannerSolutionDto) -> Vec<NamedTypeDto> {
    solution
        .plan
        .flows
        .iter()
        .filter(|flow| flow.from.is_none() || flow.to.is_none())
        .map(|flow| NamedTypeDto {
            type_id: flow.type_id,
            name: flow.name.clone(),
        })
        .collect()
}
