//! Skill plans for the Skills page: stored per workspace, normalized against
//! the SDE's prerequisites on every write, and evaluated against the
//! workspace's characters through `iskworks_core::skill_plans`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::skill_plans::{
    evaluate_plan, format_plan_text, normalize_plan, parse_plan_text, PlanEntry, PlanEvaluation,
    SkillGraph, SkillPlan, SkillPlanError, SkillPlanId, SkillPlanInput, SkillPlanRepository,
    UnrecognizedLine,
};
use iskworks_core::skills::{parse_skills_summary, sp_for_level};
use iskworks_core::{ConnectedCharacterId, InventoryError, WorkspaceId};
use iskworks_sde::{SdeReadRepository, SkillAttribute, SkillDefinition};
use serde::Serialize;

use crate::{SkillsCharacter, SkillsCharacterSource};

const ROMAN: [&str; 5] = ["I", "II", "III", "IV", "V"];

/// One character's progress, condensed for a plan card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanProgress {
    pub percent_complete: f64,
    pub remaining_seconds: Option<i64>,
    pub missing_skillbooks: usize,
    pub blocked: bool,
    pub estimated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanSummary {
    pub id: SkillPlanId,
    pub name: String,
    pub description: String,
    pub entry_count: usize,
    pub total_sp: i64,
    pub updated_at: DateTime<Utc>,
    /// For the requested character; `None` without one or without its data.
    pub progress: Option<SkillPlanProgress>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanHeader {
    pub id: SkillPlanId,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A plan entry with the skill facts the editor shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanEntryView {
    pub skill_type_id: i64,
    pub level: i16,
    pub name: String,
    pub group_name: String,
    pub rank: i32,
    pub primary_attribute: SkillAttribute,
    pub secondary_attribute: SkillAttribute,
    pub omega_only: bool,
    /// SP this level takes from the previous one.
    pub sp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterPlanProgress {
    pub connection_id: ConnectedCharacterId,
    pub name: String,
    /// `None` when the character has no readable skills data.
    pub evaluation: Option<PlanEvaluation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanDetail {
    pub plan: SkillPlanHeader,
    pub entries: Vec<SkillPlanEntryView>,
    /// Every connected character, closest to finishing first.
    pub characters: Vec<CharacterPlanProgress>,
    /// What the last write added (prerequisites, lower levels); empty on
    /// a read.
    pub inserted: Vec<PlanEntry>,
}

/// What pasting text would make, for the import dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTextPreview {
    /// Normalized, or as pasted when `error` is set.
    pub entries: Vec<SkillPlanEntryView>,
    pub inserted: Vec<PlanEntry>,
    pub unrecognized: Vec<UnrecognizedLine>,
    /// Why the text can't be imported as it is (e.g. a prerequisite listed
    /// after the skill that needs it).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSkillPlan {
    #[serde(flatten)]
    pub detail: SkillPlanDetail,
    pub unrecognized: Vec<UnrecognizedLine>,
}

#[derive(Clone)]
pub struct SkillPlanService {
    plans: Arc<dyn SkillPlanRepository>,
    characters: Arc<dyn SkillsCharacterSource>,
    sde: Arc<dyn SdeReadRepository>,
}

fn sde_error(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

fn label(graph: &SkillGraph<'_>, entry: PlanEntry) -> String {
    let name = graph.skill(entry.skill_type_id).map_or_else(
        || format!("skill {}", entry.skill_type_id),
        |skill| skill.name.clone(),
    );
    let level = usize::try_from(entry.level - 1)
        .ok()
        .and_then(|index| ROMAN.get(index))
        .map_or_else(|| entry.level.to_string(), |roman| (*roman).to_string());
    format!("{name} {level}")
}

fn plan_error(graph: &SkillGraph<'_>, error: SkillPlanError) -> InventoryError {
    InventoryError::Validation(match error {
        SkillPlanError::UnknownSkill(type_id) => format!("unknown skill {type_id}"),
        SkillPlanError::InvalidLevel(entry) => format!(
            "level {} is not a skill level (1-5) for {}",
            entry.level,
            graph
                .skill(entry.skill_type_id)
                .map_or("an unknown skill", |skill| skill.name.as_str())
        ),
        SkillPlanError::OrderViolation { entry, requires } => format!(
            "{} must come before {}",
            label(graph, requires),
            label(graph, entry)
        ),
        SkillPlanError::CyclicPrerequisites(type_id) => {
            format!("the prerequisites of skill {type_id} loop back on it")
        }
    })
}

fn level_sp(skill: &SkillDefinition, level: i16) -> i64 {
    sp_for_level(skill.rank, level) - sp_for_level(skill.rank, level - 1)
}

/// Entries with the skill facts the editor shows; skills the SDE no longer
/// has are left out.
fn entry_views(graph: &SkillGraph<'_>, entries: &[PlanEntry]) -> Vec<SkillPlanEntryView> {
    entries
        .iter()
        .filter_map(|entry| {
            let skill = graph.skill(entry.skill_type_id)?;
            Some(SkillPlanEntryView {
                skill_type_id: entry.skill_type_id,
                level: entry.level,
                name: skill.name.clone(),
                group_name: skill.group_name.clone(),
                rank: skill.rank,
                primary_attribute: skill.primary_attribute,
                secondary_attribute: skill.secondary_attribute,
                omega_only: skill.omega_only,
                sp: level_sp(skill, entry.level),
            })
        })
        .collect()
}

fn evaluate(
    plan: &SkillPlan,
    definitions: &[SkillDefinition],
    character: &SkillsCharacter,
    now: DateTime<Utc>,
) -> Option<PlanEvaluation> {
    let snapshot = parse_skills_summary(character.skills_summary.as_ref()?)?;
    Some(evaluate_plan(&plan.entries, definitions, &snapshot, now))
}

impl SkillPlanService {
    #[must_use]
    pub fn new(
        plans: Arc<dyn SkillPlanRepository>,
        characters: Arc<dyn SkillsCharacterSource>,
        sde: Arc<dyn SdeReadRepository>,
    ) -> Self {
        Self {
            plans,
            characters,
            sde,
        }
    }

    async fn definitions(&self) -> Result<Vec<SkillDefinition>, InventoryError> {
        self.sde.skill_catalogue().await.map_err(sde_error)
    }

    /// Every plan, with `character`'s progress when given. `Ok(None)` when
    /// that character isn't connected in the workspace.
    pub async fn list(
        &self,
        workspace_id: WorkspaceId,
        character: Option<ConnectedCharacterId>,
        now: DateTime<Utc>,
    ) -> Result<Option<Vec<SkillPlanSummary>>, InventoryError> {
        let character = match character {
            Some(connection_id) => {
                match self
                    .characters
                    .skills_character(workspace_id, connection_id)
                    .await?
                {
                    Some(character) => Some(character),
                    None => return Ok(None),
                }
            }
            None => None,
        };
        let definitions = self.definitions().await?;
        let graph = SkillGraph::new(&definitions);
        let plans = self.plans.list_skill_plans(workspace_id).await?;
        Ok(Some(
            plans
                .iter()
                .map(|plan| SkillPlanSummary {
                    id: plan.id,
                    name: plan.name.clone(),
                    description: plan.description.clone(),
                    entry_count: plan.entries.len(),
                    total_sp: plan
                        .entries
                        .iter()
                        .filter_map(|entry| {
                            graph
                                .skill(entry.skill_type_id)
                                .map(|skill| level_sp(skill, entry.level))
                        })
                        .sum(),
                    updated_at: plan.updated_at,
                    progress: character
                        .as_ref()
                        .and_then(|character| evaluate(plan, &definitions, character, now))
                        .map(|evaluation| SkillPlanProgress {
                            percent_complete: evaluation.percent_complete,
                            remaining_seconds: evaluation.remaining_seconds,
                            missing_skillbooks: evaluation.missing_skillbooks.len(),
                            blocked: evaluation.blocked,
                            estimated: evaluation.estimated,
                        }),
                })
                .collect(),
        ))
    }

    /// The plan with every connected character's progress. `Ok(None)` when
    /// the plan isn't in the workspace.
    pub async fn get(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlanDetail>, InventoryError> {
        let Some(plan) = self.plans.skill_plan(workspace_id, id).await? else {
            return Ok(None);
        };
        let definitions = self.definitions().await?;
        Ok(Some(
            self.detail(workspace_id, plan, &definitions, Vec::new(), now)
                .await?,
        ))
    }

    async fn detail(
        &self,
        workspace_id: WorkspaceId,
        plan: SkillPlan,
        definitions: &[SkillDefinition],
        inserted: Vec<PlanEntry>,
        now: DateTime<Utc>,
    ) -> Result<SkillPlanDetail, InventoryError> {
        let entries = entry_views(&SkillGraph::new(definitions), &plan.entries);
        let mut characters = self
            .characters
            .skills_characters(workspace_id)
            .await?
            .into_iter()
            .map(|character| CharacterPlanProgress {
                evaluation: evaluate(&plan, definitions, &character, now),
                connection_id: character.connection_id,
                name: character.name,
            })
            .collect::<Vec<_>>();
        // Closest first: evaluated and unblocked by remaining time, then
        // blocked, then characters without data; ties by name.
        characters.sort_by(|a, b| {
            let key = |progress: &CharacterPlanProgress| match &progress.evaluation {
                Some(evaluation) if !evaluation.blocked => {
                    (0, evaluation.remaining_seconds.unwrap_or(i64::MAX))
                }
                Some(evaluation) => (1, evaluation.remaining_seconds.unwrap_or(i64::MAX)),
                None => (2, i64::MAX),
            };
            key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
        });
        Ok(SkillPlanDetail {
            plan: SkillPlanHeader {
                id: plan.id,
                name: plan.name,
                description: plan.description,
                created_at: plan.created_at,
                updated_at: plan.updated_at,
            },
            entries,
            characters,
            inserted,
        })
    }

    /// Trims the name, requires one, and normalizes the entries.
    fn prepare(
        input: SkillPlanInput,
        graph: &SkillGraph<'_>,
    ) -> Result<(SkillPlanInput, Vec<PlanEntry>), InventoryError> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(InventoryError::Validation(
                "a skill plan needs a name".to_string(),
            ));
        }
        let normalized =
            normalize_plan(&input.entries, graph).map_err(|error| plan_error(graph, error))?;
        Ok((
            SkillPlanInput {
                name,
                description: input.description.trim().to_string(),
                entries: normalized.entries,
            },
            normalized.inserted,
        ))
    }

    pub async fn create(
        &self,
        workspace_id: WorkspaceId,
        input: SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<SkillPlanDetail, InventoryError> {
        let definitions = self.definitions().await?;
        let (input, inserted) = Self::prepare(input, &SkillGraph::new(&definitions))?;
        let plan = self
            .plans
            .create_skill_plan(workspace_id, &input, now)
            .await?;
        self.detail(workspace_id, plan, &definitions, inserted, now)
            .await
    }

    /// Replaces the whole plan. `Ok(None)` when it isn't in the workspace.
    pub async fn update(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
        input: SkillPlanInput,
        now: DateTime<Utc>,
    ) -> Result<Option<SkillPlanDetail>, InventoryError> {
        let definitions = self.definitions().await?;
        let (input, inserted) = Self::prepare(input, &SkillGraph::new(&definitions))?;
        let Some(plan) = self
            .plans
            .update_skill_plan(workspace_id, id, &input, now)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(
            self.detail(workspace_id, plan, &definitions, inserted, now)
                .await?,
        ))
    }

    pub async fn delete(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<bool, InventoryError> {
        self.plans.delete_skill_plan(workspace_id, id).await
    }

    /// A dry run of [`Self::import_text`]: nothing is saved.
    pub async fn preview_text(&self, text: &str) -> Result<PlanTextPreview, InventoryError> {
        let definitions = self.definitions().await?;
        let graph = SkillGraph::new(&definitions);
        let parsed = parse_plan_text(text, &definitions);
        Ok(match normalize_plan(&parsed.entries, &graph) {
            Ok(normalized) => PlanTextPreview {
                entries: entry_views(&graph, &normalized.entries),
                inserted: normalized.inserted,
                unrecognized: parsed.unrecognized,
                error: None,
            },
            Err(error) => PlanTextPreview {
                entries: entry_views(&graph, &parsed.entries),
                inserted: Vec::new(),
                unrecognized: parsed.unrecognized,
                error: match plan_error(&graph, error) {
                    InventoryError::Validation(message) => Some(message),
                    other => Some(other.to_string()),
                },
            },
        })
    }

    /// Creates a plan from text pasted from the client. Lines that aren't
    /// entries come back with the plan; text with no entries at all is a
    /// `Validation` error.
    pub async fn import_text(
        &self,
        workspace_id: WorkspaceId,
        name: String,
        description: String,
        text: &str,
        now: DateTime<Utc>,
    ) -> Result<ImportedSkillPlan, InventoryError> {
        let definitions = self.definitions().await?;
        let parsed = parse_plan_text(text, &definitions);
        if parsed.entries.is_empty() {
            return Err(InventoryError::Validation(
                "the pasted text has no skill plan entries".to_string(),
            ));
        }
        let detail = self
            .create(
                workspace_id,
                SkillPlanInput {
                    name,
                    description,
                    entries: parsed.entries,
                },
                now,
            )
            .await?;
        Ok(ImportedSkillPlan {
            detail,
            unrecognized: parsed.unrecognized,
        })
    }

    /// The plan as the client's plan text. `Ok(None)` when it isn't in the
    /// workspace.
    pub async fn export_text(
        &self,
        workspace_id: WorkspaceId,
        id: SkillPlanId,
    ) -> Result<Option<String>, InventoryError> {
        let Some(plan) = self.plans.skill_plan(workspace_id, id).await? else {
            return Ok(None);
        };
        let definitions = self.definitions().await?;
        Ok(Some(format_plan_text(
            &plan.entries,
            &SkillGraph::new(&definitions),
        )))
    }
}

#[cfg(test)]
mod tests;
