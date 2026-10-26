//! Saved fits for the Fittings page: stored per workspace, named and grouped
//! from the active SDE import, and read from and written to EFT text through
//! the fitting engine.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::fitting::{
    EftError, EftLine, Fit, FitDraft, FitId, FitInput, FitItem, FitRepository, FittingEngine,
    FittingError,
};
use iskworks_core::{InventoryError, WorkspaceId};
use iskworks_sde::{SdeReadRepository, TypeReference};
use serde::Serialize;

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
        Self { fits, sde }
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
        Ok(fits
            .iter()
            .map(|fit| FitSummary {
                id: fit.id,
                name: fit.name.clone(),
                ship: fit_type(&references, fit.ship_type_id),
                item_count: fit.items.len(),
                updated_at: fit.updated_at,
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
