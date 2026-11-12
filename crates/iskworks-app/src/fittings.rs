//! Saved fits for the Fittings page: stored per workspace, named and grouped
//! from the active SDE import, read from and written to EFT text through the
//! fitting engine, calculated for a pilot, and checked against every
//! workspace character's skills (who can fly it, and what they lack).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::fitting::{
    canonical_tag_casing, canonical_tags, evaluate_flyability, fit_requirements,
    AssetHoldingsRepository, EftError, EftLine, Fit, FitCatalogue, FitDraft, FitId, FitInput,
    FitItem, FitRepository, FitTagEdit, FittingEngine, FittingError, FlyabilityStatus,
};
use iskworks_core::skill_plans::SkillPlanRepository;
use iskworks_core::skills::CharacterSkillsSnapshot;
use iskworks_core::{ConnectedCharacterId, InventoryError, MarketRepository, WorkspaceId};
use iskworks_sde::{SdeReadRepository, TypeReference};
use serde::Serialize;

use crate::{SkillsCharacter, SkillsCharacterSource};

mod calculation;
mod catalogue;
mod cost;

pub(crate) use calculation::character_snapshot;
use calculation::snapshot;
pub use calculation::{
    AttributeView, ExplainedSource, FitPilot, FitStatsView, PilotChoice, PilotHeadline,
};
pub use catalogue::{CatalogueItem, CatalogueQuery, CatalogueView};
pub use cost::{FitAssetsView, FitCostLine, FitCostView};

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
    /// By name.
    pub tags: Vec<String>,
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

/// A fitted or carried item, with its names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitItemView {
    #[serde(flatten)]
    pub item: FitItem,
    pub name: Option<String>,
    pub group_name: Option<String>,
    pub charge_name: Option<String>,
    /// It loads charges (a gun, a launcher, a booster); false when the
    /// engine data isn't loaded.
    pub loads_charges: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitDetail {
    pub id: FitId,
    pub name: String,
    pub description: String,
    pub ship: FitType,
    pub items: Vec<FitItemView>,
    /// By name.
    pub tags: Vec<String>,
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
    market: Option<Arc<dyn MarketRepository>>,
    holdings: Option<Arc<dyn AssetHoldingsRepository>>,
    /// The engine's catalogue, for what fitted items can do; absent when
    /// the engine data didn't load.
    catalogue: Option<Arc<FitCatalogue>>,
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
            market: None,
            holdings: None,
            catalogue: None,
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

    /// Where fit prices come from.
    #[must_use]
    pub fn with_market(mut self, market: Arc<dyn MarketRepository>) -> Self {
        self.market = Some(market);
        self
    }

    /// What the workspace's characters own, for asset coverage.
    #[must_use]
    pub fn with_asset_holdings(mut self, holdings: Arc<dyn AssetHoldingsRepository>) -> Self {
        self.holdings = Some(holdings);
        self
    }

    /// The engine's catalogue: marks the items that load charges.
    #[must_use]
    pub fn with_catalogue(mut self, catalogue: Arc<FitCatalogue>) -> Self {
        self.catalogue = Some(catalogue);
        self
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
                    tags: fit.tags.clone(),
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
        Ok(self.fits.delete_fit(workspace_id, id).await?)
    }

    /// Replaces the fit's tags, in the casing the workspace already uses.
    /// `None` when the fit isn't in the workspace.
    pub async fn set_tags(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        tags: &[String],
    ) -> Result<Option<Vec<String>>, InventoryError> {
        let in_use = self.fits.fit_tags_in_use(workspace_id).await?;
        let tags = canonical_tags(tags, &in_use).map_err(InventoryError::Validation)?;
        Ok(self.fits.replace_fit_tags(workspace_id, id, &tags).await?)
    }

    /// Adds and removes tags on many fits; fits not in the workspace are
    /// skipped.
    pub async fn edit_tags(
        &self,
        workspace_id: WorkspaceId,
        edit: &FitTagEdit,
    ) -> Result<(), InventoryError> {
        let edit = edit.validate().map_err(InventoryError::Validation)?;
        let in_use = self.fits.fit_tags_in_use(workspace_id).await?;
        // Casing only: an edit can add more than a fit may hold, and the
        // per-fit limit is the single-fit form's to keep.
        let add = canonical_tag_casing(&edit.add, &in_use);
        Ok(self
            .fits
            .edit_fit_tags(workspace_id, &edit.fit_ids, &add, &edit.remove)
            .await?)
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
                        items: item_views(
                            &references,
                            Some(&engine.catalogue()),
                            &parsed.fit.items,
                        ),
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
            items: item_views(&references, self.catalogue.as_deref(), &fit.items),
            name: fit.name,
            description: fit.description,
            tags: fit.tags,
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

fn item_views(
    references: &BTreeMap<i64, TypeReference>,
    catalogue: Option<&FitCatalogue>,
    items: &[FitItem],
) -> Vec<FitItemView> {
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
                loads_charges: catalogue
                    .and_then(|catalogue| catalogue.entry(item.type_id))
                    .is_some_and(|entry| !entry.charge_groups.is_empty()),
                item: item.clone(),
            }
        })
        .collect()
}
