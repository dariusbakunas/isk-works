//! Calculating a saved or unsaved fit for a pilot (All V, All 0 or a
//! workspace character), explaining one attribute, the pilots table, and a
//! skill plan for what a fit requires.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::fitting::{
    evaluate_flyability, fit_requirements, requirement_plan, CapacitorState, DamageProfile,
    ExplainTarget, FitCalculation, FitId, FitInput, FitRequirements, FittingEngine, FittingError,
    FlyabilityStatus, Pilot, PilotFlyability, PilotSkills, SourceRef,
};
use iskworks_core::skill_plans::{SkillPlan, SkillPlanInput};
use iskworks_core::skills::{parse_skills_summary, CharacterSkillsSnapshot};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{RequiredSkill, SkillDefinition};
use serde::Serialize;

use crate::{SkillsCharacter, SkillsCharacterSource};

use super::{fit_type, sde_error, FittingService};

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

/// A character's synced skills, when it has them.
pub(super) fn snapshot(character: &SkillsCharacter) -> Option<CharacterSkillsSnapshot> {
    if !character.skills_scope_granted {
        return None;
    }
    character
        .skills_summary
        .as_ref()
        .and_then(parse_skills_summary)
}

/// A workspace character's synced skills, for a pilot choice that names
/// it; an error when it isn't in the workspace or has no synced skills.
pub(crate) async fn character_snapshot(
    characters: Option<&Arc<dyn SkillsCharacterSource>>,
    workspace_id: WorkspaceId,
    connection_id: ConnectedCharacterId,
) -> Result<CharacterSkillsSnapshot, InventoryError> {
    let character = match characters {
        Some(characters) => {
            characters
                .skills_character(workspace_id, connection_id)
                .await?
        }
        None => None,
    }
    .ok_or_else(|| InventoryError::Validation("no such character in the workspace".to_string()))?;
    snapshot(&character).ok_or_else(|| {
        InventoryError::Validation(format!(
            "{} has no synced skills to calculate with",
            character.name
        ))
    })
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

impl FittingService {
    pub(super) async fn workspace_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        match &self.characters {
            Some(characters) => characters.skills_characters(workspace_id).await,
            None => Ok(Vec::new()),
        }
    }

    /// What every type of `fits` requires, and the skills the closures need.
    pub(super) async fn requirement_data(
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
    pub(super) async fn resolve_pilot(
        &self,
        workspace_id: WorkspaceId,
        choice: PilotChoice,
    ) -> Result<(Pilot, Option<CharacterSkillsSnapshot>), InventoryError> {
        match choice {
            PilotChoice::AllV => Ok((Pilot::AllV, None)),
            PilotChoice::AllZero => Ok((Pilot::AllZero, None)),
            PilotChoice::Character(connection_id) => {
                let snapshot =
                    character_snapshot(self.characters.as_ref(), workspace_id, connection_id)
                        .await?;
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
}
