//! Saved fits for the Fittings page: stored per workspace, named and grouped
//! from the active SDE import, read from and written to EFT text through the
//! fitting engine, calculated for a pilot, and checked against every
//! workspace character's skills (who can fly it, and what they lack).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::fitting::{
    evaluate_flyability, fit_requirements, requirement_plan, CapacitorState, CatalogueEntry,
    CatalogueKind, DamageProfile, EftError, EftLine, ExplainTarget, Fit, FitCalculation,
    FitCatalogue, FitDraft, FitId, FitInput, FitItem, FitRepository, FitRequirements,
    FittingEngine, FittingError, FlyabilityStatus, Hardpoint, Pilot, PilotFlyability, PilotSkills,
    SlotKind, SourceRef,
};
use iskworks_core::skill_plans::{SkillPlan, SkillPlanInput, SkillPlanRepository};
use iskworks_core::skills::{parse_skills_summary, CharacterSkillsSnapshot};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{RequiredSkill, SdeReadRepository, SkillDefinition, TypeReference};
use serde::Serialize;

use crate::{SkillsCharacter, SkillsCharacterSource};

/// A type as the page shows it, named from the active SDE import. `name`
/// and `group_name` are `None` for a type the import doesn't have.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitType {
    pub type_id: i64,
    pub name: Option<String>,
    pub group_name: Option<String>,
}

/// One fit in the list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitSummary {
    pub id: FitId,
    pub name: String,
    pub ship: FitType,
    pub item_count: usize,
    pub updated_at: DateTime<Utc>,
    /// Every workspace character and whether it can fly the fit.
    pub pilots: Vec<FitListPilot>,
}

/// One character on the fit list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitListPilot {
    pub connection_id: ConnectedCharacterId,
    pub name: String,
    /// `None` when its skills aren't synced (or the scope is missing).
    pub status: Option<FlyabilityStatus>,
}

/// Whose skills a calculation uses.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PilotChoice {
    AllV,
    AllZero,
    Character(ConnectedCharacterId),
}

/// A calculation, and for a character, what it lacks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitStatsView {
    #[serde(flatten)]
    pub calculation: FitCalculation,
    /// Only for a character pilot.
    pub flyability: Option<PilotFlyability>,
}

/// One modifier, with its source named from the SDE import (skills and
/// fitted items; the ship; nothing for a buff).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainedSource {
    pub from: SourceRef,
    pub name: Option<String>,
    pub operator: String,
    pub value: f64,
    pub penalty: Option<f64>,
    pub applied: bool,
}

/// How one attribute got from its base to its final value.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeView {
    pub base: f64,
    pub value: f64,
    pub sources: Vec<ExplainedSource>,
}

/// What the editor's module browser asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueQuery {
    /// Modules a ship can fit, optionally in one rack.
    Modules {
        ship_type_id: i64,
        slot: Option<SlotKind>,
    },
    /// Charges a module loads.
    Charges {
        module_type_id: i64,
    },
    Drones,
    Ships,
}

/// One fittable type, as the module browser lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueItem {
    pub type_id: i64,
    pub name: String,
    pub kind: CatalogueKind,
    pub slot: Option<SlotKind>,
    pub cpu: f64,
    pub powergrid: f64,
    pub calibration: f64,
    pub hardpoint: Option<Hardpoint>,
    pub rig_size: Option<i64>,
    pub charge_size: Option<i64>,
    /// Market groups from the root down.
    pub market_path: Vec<String>,
    /// For a character pilot, whether its skills cover the type's
    /// requirements; `None` for All V / All 0.
    pub can_use: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueView {
    pub entries: Vec<CatalogueItem>,
    /// More matched than are listed; narrow the search.
    pub truncated: bool,
}

/// The most a catalogue answer lists.
const CATALOGUE_LIMIT: usize = 200;

/// The numbers the pilots table compares.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotHeadline {
    pub ehp: f64,
    pub dps: f64,
    pub capacitor: CapacitorState,
    pub max_velocity: f64,
    pub align_seconds: f64,
}

/// One row of the pilots table.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitPilot {
    pub connection_id: ConnectedCharacterId,
    pub name: String,
    /// `None` when its skills aren't synced (or the scope is missing).
    pub status: Option<FlyabilityStatus>,
    pub flyability: Option<PilotFlyability>,
    /// With this character's skills; `None` without skills or without the
    /// fitting engine.
    pub stats: Option<PilotHeadline>,
}

/// A fitted or carried item, with its names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitItemView {
    #[serde(flatten)]
    pub item: FitItem,
    pub name: Option<String>,
    pub group_name: Option<String>,
    pub charge_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitDetail {
    pub id: FitId,
    pub name: String,
    pub description: String,
    pub ship: FitType,
    pub items: Vec<FitItemView>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A fit read from text, before it is saved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitPreview {
    /// What the fit would be saved as.
    pub name: String,
    pub ship: FitType,
    pub items: Vec<FitItemView>,
}

/// What importing text would make. Nothing is saved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitTextPreview {
    /// `None` when the text has unknown lines or isn't a fit.
    pub fit: Option<FitPreview>,
    pub unknown_lines: Vec<EftLine>,
    /// Why the text isn't a fit, when it isn't.
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct FittingService {
    fits: Arc<dyn FitRepository>,
    sde: Arc<dyn SdeReadRepository>,
    characters: Option<Arc<dyn SkillsCharacterSource>>,
    skill_plans: Option<Arc<dyn SkillPlanRepository>>,
}

/// A character's synced skills, when it has them.
fn snapshot(character: &SkillsCharacter) -> Option<CharacterSkillsSnapshot> {
    if !character.skills_scope_granted {
        return None;
    }
    character
        .skills_summary
        .as_ref()
        .and_then(parse_skills_summary)
}

/// The levels a character can use: active, so an Alpha's cap applies.
fn pilot_skills(snapshot: &CharacterSkillsSnapshot) -> Result<Pilot, InventoryError> {
    let levels = snapshot
        .skills
        .iter()
        .map(|skill| (skill.skill_id, skill.active_level))
        .collect();
    PilotSkills::new(levels)
        .map(Pilot::Skills)
        .map_err(|error| InventoryError::Validation(format!("unusable skill data: {error:?}")))
}

fn status_rank(status: Option<FlyabilityStatus>) -> u8 {
    match status {
        Some(FlyabilityStatus::CanUseAll) => 0,
        Some(FlyabilityStatus::CanFlyHull) => 1,
        Some(FlyabilityStatus::Cannot) => 2,
        None => 3,
    }
}

fn calculation_error(error: FittingError) -> InventoryError {
    InventoryError::Validation(match error {
        FittingError::UnknownShip(type_id) => {
            format!("the fitting engine doesn't know ship type {type_id}")
        }
        FittingError::UnknownAttribute => "no attribute of that name".to_string(),
        FittingError::UnknownItem(index) => format!("the fit has no item {index} to explain"),
        other => format!("the fit could not be calculated: {other:?}"),
    })
}

fn sde_error(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

/// `Ship fit` for a text that names no fit.
fn default_name(ship: &FitType) -> String {
    format!("{} fit", ship.name.as_deref().unwrap_or("Ship"))
}

fn unknown_lines_message(lines: &[EftLine]) -> String {
    let listed = lines
        .iter()
        .map(|line| format!("line {}: {}", line.line, line.text))
        .collect::<Vec<_>>()
        .join("; ");
    format!("these lines match no type ({listed})")
}

impl FittingService {
    #[must_use]
    pub fn new(fits: Arc<dyn FitRepository>, sde: Arc<dyn SdeReadRepository>) -> Self {
        Self {
            fits,
            sde,
            characters: None,
            skill_plans: None,
        }
    }

    /// The workspace's characters, for pilots and flyability.
    #[must_use]
    pub fn with_characters(mut self, characters: Arc<dyn SkillsCharacterSource>) -> Self {
        self.characters = Some(characters);
        self
    }

    /// Where skill plans made from a fit go.
    #[must_use]
    pub fn with_skill_plans(mut self, skill_plans: Arc<dyn SkillPlanRepository>) -> Self {
        self.skill_plans = Some(skill_plans);
        self
    }

    async fn workspace_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        match &self.characters {
            Some(characters) => characters.skills_characters(workspace_id).await,
            None => Ok(Vec::new()),
        }
    }

    /// What every type of `fits` requires, and the skills the closures need.
    async fn requirement_data(
        &self,
        fits: &[&FitInput],
    ) -> Result<(BTreeMap<i64, Vec<RequiredSkill>>, Vec<SkillDefinition>), InventoryError> {
        let ids: BTreeSet<i64> = fits
            .iter()
            .flat_map(|fit| {
                std::iter::once(fit.ship_type_id).chain(
                    fit.items
                        .iter()
                        .flat_map(|item| std::iter::once(item.type_id).chain(item.charge_type_id)),
                )
            })
            .collect();
        let required = self
            .sde
            .required_skills(&ids.into_iter().collect::<Vec<_>>())
            .await
            .map_err(sde_error)?;
        let definitions = self.sde.skill_catalogue().await.map_err(sde_error)?;
        Ok((required, definitions))
    }

    /// The pilot a calculation uses; a character also gets what it lacks.
    async fn resolve_pilot(
        &self,
        workspace_id: WorkspaceId,
        choice: PilotChoice,
    ) -> Result<(Pilot, Option<CharacterSkillsSnapshot>), InventoryError> {
        match choice {
            PilotChoice::AllV => Ok((Pilot::AllV, None)),
            PilotChoice::AllZero => Ok((Pilot::AllZero, None)),
            PilotChoice::Character(connection_id) => {
                let character = match &self.characters {
                    Some(characters) => {
                        characters
                            .skills_character(workspace_id, connection_id)
                            .await?
                    }
                    None => None,
                }
                .ok_or_else(|| {
                    InventoryError::Validation("no such character in the workspace".to_string())
                })?;
                let snapshot = snapshot(&character).ok_or_else(|| {
                    InventoryError::Validation(format!(
                        "{} has no synced skills to calculate with",
                        character.name
                    ))
                })?;
                Ok((pilot_skills(&snapshot)?, Some(snapshot)))
            }
        }
    }

    /// Calculates `fit` (saved or not) for `pilot`.
    pub async fn calculate(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        fit: &FitInput,
        pilot: PilotChoice,
        profile: &DamageProfile,
        now: DateTime<Utc>,
    ) -> Result<FitStatsView, InventoryError> {
        let (pilot, snapshot) = self.resolve_pilot(workspace_id, pilot).await?;
        let calculation = engine
            .calculate(fit, &pilot, profile)
            .map_err(calculation_error)?;
        let flyability = match snapshot {
            Some(snapshot) => {
                let (required, definitions) = self.requirement_data(&[fit]).await?;
                Some(evaluate_flyability(
                    &fit_requirements(fit, &required),
                    &definitions,
                    &snapshot,
                    now,
                ))
            }
            None => None,
        };
        Ok(FitStatsView {
            calculation,
            flyability,
        })
    }

    /// Fittable types for the editor: modules a ship fits, charges a module
    /// loads, drones or ships, matching `text`, capped at 200.
    pub async fn catalogue(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        query: CatalogueQuery,
        text: Option<&str>,
        pilot: PilotChoice,
    ) -> Result<CatalogueView, InventoryError> {
        let catalogue = engine.catalogue();
        let entries = match query {
            CatalogueQuery::Modules { ship_type_id, slot } => {
                let ship = catalogue
                    .entry(ship_type_id)
                    .filter(|entry| entry.kind == CatalogueKind::Ship)
                    .ok_or_else(|| {
                        InventoryError::Validation(format!("type {ship_type_id} is not a ship"))
                    })?;
                catalogue.modules_for(ship, slot, text)
            }
            CatalogueQuery::Charges { module_type_id } => {
                let module = catalogue
                    .entry(module_type_id)
                    .filter(|entry| entry.kind == CatalogueKind::Module)
                    .ok_or_else(|| {
                        InventoryError::Validation(format!("type {module_type_id} is not a module"))
                    })?;
                catalogue.charges_for(module)
            }
            CatalogueQuery::Drones => catalogue.drones(text),
            CatalogueQuery::Ships => catalogue.ships(text),
        };
        let truncated = entries.len() > CATALOGUE_LIMIT;
        let page: Vec<&CatalogueEntry> = entries.into_iter().take(CATALOGUE_LIMIT).collect();
        Ok(CatalogueView {
            entries: self
                .catalogue_items(workspace_id, &catalogue, &page, pilot)
                .await?,
            truncated,
        })
    }

    /// `type_id`'s variations (its family, itself included).
    pub async fn variations(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        type_id: i64,
        pilot: PilotChoice,
    ) -> Result<CatalogueView, InventoryError> {
        let catalogue = engine.catalogue();
        let family = catalogue.variations(type_id);
        Ok(CatalogueView {
            entries: self
                .catalogue_items(workspace_id, &catalogue, &family, pilot)
                .await?,
            truncated: false,
        })
    }

    async fn catalogue_items(
        &self,
        workspace_id: WorkspaceId,
        catalogue: &FitCatalogue,
        entries: &[&CatalogueEntry],
        pilot: PilotChoice,
    ) -> Result<Vec<CatalogueItem>, InventoryError> {
        let (_, snapshot) = self.resolve_pilot(workspace_id, pilot).await?;
        let required = match &snapshot {
            Some(_) => self
                .sde
                .required_skills(
                    &entries
                        .iter()
                        .map(|entry| entry.type_id)
                        .collect::<Vec<_>>(),
                )
                .await
                .map_err(sde_error)?,
            None => BTreeMap::new(),
        };
        Ok(entries
            .iter()
            .map(|entry| CatalogueItem {
                type_id: entry.type_id,
                name: entry.name.clone(),
                kind: entry.kind,
                slot: entry.slot,
                cpu: entry.cpu,
                powergrid: entry.powergrid,
                calibration: entry.calibration,
                hardpoint: entry.hardpoint,
                rig_size: entry.rig_size,
                charge_size: entry.charge_size,
                market_path: entry
                    .market_group_id
                    .map(|id| catalogue.market_path(id))
                    .unwrap_or_default(),
                // Prerequisites are trained before a skill, so the direct
                // requirements decide it, at active levels.
                can_use: snapshot.as_ref().map(|snapshot| {
                    required
                        .get(&entry.type_id)
                        .into_iter()
                        .flatten()
                        .all(|requirement| {
                            snapshot
                                .skill(requirement.skill_type_id)
                                .is_some_and(|skill| skill.active_level >= requirement.level)
                        })
                }),
            })
            .collect())
    }

    /// Every modifier on one attribute of a saved fit's ship or item, with
    /// named sources. `None` when the fit isn't in the workspace.
    #[allow(clippy::too_many_arguments)]
    pub async fn explain(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        id: FitId,
        pilot: PilotChoice,
        profile: &DamageProfile,
        target: ExplainTarget,
        attribute: &str,
    ) -> Result<Option<AttributeView>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let (pilot, _) = self.resolve_pilot(workspace_id, pilot).await?;
        let explanation = engine
            .explain(&fit.input(), &pilot, profile, target, attribute)
            .map_err(calculation_error)?;
        let skill_ids: Vec<i64> = explanation
            .sources
            .iter()
            .filter_map(|source| match source.from {
                SourceRef::Skill { type_id } => Some(type_id),
                _ => None,
            })
            .collect();
        let mut references = self.references(&[(fit.ship_type_id, &fit.items)]).await?;
        references.extend(
            self.sde
                .type_reference(&skill_ids)
                .await
                .map_err(sde_error)?,
        );
        let name_of = |type_id: i64| fit_type(&references, type_id).name;
        let item = |index: usize| fit.items.get(index);
        Ok(Some(AttributeView {
            base: explanation.base,
            value: explanation.value,
            sources: explanation
                .sources
                .into_iter()
                .map(|source| ExplainedSource {
                    name: match source.from {
                        SourceRef::Ship => name_of(fit.ship_type_id),
                        SourceRef::Item { index } => {
                            item(index).and_then(|item| name_of(item.type_id))
                        }
                        SourceRef::Charge { index } => item(index)
                            .and_then(|item| item.charge_type_id)
                            .and_then(name_of),
                        SourceRef::Skill { type_id } => name_of(type_id),
                        SourceRef::Character => Some("Character".to_string()),
                        SourceRef::Mode | SourceRef::Projected { .. } | SourceRef::Buff { .. } => {
                            None
                        }
                    },
                    from: source.from,
                    operator: source.operator,
                    value: source.value,
                    penalty: source.penalty,
                    applied: source.applied,
                })
                .collect(),
        }))
    }

    /// A saved fit's stats; `None` when the fit isn't in the workspace.
    pub async fn stats(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        id: FitId,
        pilot: PilotChoice,
        profile: &DamageProfile,
        now: DateTime<Utc>,
    ) -> Result<Option<FitStatsView>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        self.calculate(workspace_id, engine, &fit.input(), pilot, profile, now)
            .await
            .map(Some)
    }

    /// Every workspace character against a saved fit: can use all, then
    /// hull only, then cannot, then unknown; closest first within each.
    /// `None` when the fit isn't in the workspace.
    pub async fn pilots(
        &self,
        workspace_id: WorkspaceId,
        engine: Option<&dyn FittingEngine>,
        id: FitId,
        profile: &DamageProfile,
        now: DateTime<Utc>,
    ) -> Result<Option<Vec<FitPilot>>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let input = fit.input();
        let (required, definitions) = self.requirement_data(&[&input]).await?;
        let requirements = fit_requirements(&input, &required);
        let mut rows = Vec::new();
        for character in self.workspace_characters(workspace_id).await? {
            let snapshot = snapshot(&character);
            let flyability = snapshot
                .as_ref()
                .map(|snapshot| evaluate_flyability(&requirements, &definitions, snapshot, now));
            let stats = match (engine, &snapshot) {
                (Some(engine), Some(snapshot)) => {
                    let calculation = engine
                        .calculate(&input, &pilot_skills(snapshot)?, profile)
                        .map_err(calculation_error)?;
                    let stats = calculation.stats;
                    Some(PilotHeadline {
                        ehp: stats.defense.ehp,
                        dps: stats.offense.total_dps,
                        capacitor: stats.capacitor.state,
                        max_velocity: stats.navigation.max_velocity,
                        align_seconds: stats.navigation.align_seconds,
                    })
                }
                _ => None,
            };
            rows.push(FitPilot {
                connection_id: character.connection_id,
                name: character.name,
                status: flyability.as_ref().map(|flyability| flyability.status),
                flyability,
                stats,
            });
        }
        rows.sort_by(|a, b| {
            let remaining = |row: &FitPilot| {
                row.flyability
                    .as_ref()
                    .and_then(|flyability| flyability.remaining_seconds)
                    .unwrap_or(i64::MAX)
            };
            let ehp = |row: &FitPilot| row.stats.map_or(0.0, |stats| stats.ehp);
            status_rank(a.status)
                .cmp(&status_rank(b.status))
                .then(remaining(a).cmp(&remaining(b)))
                .then(ehp(b).total_cmp(&ehp(a)))
                .then(a.name.cmp(&b.name))
        });
        Ok(Some(rows))
    }

    /// A skill plan named `Fit: <name>`: the fit's whole requirement
    /// closure, or with `connection_id`, only what that character still has
    /// to train. `None` when the fit isn't in the workspace.
    pub async fn create_skill_plan(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        connection_id: Option<ConnectedCharacterId>,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlan>, InventoryError> {
        let skill_plans = self.skill_plans.clone().ok_or_else(|| {
            InventoryError::Persistence("Skill plan persistence is unavailable.".to_string())
        })?;
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let snapshot = match connection_id {
            Some(connection_id) => {
                self.resolve_pilot(workspace_id, PilotChoice::Character(connection_id))
                    .await?
                    .1
            }
            None => None,
        };
        let input = fit.input();
        let (required, definitions) = self.requirement_data(&[&input]).await?;
        let requirements: FitRequirements = fit_requirements(&input, &required);
        let entries = requirement_plan(&requirements, &definitions, snapshot.as_ref())
            .map_err(|error| InventoryError::Validation(format!("{error:?}")))?;
        if entries.is_empty() {
            return Err(InventoryError::Validation(
                "nothing left to train for this fit".to_string(),
            ));
        }
        skill_plans
            .create_skill_plan(
                workspace_id,
                &SkillPlanInput {
                    name: format!("Fit: {}", fit.name),
                    description: String::new(),
                    entries,
                },
                now,
            )
            .await
            .map(Some)
    }

    /// Names and groups for every type the fits use, from the active import.
    async fn references(
        &self,
        inputs: &[(i64, &[FitItem])],
    ) -> Result<BTreeMap<i64, TypeReference>, InventoryError> {
        let ids: BTreeSet<i64> = inputs
            .iter()
            .flat_map(|(ship, items)| {
                std::iter::once(*ship).chain(
                    items
                        .iter()
                        .flat_map(|item| std::iter::once(item.type_id).chain(item.charge_type_id)),
                )
            })
            .collect();
        self.sde
            .type_reference(&ids.into_iter().collect::<Vec<_>>())
            .await
            .map_err(sde_error)
    }

    pub async fn list(&self, workspace_id: WorkspaceId) -> Result<Vec<FitSummary>, InventoryError> {
        let fits = self.fits.list_fits(workspace_id).await?;
        let ships: Vec<(i64, &[FitItem])> =
            fits.iter().map(|fit| (fit.ship_type_id, &[][..])).collect();
        let references = self.references(&ships).await?;
        let inputs: Vec<FitInput> = fits.iter().map(Fit::input).collect();
        let characters = self.workspace_characters(workspace_id).await?;
        let snapshots: Vec<(&SkillsCharacter, Option<CharacterSkillsSnapshot>)> = characters
            .iter()
            .map(|character| (character, snapshot(character)))
            .collect();
        let (required, definitions) = if fits.is_empty() || characters.is_empty() {
            (BTreeMap::new(), Vec::new())
        } else {
            self.requirement_data(&inputs.iter().collect::<Vec<_>>())
                .await?
        };
        let now = Utc::now();
        Ok(fits
            .iter()
            .zip(&inputs)
            .map(|(fit, input)| {
                let requirements = fit_requirements(input, &required);
                FitSummary {
                    id: fit.id,
                    name: fit.name.clone(),
                    ship: fit_type(&references, fit.ship_type_id),
                    item_count: fit.items.len(),
                    updated_at: fit.updated_at,
                    pilots: snapshots
                        .iter()
                        .map(|(character, snapshot)| FitListPilot {
                            connection_id: character.connection_id,
                            name: character.name.clone(),
                            status: snapshot.as_ref().map(|snapshot| {
                                evaluate_flyability(&requirements, &definitions, snapshot, now)
                                    .status
                            }),
                        })
                        .collect(),
                }
            })
            .collect())
    }

    pub async fn get(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<Option<FitDetail>, InventoryError> {
        match self.fits.fit(workspace_id, id).await? {
            Some(fit) => Ok(Some(self.detail(fit).await?)),
            None => Ok(None),
        }
    }

    pub async fn create(
        &self,
        workspace_id: WorkspaceId,
        draft: FitDraft,
        now: DateTime<Utc>,
    ) -> Result<FitDetail, InventoryError> {
        draft.validate().map_err(InventoryError::Validation)?;
        let fit = self.fits.create_fit(workspace_id, &draft, now).await?;
        self.detail(fit).await
    }

    pub async fn update(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        draft: FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Option<FitDetail>, InventoryError> {
        draft.validate().map_err(InventoryError::Validation)?;
        match self.fits.update_fit(workspace_id, id, &draft, now).await? {
            Some(fit) => Ok(Some(self.detail(fit).await?)),
            None => Ok(None),
        }
    }

    pub async fn delete(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<bool, InventoryError> {
        self.fits.delete_fit(workspace_id, id).await
    }

    /// What importing `text` would make; nothing is saved.
    pub async fn preview_text(
        &self,
        engine: &dyn FittingEngine,
        text: &str,
    ) -> Result<FitTextPreview, InventoryError> {
        match engine.parse_eft(text) {
            Ok(parsed) => {
                let references = self
                    .references(&[(parsed.fit.ship_type_id, &parsed.fit.items)])
                    .await?;
                let ship = fit_type(&references, parsed.fit.ship_type_id);
                Ok(FitTextPreview {
                    fit: Some(FitPreview {
                        name: parsed.name.unwrap_or_else(|| default_name(&ship)),
                        items: item_views(&references, &parsed.fit.items),
                        ship,
                    }),
                    unknown_lines: Vec::new(),
                    error: None,
                })
            }
            Err(EftError::UnknownLines(lines)) => Ok(FitTextPreview {
                fit: None,
                unknown_lines: lines,
                error: None,
            }),
            Err(EftError::Invalid(message)) => Ok(FitTextPreview {
                fit: None,
                unknown_lines: Vec::new(),
                error: Some(message),
            }),
        }
    }

    /// Saves the fit `text` describes. The name is `name` when given, else
    /// the one in the text, else `<Ship> fit`.
    pub async fn import_text(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        text: &str,
        name: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<FitDetail, InventoryError> {
        let parsed = engine.parse_eft(text).map_err(|error| match error {
            EftError::UnknownLines(lines) => {
                InventoryError::Validation(unknown_lines_message(&lines))
            }
            EftError::Invalid(message) => InventoryError::Validation(message),
        })?;
        let name = match name.filter(|name| !name.trim().is_empty()).or(parsed.name) {
            Some(name) => name,
            None => {
                let references = self.references(&[(parsed.fit.ship_type_id, &[])]).await?;
                default_name(&fit_type(&references, parsed.fit.ship_type_id))
            }
        };
        self.create(
            workspace_id,
            FitDraft {
                name,
                description: String::new(),
                ship_type_id: parsed.fit.ship_type_id,
                items: parsed.fit.items,
            },
            now,
        )
        .await
    }

    /// The fit as EFT, for pasting into the client. `None` when the fit
    /// isn't in the workspace.
    pub async fn export_text(
        &self,
        workspace_id: WorkspaceId,
        engine: &dyn FittingEngine,
        id: FitId,
    ) -> Result<Option<String>, InventoryError> {
        let Some(fit) = self.fits.fit(workspace_id, id).await? else {
            return Ok(None);
        };
        let input: FitInput = fit.input();
        engine
            .write_eft(&fit.name, &input)
            .map(Some)
            .map_err(|error| match error {
                FittingError::UnknownShip(type_id) => InventoryError::Validation(format!(
                    "the fitting engine doesn't know ship type {type_id}"
                )),
                other => InventoryError::Persistence(format!("could not write the fit: {other:?}")),
            })
    }

    async fn detail(&self, fit: Fit) -> Result<FitDetail, InventoryError> {
        let references = self.references(&[(fit.ship_type_id, &fit.items)]).await?;
        Ok(FitDetail {
            id: fit.id,
            ship: fit_type(&references, fit.ship_type_id),
            items: item_views(&references, &fit.items),
            name: fit.name,
            description: fit.description,
            created_at: fit.created_at,
            updated_at: fit.updated_at,
        })
    }
}

fn fit_type(references: &BTreeMap<i64, TypeReference>, type_id: i64) -> FitType {
    let reference = references.get(&type_id);
    FitType {
        type_id,
        name: reference.and_then(|reference| reference.type_name.clone()),
        group_name: reference.and_then(|reference| reference.group_name.clone()),
    }
}

fn item_views(references: &BTreeMap<i64, TypeReference>, items: &[FitItem]) -> Vec<FitItemView> {
    items
        .iter()
        .map(|item| {
            let item_type = fit_type(references, item.type_id);
            FitItemView {
                name: item_type.name,
                group_name: item_type.group_name,
                charge_name: item
                    .charge_type_id
                    .and_then(|charge| fit_type(references, charge).name),
                item: item.clone(),
            }
        })
        .collect()
}
