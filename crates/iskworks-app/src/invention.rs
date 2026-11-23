//! Read model for the Invention page: for a product whose blueprint can be
//! invented, the chance, the resulting copy and the costs of "no decryptor"
//! and of every decryptor, side by side.
//!
//! Nothing is written except the usual evidence registrations (a facility's
//! solar system for cost indices, adjusted prices). The rules live in
//! `iskworks_core::invention`; this gathers what they need from the SDE, the
//! market, the cost indices and a character's synced skills.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::invention::{
    invention_chance, invention_duration_seconds, quote, science_job_fee, skill_chance_factor,
    DecryptorEffect, InventedCopy, InventionOption, InventionSkills, ManufacturingCostByCopy,
    QuoteDecryptor, QuoteInputs, QuoteMaterial, ScienceJobFee, ScienceJobRates,
};
use iskworks_core::skills::{parse_skills_summary, CharacterSkillsSnapshot};
use iskworks_core::{
    calculate_adjusted_price_eiv, preview_blueprint_effects, preview_facility,
    summarize_scoped_orders, AdjustedPriceRepository, CapturedRecipe, ConnectedCharacterId,
    EffectiveMaterialRequirement, FacilityError, FacilityProfileId, FacilityRole, IndustryActivity,
    IndustryError, IndustryFacilityProfile, IndustryRepository, MarketRepository, MarketScope,
    Money, ProductClassification, SkillsCharacterSource, StoreError, WorkspaceId,
};
use iskworks_sde::{
    InventableProduct, InventionProduct, InventionRecipe, RequiredSkill, SdeReadRepository,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// The skill that shortens every science job: 3% per level.
const ADVANCED_INDUSTRY_SKILL_ID: i64 = 3_388;
/// The level assumed for every skill when no character is chosen.
const DEFAULT_SKILL_LEVEL: u8 = 4;
const DEFAULT_TARGET_RUNS: u64 = 100;
const MAX_TARGET_RUNS: u64 = 1_000_000;
const SEARCH_LIMIT: u32 = 25;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionQuoteCommand {
    /// The item to build, not its blueprint.
    pub product_type_id: i64,
    pub character_id: Option<Uuid>,
    /// Levels to use in place of the character's, by skill type id.
    #[serde(default)]
    pub skill_overrides: BTreeMap<i64, u8>,
    pub invention_facility_id: Option<Uuid>,
    pub manufacturing_facility_id: Option<Uuid>,
    pub target_runs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionProductSearch {
    /// False when the active SDE import has no invention data (re-import).
    pub invention_data_available: bool,
    pub products: Vec<InventableProduct>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InventionSkillRole {
    /// Counts towards the chance at 1/40 per level.
    Encryption,
    /// A datacore's skill: counts at 1/30 per level.
    Science,
    /// Needed to start the job; does not change the chance.
    Requirement,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionSkillView {
    pub skill_type_id: i64,
    pub skill_name: String,
    pub role: InventionSkillRole,
    /// The level the job lists; `None` for a datacore skill it does not.
    pub required_level: Option<u8>,
    /// The character's level; `None` when no character was chosen.
    pub character_level: Option<u8>,
    /// The level the quote used.
    pub level: u8,
    pub overridden: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionOutputView {
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionQuoteView {
    pub product_type_id: i64,
    pub product_name: String,
    pub blueprint_type_id: i64,
    pub source_blueprint_type_id: i64,
    pub source_blueprint_name: String,
    /// Every blueprint the source can yield. More than one means the player
    /// picks; the chance and the inputs are the same for each.
    pub outputs: Vec<InventionOutputView>,
    #[serde(with = "rust_decimal::serde::str")]
    pub base_chance: Decimal,
    pub base_runs: u64,
    pub skills: Vec<InventionSkillView>,
    /// True when no character was chosen and every level defaulted to 4.
    pub skills_defaulted: bool,
    #[serde(with = "rust_decimal::serde::str")]
    pub skill_factor: Decimal,
    /// The chance with skills and no decryptor.
    #[serde(with = "rust_decimal::serde::str")]
    pub chance_with_skills: Decimal,
    /// Skills the job lists that are below its level. Empty when it can
    /// start. The quote is worked out either way.
    pub missing_skills: Vec<String>,
    pub materials: Vec<QuoteMaterial>,
    /// Datacores for one attempt; `None` when one has no price.
    pub materials_cost: Option<Money>,
    pub job_fee: ScienceJobFee,
    /// One run of the Tech I blueprint copy.
    pub copy_fee: ScienceJobFee,
    /// With Advanced Industry only; structure bonuses are not applied.
    pub invention_duration_seconds: Option<u64>,
    pub target_runs: u64,
    pub output_per_run: u64,
    /// False when no manufacturing facility was chosen, so the unit cost
    /// has materials only.
    pub manufacturing_installation_included: bool,
    pub market_scope: MarketScope,
    pub prices_observed_at: Option<DateTime<Utc>>,
    pub options: Vec<InventionOption>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionQuoteResponse {
    /// False when the active SDE import has no invention data (re-import).
    pub invention_data_available: bool,
    pub quote: Option<InventionQuoteView>,
    /// The types the quote priced, for market coverage registration.
    #[serde(skip)]
    pub priced_types: BTreeMap<i64, String>,
}

#[derive(Clone)]
pub struct InventionService {
    sde: Arc<dyn SdeReadRepository>,
    industry: Arc<dyn IndustryRepository>,
    market: Arc<dyn MarketRepository>,
    adjusted_prices: Arc<dyn AdjustedPriceRepository>,
    characters: Option<Arc<dyn SkillsCharacterSource>>,
}

impl InventionService {
    #[must_use]
    pub fn new(
        sde: Arc<dyn SdeReadRepository>,
        industry: Arc<dyn IndustryRepository>,
        market: Arc<dyn MarketRepository>,
        adjusted_prices: Arc<dyn AdjustedPriceRepository>,
    ) -> Self {
        Self {
            sde,
            industry,
            market,
            adjusted_prices,
            characters: None,
        }
    }

    #[must_use]
    pub fn with_characters(mut self, characters: Arc<dyn SkillsCharacterSource>) -> Self {
        self.characters = Some(characters);
        self
    }

    pub async fn search_products(&self, query: &str) -> Result<InventionProductSearch, StoreError> {
        Ok(InventionProductSearch {
            invention_data_available: self.sde.has_invention_data().await.map_err(storage)?,
            products: self
                .sde
                .search_inventable_products(query, SEARCH_LIMIT)
                .await
                .map_err(storage)?,
        })
    }

    /// `NotFound` when the product's blueprint cannot be invented, or a
    /// named character or facility is not in the workspace.
    pub async fn quote(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
        command: InventionQuoteCommand,
        now: DateTime<Utc>,
    ) -> Result<InventionQuoteResponse, StoreError> {
        if !self.sde.has_invention_data().await.map_err(storage)? {
            return Ok(InventionQuoteResponse {
                invention_data_available: false,
                quote: None,
                priced_types: BTreeMap::new(),
            });
        }
        let target_runs = command.target_runs.unwrap_or(DEFAULT_TARGET_RUNS);
        if !(1..=MAX_TARGET_RUNS).contains(&target_runs) {
            return Err(StoreError::Validation(format!(
                "Target runs must be between 1 and {MAX_TARGET_RUNS}."
            )));
        }
        if command.skill_overrides.values().any(|level| *level > 5) {
            return Err(StoreError::Validation(
                "A skill level must be between 0 and 5.".to_string(),
            ));
        }

        let blueprint_type_id = self
            .sde
            .manufacturing_blueprint_for_product(command.product_type_id)
            .await
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let recipe = self
            .sde
            .invention_recipe(blueprint_type_id)
            .await
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let product = recipe
            .products
            .iter()
            .find(|product| product.blueprint_type_id == blueprint_type_id)
            .cloned()
            .ok_or(StoreError::InvalidData)?;
        let active = self
            .sde
            .active_sde()
            .await
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let manufacturing_recipe = self
            .sde
            .manufacturing_recipe(blueprint_type_id)
            .await
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let manufacturing = CapturedRecipe::capture(
            active.import_id,
            active.source_version.clone(),
            manufacturing_recipe,
        )
        .map_err(storage)?;
        // The copy fee is based on what the source blueprint builds.
        let source_manufacturing = self
            .sde
            .manufacturing_recipe(recipe.source_blueprint_type_id)
            .await
            .map_err(storage)?
            .and_then(|recipe| {
                CapturedRecipe::capture(active.import_id, active.source_version.clone(), recipe)
                    .ok()
            });
        let decryptors = self.sde.decryptors().await.map_err(storage)?;

        // Skills.
        let snapshot = match command.character_id {
            Some(character_id) => Some(
                self.character_snapshot(workspace_id, ConnectedCharacterId(character_id))
                    .await?,
            ),
            None => None,
        };
        let material_ids = recipe
            .materials
            .iter()
            .map(|line| line.type_id)
            .collect::<Vec<_>>();
        let datacore_skills = self
            .sde
            .required_skills(&material_ids)
            .await
            .map_err(storage)?;
        let science_skill_ids = science_skill_ids(&recipe, &datacore_skills);
        let skill_names = self
            .sde
            .type_names(&science_skill_ids)
            .await
            .map_err(storage)?;
        let skills = resolve_skills(
            &recipe,
            &science_skill_ids,
            &skill_names,
            snapshot.as_ref(),
            &command.skill_overrides,
        );
        let advanced_industry = snapshot.as_ref().map_or(DEFAULT_SKILL_LEVEL, |snapshot| {
            character_level(snapshot, ADVANCED_INDUSTRY_SKILL_ID)
        });

        // Facilities and their cost indices.
        let invention_facility = match command.invention_facility_id {
            Some(id) => Some(self.facility(workspace_id, id).await?),
            None => None,
        };
        let mut manufacturing_facility = match command.manufacturing_facility_id {
            Some(id) => {
                let profile = self.facility(workspace_id, id).await?;
                if profile.role != FacilityRole::Manufacturing {
                    return Err(StoreError::Validation(
                        "The manufacturing facility must be a manufacturing profile.".to_string(),
                    ));
                }
                Some(profile)
            }
            None => None,
        };
        let science_system = invention_facility
            .as_ref()
            .and_then(|profile| profile.solar_system_id);
        let (invention_index, copying_index) = match science_system {
            Some(solar_system_id) => {
                self.adjusted_prices
                    .register_system_cost_index(solar_system_id, now)
                    .await?;
                (
                    self.adjusted_prices
                        .latest_system_cost_index_for(solar_system_id, IndustryActivity::Invention)
                        .await?
                        .map(|(index, _)| index),
                    self.adjusted_prices
                        .latest_system_cost_index_for(solar_system_id, IndustryActivity::Copying)
                        .await?
                        .map(|(index, _)| index),
                )
            }
            None => (None, None),
        };
        if let Some(profile) = manufacturing_facility.as_mut() {
            if profile.manual_system_cost_index.is_none() {
                if let Some(solar_system_id) = profile.solar_system_id {
                    self.adjusted_prices
                        .register_system_cost_index(solar_system_id, now)
                        .await?;
                    profile.manual_system_cost_index = self
                        .adjusted_prices
                        .latest_system_cost_index(solar_system_id)
                        .await?
                        .map(|(index, _)| index);
                }
            }
        }

        // Adjusted prices, for the EIV of the output and of the source.
        self.adjusted_prices
            .register_adjusted_price_refresh(now)
            .await?;
        let eiv_type_ids = manufacturing
            .materials
            .iter()
            .chain(
                source_manufacturing
                    .iter()
                    .flat_map(|recipe| &recipe.materials),
            )
            .map(|line| line.type_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let adjusted = self
            .adjusted_prices
            .latest_adjusted_prices(&eiv_type_ids, now)
            .await?;
        let eiv = |recipe: &CapturedRecipe, runs: u64| {
            calculate_adjusted_price_eiv(&recipe.materials, runs, &adjusted)
                .ok()
                .and_then(|eiv| eiv.value)
        };
        let rates = |index: Option<Decimal>| match &invention_facility {
            Some(profile) => ScienceJobRates::from_profile(profile, index),
            None => ScienceJobRates {
                system_cost_index: None,
                facility_tax_percent: Decimal::ZERO,
                scc_surcharge_percent: Decimal::ZERO,
                alliance_surcharge_percent: Decimal::ZERO,
            },
        };
        let job_fee = science_job_fee(eiv(&manufacturing, 1), 1, rates(invention_index));
        let copy_fee = science_job_fee(
            source_manufacturing
                .as_ref()
                .and_then(|recipe| eiv(recipe, 1)),
            1,
            rates(copying_index),
        );

        // Market prices: what an attempt consumes and what a run consumes.
        let mut priced_types = BTreeMap::new();
        for line in &recipe.materials {
            priced_types.insert(line.type_id, line.type_name.clone());
        }
        for decryptor in &decryptors {
            priced_types.insert(decryptor.type_id, decryptor.name.clone());
        }
        for line in &manufacturing.materials {
            priced_types.insert(line.type_id, line.type_name.clone());
        }
        let type_ids = priced_types.keys().copied().collect::<Vec<_>>();
        let books = self
            .market
            .scoped_order_books(workspace_id, scope, &type_ids)
            .await
            .map_err(storage)?;
        let mut best_sell = BTreeMap::new();
        let mut prices_observed_at = None;
        for (type_id, orders) in &books {
            let summary = summarize_scoped_orders(orders);
            if let Some(price) = summary.best_sell {
                best_sell.insert(*type_id, price);
                prices_observed_at = prices_observed_at.max(summary.observed_at);
            }
        }

        let quote_decryptors = decryptors
            .into_iter()
            .map(|decryptor| QuoteDecryptor {
                unit_price: best_sell.get(&decryptor.type_id).copied(),
                type_id: decryptor.type_id,
                name: decryptor.name,
                effect: DecryptorEffect {
                    probability_multiplier: decryptor.probability_multiplier,
                    me_modifier: decryptor.me_modifier,
                    te_modifier: decryptor.te_modifier,
                    run_modifier: decryptor.run_modifier,
                },
            })
            .collect::<Vec<_>>();
        let base_runs = u64::try_from(product.base_runs).map_err(|_| StoreError::InvalidData)?;
        let copies = std::iter::once(DecryptorEffect::NONE)
            .chain(quote_decryptors.iter().map(|decryptor| decryptor.effect))
            .map(|effect| iskworks_core::invention::invented_copy(base_runs, &effect))
            .collect::<Vec<_>>();
        let classification = self
            .sde
            .type_classifications(&[command.product_type_id])
            .await
            .map_err(storage)?
            .remove(&command.product_type_id)
            .unwrap_or_default();
        let product_class = ProductClassification {
            category_id: classification.category_id,
            group_id: classification.group_id,
        };
        let mut manufacturing_cost_by_copy = ManufacturingCostByCopy::new();
        for copy in copies {
            let key = (copy.material_efficiency, copy.runs);
            if manufacturing_cost_by_copy.contains_key(&key) {
                continue;
            }
            let cost = manufacturing_cost_per_run(
                &manufacturing,
                copy,
                manufacturing_facility.as_ref(),
                product_class,
                eiv(&manufacturing, copy.runs),
                &best_sell,
            )?;
            manufacturing_cost_by_copy.insert(key, cost);
        }

        let materials = recipe
            .materials
            .iter()
            .map(|line| {
                Ok(QuoteMaterial {
                    type_id: line.type_id,
                    type_name: line.type_name.clone(),
                    quantity: u64::try_from(line.quantity).map_err(|_| StoreError::InvalidData)?,
                    unit_price: best_sell.get(&line.type_id).copied(),
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let materials_cost = materials
            .iter()
            .try_fold(Decimal::ZERO, |sum, material| {
                sum.checked_add(
                    material
                        .unit_price?
                        .0
                        .checked_mul(Decimal::from(material.quantity))?,
                )
            })
            .map(Money);
        let output_per_run = manufacturing
            .products
            .first()
            .map_or(1, |line| line.quantity_per_run);
        let inputs = QuoteInputs {
            base_chance: product.probability,
            base_runs,
            skills: skills.levels,
            materials: materials.clone(),
            decryptors: quote_decryptors,
            job_fee: job_fee.total,
            copy_cost: copy_fee.total,
            manufacturing_cost_by_copy,
            output_per_run,
            target_runs,
            confidence: Decimal::new(9, 1),
        };
        let options = quote(&inputs).options;

        Ok(InventionQuoteResponse {
            invention_data_available: true,
            quote: Some(InventionQuoteView {
                product_type_id: command.product_type_id,
                product_name: product
                    .product_name
                    .clone()
                    .unwrap_or_else(|| product.blueprint_name.clone()),
                blueprint_type_id,
                source_blueprint_type_id: recipe.source_blueprint_type_id,
                source_blueprint_name: recipe.source_blueprint_name.clone(),
                outputs: recipe.products.iter().map(output_view).collect(),
                base_chance: product.probability,
                base_runs,
                skill_factor: skill_chance_factor(skills.levels),
                chance_with_skills: invention_chance(
                    product.probability,
                    skills.levels,
                    &DecryptorEffect::NONE,
                ),
                skills_defaulted: snapshot.is_none(),
                missing_skills: skills.missing,
                skills: skills.views,
                materials,
                materials_cost,
                job_fee,
                copy_fee,
                invention_duration_seconds: recipe
                    .duration_seconds
                    .and_then(|seconds| u64::try_from(seconds).ok())
                    .map(|seconds| invention_duration_seconds(seconds, advanced_industry)),
                target_runs,
                output_per_run,
                manufacturing_installation_included: manufacturing_facility.is_some(),
                market_scope: scope,
                prices_observed_at,
                options,
            }),
            priced_types,
        })
    }

    async fn facility(
        &self,
        workspace_id: WorkspaceId,
        id: Uuid,
    ) -> Result<IndustryFacilityProfile, StoreError> {
        let profile = self
            .industry
            .get_facility_profile(workspace_id, FacilityProfileId(id))
            .await
            .map_err(|error| match error {
                IndustryError::Facility(FacilityError::NotFound) => StoreError::NotFound,
                error => storage(error),
            })?;
        if profile.workspace_id != workspace_id {
            return Err(StoreError::NotFound);
        }
        if profile.archived_at.is_some() {
            return Err(StoreError::Validation(
                "That facility profile is archived.".to_string(),
            ));
        }
        Ok(profile)
    }

    async fn character_snapshot(
        &self,
        workspace_id: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<CharacterSkillsSnapshot, StoreError> {
        let character = match &self.characters {
            Some(characters) => {
                characters
                    .skills_character(workspace_id, connection_id)
                    .await?
            }
            None => None,
        }
        .ok_or(StoreError::NotFound)?;
        character
            .skills_scope_granted
            .then_some(())
            .and(character.skills_summary.as_ref())
            .and_then(parse_skills_summary)
            .ok_or_else(|| {
                StoreError::Validation(format!("{} has no synced skills.", character.name))
            })
    }
}

fn storage(error: impl std::fmt::Display) -> StoreError {
    StoreError::Persistence(error.to_string())
}

fn output_view(product: &InventionProduct) -> InventionOutputView {
    InventionOutputView {
        product_type_id: product.product_type_id,
        product_name: product.product_name.clone(),
        blueprint_type_id: product.blueprint_type_id,
        blueprint_name: product.blueprint_name.clone(),
    }
}

/// The level a character can use: active, so an Alpha's cap applies. A
/// skill it does not have is level 0.
fn character_level(snapshot: &CharacterSkillsSnapshot, skill_type_id: i64) -> u8 {
    snapshot
        .skill(skill_type_id)
        .map_or(0, |skill| skill.active_level.clamp(0, 5) as u8)
}

/// An encryption skill is recognised by its name; the SDE has no flag.
fn is_encryption_skill(name: &str) -> bool {
    name.contains("Encryption Methods")
}

/// The two skills that count at 1/30 per level: each datacore's own
/// required skill, in material order. When a datacore names none, the
/// job's remaining non-encryption skills fill in.
fn science_skill_ids(
    recipe: &InventionRecipe,
    datacore_skills: &BTreeMap<i64, Vec<RequiredSkill>>,
) -> Vec<i64> {
    let mut ids = recipe
        .materials
        .iter()
        .filter_map(|line| datacore_skills.get(&line.type_id)?.first())
        .map(|skill| skill.skill_type_id)
        .collect::<Vec<_>>();
    ids.dedup();
    for skill in &recipe.skills {
        if ids.len() >= 2 {
            break;
        }
        if !is_encryption_skill(&skill.skill_name) && !ids.contains(&skill.skill_type_id) {
            ids.push(skill.skill_type_id);
        }
    }
    ids.truncate(2);
    ids
}

struct ResolvedSkills {
    views: Vec<InventionSkillView>,
    levels: InventionSkills,
    missing: Vec<String>,
}

/// Works out each skill's level (override, else the character's, else 4),
/// its part in the chance, and which listed skills are below the job's
/// level. Encryption first, then the science skills, then requirements.
fn resolve_skills(
    recipe: &InventionRecipe,
    science_skill_ids: &[i64],
    skill_names: &BTreeMap<i64, String>,
    snapshot: Option<&CharacterSkillsSnapshot>,
    overrides: &BTreeMap<i64, u8>,
) -> ResolvedSkills {
    let required = |skill_type_id: i64| {
        recipe
            .skills
            .iter()
            .find(|skill| skill.skill_type_id == skill_type_id)
    };
    let view = |skill_type_id: i64, role: InventionSkillRole| {
        let character_level = snapshot.map(|snapshot| character_level(snapshot, skill_type_id));
        let overridden = overrides.contains_key(&skill_type_id);
        InventionSkillView {
            skill_type_id,
            skill_name: required(skill_type_id)
                .map(|skill| skill.skill_name.clone())
                .or_else(|| skill_names.get(&skill_type_id).cloned())
                .unwrap_or_else(|| format!("Skill {skill_type_id}")),
            role,
            required_level: required(skill_type_id).map(|skill| skill.level.clamp(0, 5) as u8),
            character_level,
            level: overrides
                .get(&skill_type_id)
                .copied()
                .or(character_level)
                .unwrap_or(DEFAULT_SKILL_LEVEL)
                .min(5),
            overridden,
        }
    };

    let encryption = recipe
        .skills
        .iter()
        .find(|skill| is_encryption_skill(&skill.skill_name))
        .map(|skill| skill.skill_type_id);
    let mut views = Vec::new();
    views.extend(encryption.map(|id| view(id, InventionSkillRole::Encryption)));
    views.extend(
        science_skill_ids
            .iter()
            .map(|id| view(*id, InventionSkillRole::Science)),
    );
    for skill in &recipe.skills {
        if !views
            .iter()
            .any(|view| view.skill_type_id == skill.skill_type_id)
        {
            views.push(view(skill.skill_type_id, InventionSkillRole::Requirement));
        }
    }

    let level_of = |role: InventionSkillRole, index: usize| {
        views
            .iter()
            .filter(|view| view.role == role)
            .nth(index)
            .map_or(0, |view| view.level)
    };
    let levels = InventionSkills {
        encryption: level_of(InventionSkillRole::Encryption, 0),
        science: [
            level_of(InventionSkillRole::Science, 0),
            level_of(InventionSkillRole::Science, 1),
        ],
    };
    let missing = views
        .iter()
        .filter(|view| view.required_level.is_some_and(|level| view.level < level))
        .map(|view| view.skill_name.clone())
        .collect();
    ResolvedSkills {
        views,
        levels,
        missing,
    }
}

/// One manufacturing run's cost, as a share of a job over all the copy's
/// runs: every input at its best sell price, plus installation when a
/// facility was chosen. `None` when a price or the installation cost is
/// unknown.
fn manufacturing_cost_per_run(
    recipe: &CapturedRecipe,
    copy: InventedCopy,
    facility: Option<&IndustryFacilityProfile>,
    product: ProductClassification,
    eiv: Option<Money>,
    best_sell: &BTreeMap<i64, Money>,
) -> Result<Option<Money>, StoreError> {
    let validation = |error: FacilityError| StoreError::Validation(error.to_string());
    let (requirements, installation): (Vec<EffectiveMaterialRequirement>, Option<Money>) =
        match facility {
            Some(profile) => {
                let preview = preview_facility(
                    recipe,
                    copy.runs,
                    profile.clone(),
                    product,
                    copy.material_efficiency,
                    copy.time_efficiency,
                    eiv,
                    None,
                )
                .map_err(validation)?;
                (preview.requirements, preview.installation_cost.total)
            }
            None => (
                preview_blueprint_effects(
                    recipe,
                    copy.runs,
                    copy.material_efficiency,
                    copy.time_efficiency,
                    None,
                )
                .map_err(validation)?
                .0,
                Some(Money::zero()),
            ),
        };
    let materials = requirements.iter().try_fold(Decimal::ZERO, |sum, line| {
        sum.checked_add(
            best_sell
                .get(&line.type_id)?
                .0
                .checked_mul(Decimal::from(line.final_required_quantity))?,
        )
    });
    Ok(materials
        .zip(installation)
        .and_then(|(materials, installation)| materials.checked_add(installation.0))
        .and_then(|total| total.checked_div(Decimal::from(copy.runs)))
        .map(|per_run| Money(per_run.round_dp(4))))
}
