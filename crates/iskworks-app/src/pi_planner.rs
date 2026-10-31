//! The PI Planner read model: planets around a home system with what the
//! player entered for them, the connected characters' PI skills, and the
//! product ranking from `iskworks_core::pi_planner`, priced in the
//! workspace's market scope.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::pi_planner::{
    rank, CandidatePlanet, DensityEntry, DensityWrite, EffortSettings, OutOfReach, PiCatalogue,
    PiPlannerInputsRepository, PlanetSettingsWrite, PlannerCharacter, Prices, RankScenario,
    SecurityBand, YieldSource, INTERVAL_DAYS,
};
use iskworks_core::{
    summarize_scoped_orders, InventoryError, MarketError, MarketRepository, MarketScope,
    WorkspaceId,
};
use iskworks_sde::{SdeError, SdeReadRepository, SystemDistance};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::character_roster::current_trained_level;
use crate::PlanetaryCharacterSource;

/// Interplanetary Consolidation: colonies beyond the first.
const INTERPLANETARY_CONSOLIDATION_SKILL_ID: i64 = 2_495;
/// Command Center Upgrades: the command center level.
const COMMAND_CENTER_UPGRADES_SKILL_ID: i64 = 2_505;
/// Planet type names. The SDE import skips celestial types, so their names
/// aren't in `sde_types`; these ids have been stable since PI shipped.
fn planet_type_name(type_id: i64) -> Option<&'static str> {
    Some(match type_id {
        11 => "Planet (Temperate)",
        12 => "Planet (Ice)",
        13 => "Planet (Gas)",
        2014 => "Planet (Oceanic)",
        2015 => "Planet (Lava)",
        2016 => "Planet (Barren)",
        2017 => "Planet (Storm)",
        2063 => "Planet (Plasma)",
        30_889 => "Planet (Shattered)",
        _ => return None,
    })
}

/// The widest radius the planner walks, in jumps.
pub const MAX_PLANNER_RADIUS: u8 = 5;

#[derive(Debug, thiserror::Error)]
pub enum PiPlannerError {
    /// The active SDE import predates the PI Planner data.
    #[error("the active SDE import has no PI Planner data")]
    SdeMissingPlannerData,
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error(transparent)]
    Sde(#[from] SdeError),
    #[error(transparent)]
    Market(#[from] MarketError),
    /// The solve ran past its deadline.
    #[error("the plan took too long to solve")]
    SolveTimeout,
    #[error("the plan could not be solved: {0}")]
    Solver(String),
}

fn invalid(message: &str) -> PiPlannerError {
    PiPlannerError::Inventory(InventoryError::Validation(message.to_string()))
}

#[derive(Clone)]
pub struct PiPlannerService {
    sde: Arc<dyn SdeReadRepository>,
    market: Arc<dyn MarketRepository>,
    inputs: Arc<dyn PiPlannerInputsRepository>,
    characters: Option<Arc<dyn PlanetaryCharacterSource>>,
}

// ---- Planets ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerPlanetsDto {
    pub systems: Vec<PlannerSystemDto>,
    pub planets: Vec<PlannerPlanetDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerSystemDto {
    pub solar_system_id: i64,
    pub name: String,
    pub security_status: Option<String>,
    pub region_name: Option<String>,
    pub band: SecurityBandDto,
    pub jumps: u8,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityBandDto {
    High,
    Low,
    Null,
    Wormhole,
}

impl From<SecurityBand> for SecurityBandDto {
    fn from(band: SecurityBand) -> Self {
        match band {
            SecurityBand::High => Self::High,
            SecurityBand::Low => Self::Low,
            SecurityBand::Null => Self::Null,
            SecurityBand::Wormhole => Self::Wormhole,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerPlanetDto {
    pub planet_id: i64,
    pub name: String,
    pub solar_system_id: i64,
    pub jumps: u8,
    pub type_id: i64,
    pub type_name: String,
    pub customs_rate: Option<String>,
    pub contested: bool,
    pub settings_updated_at: Option<DateTime<Utc>>,
    /// The raw resources this planet type yields, by name.
    pub resources: Vec<PlannerResourceDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerResourceDto {
    pub type_id: i64,
    pub name: String,
    pub density_pct: Option<String>,
    pub head_value: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    /// The density assumed while nothing is entered.
    pub default_density_pct: String,
}

// ---- Inputs ----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DensityWriteDto {
    pub planet_id: i64,
    pub resource_type_id: i64,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub density_pct: Option<Decimal>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub head_value: Option<Decimal>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetSettingsWriteDto {
    pub planet_id: i64,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub customs_rate: Option<Decimal>,
    #[serde(default)]
    pub contested: bool,
}

// ---- Characters ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerCharacterSkillsDto {
    pub eve_character_id: i64,
    pub name: String,
    /// `None` until the character's skills have synced.
    pub interplanetary_consolidation: Option<i64>,
    pub command_center_upgrades: Option<i64>,
}

// ---- Rank ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerCharacterDto {
    pub interplanetary_consolidation: u8,
    pub command_center_upgrades: u8,
}

/// A scenario: request body only, never stored.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerScenarioDto {
    pub home_system_id: i64,
    pub radius: u8,
    pub characters: Vec<PlannerCharacterDto>,
    /// 0.1 = 10 %.
    #[serde(with = "rust_decimal::serde::str")]
    pub default_customs_rate: Decimal,
    pub reset_days: u32,
    pub haul_days: u32,
    #[serde(default)]
    pub uplift_at_login: bool,
    #[serde(default)]
    pub buy_intermediates: bool,
    #[serde(default)]
    pub max_colonies: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerRankingDto {
    pub price_observed_at: Option<DateTime<Utc>>,
    pub available_colonies: u32,
    pub ranked: Vec<RankedProductDto>,
    pub out_of_reach: Vec<OutOfReachDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedProductDto {
    pub type_id: i64,
    pub name: String,
    pub tier: u8,
    pub isk_per_month: String,
    pub units_per_hour: String,
    pub colonies: u32,
    pub chains: u32,
    pub inputs: Vec<RankedInputDto>,
    pub bought: Vec<NamedTypeDto>,
    /// 0–1.
    pub confidence: String,
    pub effort: EffortDto,
    pub haul_constrained_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedInputDto {
    pub resource_type_id: i64,
    pub resource_name: String,
    pub planet_id: i64,
    pub planet_name: String,
    pub source: YieldSourceDto,
    pub contested: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum YieldSourceDto {
    HeadValue,
    Scanned,
    Default,
}

impl From<YieldSource> for YieldSourceDto {
    fn from(source: YieldSource) -> Self {
        match source {
            YieldSource::HeadValue => Self::HeadValue,
            YieldSource::Scanned => Self::Scanned,
            YieldSource::Default => Self::Default,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedTypeDto {
    pub type_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortDto {
    pub logins_per_week_by_character: Vec<String>,
    pub haul_runs_per_week: String,
    pub stops_per_run: u32,
    pub m3_per_run: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutOfReachDto {
    pub type_id: i64,
    pub name: String,
    pub tier: u8,
    pub reason: OutOfReachReasonDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutOfReachReasonDto {
    #[serde(rename_all = "camelCase")]
    MissingResource {
        type_id: i64,
        name: String,
    },
    NoFactoryPlanet,
    NeedsColonies {
        needed: u32,
        available: u32,
    },
    OverflowsSkyhook,
    NoRoom,
    NoPrice,
}

// ---- Service ---------------------------------------------------------------

/// Decimal string of an estimate, at `dp` places.
fn fmt_f64(value: f64, dp: u32) -> String {
    Decimal::from_f64(value)
        .unwrap_or_default()
        .round_dp(dp)
        .normalize()
        .to_string()
}

fn fmt(value: Decimal) -> String {
    value.normalize().to_string()
}

fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or_default()
}

fn band(system: &SystemDistance) -> SecurityBand {
    SecurityBand::classify(system.security_status.map(to_f64), system.wormhole)
}

/// The static PI data and every PI commodity's name.
struct Catalogue {
    catalogue: PiCatalogue,
    names: BTreeMap<i64, String>,
}

impl Catalogue {
    fn name(&self, type_id: i64) -> String {
        self.names
            .get(&type_id)
            .cloned()
            .unwrap_or_else(|| format!("Type {type_id}"))
    }
}

/// What a rank or a solve works from.
struct Prepared {
    scenario: RankScenario,
    catalogue: Catalogue,
    area: Neighbourhood,
    candidates: Vec<CandidatePlanet>,
    prices: Prices,
    price_observed_at: Option<DateTime<Utc>>,
}

fn ranked_input(
    catalogue: &Catalogue,
    input: iskworks_core::pi_planner::RankedInput,
    planet_name: &impl Fn(i64) -> String,
) -> RankedInputDto {
    RankedInputDto {
        resource_type_id: input.resource_type_id,
        resource_name: catalogue.name(input.resource_type_id),
        planet_id: input.planet_id,
        planet_name: planet_name(input.planet_id),
        source: input.source.into(),
        contested: input.contested,
    }
}

fn effort_dto(effort: &iskworks_core::pi_planner::Effort) -> EffortDto {
    EffortDto {
        logins_per_week_by_character: effort
            .logins_per_week_by_character
            .iter()
            .map(|logins| fmt_f64(*logins, 2))
            .collect(),
        haul_runs_per_week: fmt_f64(effort.haul_runs_per_week, 2),
        stops_per_run: effort.stops_per_run,
        m3_per_run: fmt_f64(effort.m3_per_run, 2),
    }
}

/// The planets in range with what the player entered for them.
struct Neighbourhood {
    systems: Vec<SystemDistance>,
    planets: Vec<iskworks_sde::PlannerPlanet>,
    densities: BTreeMap<(i64, i64), iskworks_core::pi_planner::StoredDensity>,
    settings: BTreeMap<i64, iskworks_core::pi_planner::StoredPlanetSettings>,
}

impl Neighbourhood {
    fn planet_name(&self, planet_id: i64) -> String {
        self.planets
            .iter()
            .find(|planet| planet.planet_id == planet_id)
            .map_or_else(
                || format!("Planet {planet_id}"),
                |planet| planet.name.clone(),
            )
    }

    fn system(&self, solar_system_id: i64) -> Option<&SystemDistance> {
        self.systems
            .iter()
            .find(|system| system.solar_system_id == solar_system_id)
    }
}

impl PiPlannerService {
    #[must_use]
    pub fn new(
        sde: Arc<dyn SdeReadRepository>,
        market: Arc<dyn MarketRepository>,
        inputs: Arc<dyn PiPlannerInputsRepository>,
    ) -> Self {
        Self {
            sde,
            market,
            inputs,
            characters: None,
        }
    }

    /// Connected characters, for prefilling skills. Without it the
    /// characters list is empty.
    #[must_use]
    pub fn with_characters(mut self, characters: Arc<dyn PlanetaryCharacterSource>) -> Self {
        self.characters = Some(characters);
        self
    }

    async fn catalogue(&self) -> Result<Catalogue, PiPlannerError> {
        let pins = self.sde.planet_pin_types().await?;
        let schematics = self.sde.planet_schematic_catalogue().await?;
        if pins.is_empty() || schematics.is_empty() {
            return Err(PiPlannerError::SdeMissingPlannerData);
        }
        let commodities = self.sde.planet_commodities().await?;
        let type_ids: Vec<i64> = schematics
            .iter()
            .flat_map(|schematic| schematic.inputs.iter().chain(&schematic.outputs))
            .map(|line| line.type_id)
            .chain(pins.iter().filter_map(|pin| pin.harvester_type_id))
            .chain(pins.iter().map(|pin| pin.type_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let references = self.sde.type_reference(&type_ids).await?;
        let volumes = references
            .iter()
            .filter_map(|(type_id, reference)| {
                reference
                    .packaged_volume_m3
                    .map(|volume| (*type_id, to_f64(volume)))
            })
            .collect();
        let names = references
            .into_iter()
            .filter_map(|(type_id, reference)| reference.type_name.map(|name| (type_id, name)))
            .collect();
        Ok(Catalogue {
            catalogue: PiCatalogue::build(pins, schematics, commodities, volumes),
            names,
        })
    }

    async fn neighbourhood(
        &self,
        workspace_id: WorkspaceId,
        system_id: i64,
        radius: u8,
    ) -> Result<Neighbourhood, PiPlannerError> {
        if radius > MAX_PLANNER_RADIUS {
            return Err(invalid("radius must be 0 to 5 jumps"));
        }
        let systems = self.sde.systems_within_jumps(system_id, radius).await?;
        if systems.is_empty() {
            return Err(invalid("systemId is not a known solar system"));
        }
        let system_ids: Vec<i64> = systems
            .iter()
            .map(|system| system.solar_system_id)
            .collect();
        let planets = self.sde.planets_in_systems(&system_ids).await?;
        let planet_ids: Vec<i64> = planets.iter().map(|planet| planet.planet_id).collect();
        let densities = self
            .inputs
            .densities(workspace_id, &planet_ids)
            .await?
            .into_iter()
            .map(|density| ((density.planet_id, density.resource_type_id), density))
            .collect();
        let settings = self
            .inputs
            .planet_settings(workspace_id, &planet_ids)
            .await?
            .into_iter()
            .map(|settings| (settings.planet_id, settings))
            .collect();
        let mut neighbourhood = Neighbourhood {
            systems,
            planets,
            densities,
            settings,
        };
        let order: BTreeMap<i64, usize> = neighbourhood
            .systems
            .iter()
            .enumerate()
            .map(|(index, system)| (system.solar_system_id, index))
            .collect();
        neighbourhood.planets.sort_by_key(|planet| {
            (
                order.get(&planet.solar_system_id).copied(),
                planet.celestial_index,
                planet.planet_id,
            )
        });
        Ok(neighbourhood)
    }

    /// Systems within `radius` jumps of `system_id`, and their planets with
    /// each raw resource's entered reading.
    pub async fn planets(
        &self,
        workspace_id: WorkspaceId,
        system_id: i64,
        radius: u8,
    ) -> Result<PlannerPlanetsDto, PiPlannerError> {
        let catalogue = self.catalogue().await?;
        let area = self.neighbourhood(workspace_id, system_id, radius).await?;
        let planet_type_ids: Vec<i64> = area
            .planets
            .iter()
            .map(|planet| planet.type_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let type_names = self.sde.type_names(&planet_type_ids).await?;

        let planets = area
            .planets
            .iter()
            .map(|planet| {
                let system = area.system(planet.solar_system_id);
                let default_pct = system
                    .map_or(SecurityBand::Null, band)
                    .default_density_pct();
                let mut resources: Vec<PlannerResourceDto> = catalogue
                    .catalogue
                    .resources(planet.type_id)
                    .map(|resource| {
                        let entry = area.densities.get(&(planet.planet_id, resource));
                        PlannerResourceDto {
                            type_id: resource,
                            name: catalogue.name(resource),
                            density_pct: entry.and_then(|entry| entry.density_pct).map(fmt),
                            head_value: entry.and_then(|entry| entry.head_value).map(fmt),
                            updated_at: entry.map(|entry| entry.updated_at),
                            default_density_pct: fmt_f64(default_pct, 2),
                        }
                    })
                    .collect();
                // The in-game scan list is alphabetical.
                resources.sort_by(|a, b| a.name.cmp(&b.name));
                let settings = area.settings.get(&planet.planet_id);
                PlannerPlanetDto {
                    planet_id: planet.planet_id,
                    name: planet.name.clone(),
                    solar_system_id: planet.solar_system_id,
                    jumps: system.map_or(0, |system| system.jumps),
                    type_id: planet.type_id,
                    type_name: type_names
                        .get(&planet.type_id)
                        .cloned()
                        .or_else(|| planet_type_name(planet.type_id).map(str::to_string))
                        .unwrap_or_else(|| format!("Type {}", planet.type_id)),
                    customs_rate: settings.and_then(|settings| settings.customs_rate).map(fmt),
                    contested: settings.is_some_and(|settings| settings.contested),
                    settings_updated_at: settings.map(|settings| settings.updated_at),
                    resources,
                }
            })
            .collect();
        let systems = area
            .systems
            .iter()
            .map(|system| PlannerSystemDto {
                solar_system_id: system.solar_system_id,
                name: system.name.clone(),
                security_status: system
                    .security_status
                    .map(|security| security.round_dp(2))
                    .map(fmt),
                region_name: system.region_name.clone(),
                band: band(system).into(),
                jumps: system.jumps,
            })
            .collect();
        Ok(PlannerPlanetsDto { systems, planets })
    }

    pub async fn save_densities(
        &self,
        workspace_id: WorkspaceId,
        writes: Vec<DensityWriteDto>,
    ) -> Result<(), PiPlannerError> {
        let writes = writes
            .into_iter()
            .map(|write| DensityWrite {
                planet_id: write.planet_id,
                resource_type_id: write.resource_type_id,
                density_pct: write.density_pct,
                head_value: write.head_value,
            })
            .collect::<Vec<_>>();
        for write in &writes {
            write.validate()?;
        }
        Ok(self.inputs.save_densities(workspace_id, &writes).await?)
    }

    pub async fn save_planet_settings(
        &self,
        workspace_id: WorkspaceId,
        writes: Vec<PlanetSettingsWriteDto>,
    ) -> Result<(), PiPlannerError> {
        let writes = writes
            .into_iter()
            .map(|write| PlanetSettingsWrite {
                planet_id: write.planet_id,
                customs_rate: write.customs_rate,
                contested: write.contested,
            })
            .collect::<Vec<_>>();
        for write in &writes {
            write.validate()?;
        }
        Ok(self
            .inputs
            .save_planet_settings(workspace_id, &writes)
            .await?)
    }

    /// Connected characters' PI skills, for prefilling a scenario.
    pub async fn characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlannerCharacterSkillsDto>, PiPlannerError> {
        let Some(source) = &self.characters else {
            return Ok(Vec::new());
        };
        Ok(source
            .planetary_characters(workspace_id)
            .await?
            .into_iter()
            .map(|character| {
                let summary = character.skills_summary.as_ref();
                PlannerCharacterSkillsDto {
                    eve_character_id: character.eve_character_id,
                    name: character.name,
                    interplanetary_consolidation: current_trained_level(
                        summary,
                        INTERPLANETARY_CONSOLIDATION_SKILL_ID,
                    ),
                    command_center_upgrades: current_trained_level(
                        summary,
                        COMMAND_CENTER_UPGRADES_SKILL_ID,
                    ),
                }
            })
            .collect())
    }

    /// Every product the planets in range support, best ISK/month first.
    pub async fn rank(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        request: PlannerScenarioDto,
    ) -> Result<PlannerRankingDto, PiPlannerError> {
        let Prepared {
            scenario,
            catalogue,
            area,
            candidates,
            prices,
            price_observed_at,
        } = self.prepare(workspace_id, scope, &request).await?;

        let ranking = rank(&catalogue.catalogue, &scenario, &candidates, &prices);
        let planet_name = |planet_id: i64| area.planet_name(planet_id);
        let ranked = ranking
            .ranked
            .into_iter()
            .map(|product| RankedProductDto {
                type_id: product.type_id,
                name: catalogue.name(product.type_id),
                tier: product.tier,
                isk_per_month: fmt_f64(product.isk_per_month, 2),
                units_per_hour: fmt_f64(product.units_per_hour, 2),
                colonies: product.colonies,
                chains: product.chains,
                inputs: product
                    .inputs
                    .into_iter()
                    .map(|input| ranked_input(&catalogue, input, &planet_name))
                    .collect(),
                bought: product
                    .bought
                    .into_iter()
                    .map(|type_id| NamedTypeDto {
                        type_id,
                        name: catalogue.name(type_id),
                    })
                    .collect(),
                confidence: fmt_f64(product.confidence, 3),
                effort: effort_dto(&product.effort),
                haul_constrained_days: product.haul_constrained_days,
            })
            .collect();
        let out_of_reach = ranking
            .out_of_reach
            .into_iter()
            .map(|(type_id, tier, reason)| OutOfReachDto {
                type_id,
                name: catalogue.name(type_id),
                tier,
                reason: match reason {
                    OutOfReach::MissingResource { type_id } => {
                        OutOfReachReasonDto::MissingResource {
                            type_id,
                            name: catalogue.name(type_id),
                        }
                    }
                    OutOfReach::NoFactoryPlanet => OutOfReachReasonDto::NoFactoryPlanet,
                    OutOfReach::NeedsColonies { needed, available } => {
                        OutOfReachReasonDto::NeedsColonies { needed, available }
                    }
                    OutOfReach::OverflowsSkyhook => OutOfReachReasonDto::OverflowsSkyhook,
                    OutOfReach::NoRoom => OutOfReachReasonDto::NoRoom,
                    OutOfReach::NoPrice => OutOfReachReasonDto::NoPrice,
                },
            })
            .collect();
        Ok(PlannerRankingDto {
            price_observed_at,
            available_colonies: scenario.available_colonies(),
            ranked,
            out_of_reach,
        })
    }

    /// The scenario, catalogue, planets in range and prices a rank or a
    /// solve works from.
    async fn prepare(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        request: &PlannerScenarioDto,
    ) -> Result<Prepared, PiPlannerError> {
        let scenario = scenario(request)?;
        let catalogue = self.catalogue().await?;
        let area = self
            .neighbourhood(workspace_id, request.home_system_id, request.radius)
            .await?;
        let candidates = candidates(&area);

        let products: Vec<i64> = catalogue
            .catalogue
            .recipes()
            .map(|recipe| recipe.output_type_id)
            .collect();
        let books = self
            .market
            .scoped_order_books(workspace_id, scope, &products)
            .await?;
        let mut prices = Prices::default();
        let mut price_observed_at = None;
        for (type_id, orders) in &books {
            let data = summarize_scoped_orders(orders);
            if let Some(best_buy) = data.best_buy {
                prices.best_buy.insert(*type_id, to_f64(best_buy.0));
            }
            if let Some(best_sell) = data.best_sell {
                prices.best_sell.insert(*type_id, to_f64(best_sell.0));
            }
            price_observed_at = price_observed_at.max(data.observed_at);
        }

        Ok(Prepared {
            scenario,
            catalogue,
            area,
            candidates,
            prices,
            price_observed_at,
        })
    }
}

fn scenario(request: &PlannerScenarioDto) -> Result<RankScenario, PiPlannerError> {
    if request.characters.is_empty() {
        return Err(invalid("at least one character is required"));
    }
    if request.characters.iter().any(|character| {
        character.interplanetary_consolidation > 5 || character.command_center_upgrades > 5
    }) {
        return Err(invalid("skill levels must be 0 to 5"));
    }
    if !INTERVAL_DAYS.contains(&request.reset_days) || !INTERVAL_DAYS.contains(&request.haul_days) {
        return Err(invalid(
            "resetDays and haulDays must be 1, 2, 3, 4, 7 or 14",
        ));
    }
    if request.default_customs_rate < Decimal::ZERO || request.default_customs_rate > Decimal::ONE {
        return Err(invalid("defaultCustomsRate must be between 0 and 1"));
    }
    if request.max_colonies == Some(0) {
        return Err(invalid("maxColonies must be at least 1"));
    }
    Ok(RankScenario {
        characters: request
            .characters
            .iter()
            .map(|character| PlannerCharacter {
                interplanetary_consolidation: character.interplanetary_consolidation,
                command_center_upgrades: character.command_center_upgrades,
            })
            .collect(),
        effort: EffortSettings {
            reset_days: request.reset_days,
            haul_days: request.haul_days,
            uplift_at_login: request.uplift_at_login,
        },
        default_customs_rate: to_f64(request.default_customs_rate),
        buy_intermediates: request.buy_intermediates,
        max_colonies: request.max_colonies,
    })
}

fn candidates(area: &Neighbourhood) -> Vec<CandidatePlanet> {
    area.planets
        .iter()
        .map(|planet| {
            let system = area.system(planet.solar_system_id);
            let settings = area.settings.get(&planet.planet_id);
            CandidatePlanet {
                planet_id: planet.planet_id,
                type_id: planet.type_id,
                radius_m: planet.radius_m,
                jumps: system.map_or(0, |system| system.jumps),
                band: system.map_or(SecurityBand::Null, band),
                customs_rate: settings
                    .and_then(|settings| settings.customs_rate)
                    .map(to_f64),
                contested: settings.is_some_and(|settings| settings.contested),
                densities: area
                    .densities
                    .range((planet.planet_id, i64::MIN)..=(planet.planet_id, i64::MAX))
                    .map(|((_, resource), entry)| {
                        (
                            *resource,
                            DensityEntry {
                                density_pct: entry.density_pct.map(to_f64),
                                head_value: entry.head_value.map(to_f64),
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

mod solve;

pub use solve::{traded_types, PlannerSolutionDto};

#[cfg(test)]
mod tests;
