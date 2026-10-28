//! The Type Info window's data: one type from the active SDE import, with
//! its description, attributes, ship bonuses, required skills (checked
//! against a pilot), industry and price, shaped for display.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::type_info::{
    attribute_groups, parse_rich_text, requirement_summary, requirement_tree, trait_groups,
    AttributeGroup, NameMaps, RequirementPilot, RequirementSummary, RichSegment, SkillNode,
    TraitGroup,
};
use iskworks_core::{
    summarize_scoped_orders, InventoryError, MarketRepository, MarketScope, Money, WorkspaceId,
};
use iskworks_sde::{CandidateRecipeKind, SdeReadRepository};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::fittings::character_snapshot;
use crate::{PilotChoice, SkillsCharacterSource};

/// An id with its name.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedId {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypePrice {
    /// The lowest sell order in `scope`.
    pub best_sell: Money,
    pub scope: MarketScope,
    pub observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeBasics {
    pub volume_m3: Option<Decimal>,
    pub packaged_volume_m3: Option<Decimal>,
    pub mass_kg: Option<Decimal>,
    pub capacity_m3: Option<Decimal>,
    pub portion_size: Option<i32>,
}

/// A blueprint or reaction formula that makes the type.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeProducer {
    pub kind: CandidateRecipeKind,
    pub type_id: i64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeQuantity {
    pub type_id: i64,
    pub name: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeIndustry {
    pub produced_by: Vec<TypeProducer>,
    /// Per portion, before any reprocessing yield.
    pub reprocesses_into: Vec<TypeQuantity>,
    /// Blueprints and reaction formulas that consume it.
    pub used_in_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfoView {
    pub type_id: i64,
    pub name: String,
    pub published: bool,
    pub group: Option<NamedId>,
    pub category: Option<NamedId>,
    pub meta_group: Option<NamedId>,
    /// Market group names, root first.
    pub market_group_path: Vec<String>,
    /// `None` without market data or a sell order.
    pub price: Option<TypePrice>,
    pub description: Vec<RichSegment>,
    pub basics: TypeBasics,
    pub attributes: Vec<AttributeGroup>,
    pub traits: Vec<TraitGroup>,
    pub requirements: Vec<SkillNode>,
    /// For a character pilot: what it still has to train.
    pub missing: Option<RequirementSummary>,
    pub industry: TypeIndustry,
    /// Types in its variation family, itself included.
    pub variation_count: i64,
}

#[derive(Clone)]
pub struct TypeInfoService {
    sde: Arc<dyn SdeReadRepository>,
    characters: Option<Arc<dyn SkillsCharacterSource>>,
    market: Option<Arc<dyn MarketRepository>>,
}

fn named(id: Option<i64>, name: Option<String>) -> Option<NamedId> {
    Some(NamedId {
        id: id?,
        name: name?,
    })
}

fn sde_error(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

impl TypeInfoService {
    #[must_use]
    pub fn new(sde: Arc<dyn SdeReadRepository>) -> Self {
        Self {
            sde,
            characters: None,
            market: None,
        }
    }

    /// The workspace's characters, for a character pilot.
    #[must_use]
    pub fn with_characters(mut self, characters: Arc<dyn SkillsCharacterSource>) -> Self {
        self.characters = Some(characters);
        self
    }

    /// Where the price comes from.
    #[must_use]
    pub fn with_market(mut self, market: Arc<dyn MarketRepository>) -> Self {
        self.market = Some(market);
        self
    }

    /// `type_id`'s info, its requirements checked against `pilot` and
    /// priced in `price_scope`; `None` for a type the active import
    /// doesn't have.
    pub async fn info(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        pilot: PilotChoice,
        price_scope: Option<MarketScope>,
        now: DateTime<Utc>,
    ) -> Result<Option<TypeInfoView>, InventoryError> {
        let Some(base) = self.sde.type_info_base(type_id).await.map_err(sde_error)? else {
            return Ok(None);
        };
        // Resolve the pilot first: an unknown character fails the request.
        let character = match pilot {
            PilotChoice::Character(connection_id) => Some(
                character_snapshot(self.characters.as_ref(), workspace_id, connection_id).await?,
            ),
            PilotChoice::AllV | PilotChoice::AllZero => None,
        };
        let values = self.sde.type_attributes(type_id).await.map_err(sde_error)?;
        let bonuses = self.sde.type_bonuses(type_id).await.map_err(sde_error)?;
        let materials = self.sde.type_materials(type_id).await.map_err(sde_error)?;
        let counts = self
            .sde
            .type_info_counts(type_id)
            .await
            .map_err(sde_error)?;
        let recipes = self
            .sde
            .production_recipes_for_products(&[type_id])
            .await
            .map_err(sde_error)?
            .remove(&type_id)
            .unwrap_or_default();
        let required = self
            .sde
            .required_skills(&[type_id])
            .await
            .map_err(sde_error)?
            .remove(&type_id)
            .unwrap_or_default();

        // Everything the view names by id, in two lookups.
        let id_value = |value: &iskworks_sde::TypeAttributeValue, unit: i64| {
            (value.unit_id == Some(unit) && value.value.fract().is_zero())
                .then(|| value.value.try_into().ok())
                .flatten()
        };
        let group_ids: Vec<i64> = values
            .iter()
            .filter_map(|value| id_value(value, 115))
            .collect();
        let type_ids: BTreeSet<i64> = values
            .iter()
            .filter_map(|value| id_value(value, 116))
            .chain(
                bonuses
                    .iter()
                    .filter_map(|line| line.source.skill_type_id()),
            )
            .chain(recipes.blueprint_type_id)
            .chain(recipes.reaction_formula_type_id)
            .collect();
        let type_names = self
            .sde
            .type_names(&type_ids.into_iter().collect::<Vec<_>>())
            .await
            .map_err(sde_error)?;
        let names = NameMaps {
            groups: self.sde.group_names(&group_ids).await.map_err(sde_error)?,
            attributes: values
                .iter()
                .map(|value| (value.attribute_id, value.display_name.clone()))
                .collect(),
            types: type_names.clone(),
        };

        let (requirements, missing) = if required.is_empty() {
            (Vec::new(), None)
        } else {
            let definitions = self.sde.skill_catalogue().await.map_err(sde_error)?;
            let requirement_pilot = match (&character, pilot) {
                (Some(snapshot), _) => RequirementPilot::Character(snapshot),
                (None, PilotChoice::AllZero) => RequirementPilot::AllZero,
                (None, _) => RequirementPilot::AllV,
            };
            (
                requirement_tree(&required, &definitions, requirement_pilot),
                character
                    .as_ref()
                    .map(|snapshot| requirement_summary(&required, &definitions, snapshot, now)),
            )
        };

        let produced_by = [
            (
                CandidateRecipeKind::Manufacturing,
                recipes.blueprint_type_id,
            ),
            (
                CandidateRecipeKind::Reaction,
                recipes.reaction_formula_type_id,
            ),
        ]
        .into_iter()
        .filter_map(|(kind, type_id)| {
            let type_id = type_id?;
            Some(TypeProducer {
                kind,
                type_id,
                name: type_names.get(&type_id).cloned(),
            })
        })
        .collect();

        let price = match price_scope {
            Some(scope) => self.price(workspace_id, type_id, scope).await,
            None => None,
        };

        Ok(Some(TypeInfoView {
            type_id: base.type_id,
            name: base.name,
            published: base.published,
            group: named(base.group_id, base.group_name),
            category: named(base.category_id, base.category_name),
            meta_group: named(base.meta_group_id, base.meta_group_name),
            market_group_path: base.market_group_path,
            price,
            description: base
                .description
                .as_deref()
                .map(parse_rich_text)
                .unwrap_or_default(),
            basics: TypeBasics {
                volume_m3: base.volume_m3,
                packaged_volume_m3: base.packaged_volume_m3,
                mass_kg: base.mass_kg,
                capacity_m3: base.capacity_m3,
                portion_size: base.portion_size,
            },
            attributes: attribute_groups(&values, &names),
            traits: trait_groups(&bonuses, &type_names),
            requirements,
            missing,
            industry: TypeIndustry {
                produced_by,
                reprocesses_into: materials
                    .into_iter()
                    .map(|line| TypeQuantity {
                        type_id: line.type_id,
                        name: line.type_name,
                        quantity: line.quantity,
                    })
                    .collect(),
                used_in_count: counts.used_in,
            },
            variation_count: counts.variations,
        }))
    }

    /// The lowest sell in `scope`. A missing or failing market leaves the
    /// price out rather than failing the window.
    async fn price(
        &self,
        workspace_id: WorkspaceId,
        type_id: i64,
        scope: MarketScope,
    ) -> Option<TypePrice> {
        let market = self.market.as_ref()?;
        let books = match market
            .scoped_order_books(workspace_id, scope, &[type_id])
            .await
        {
            Ok(books) => books,
            Err(error) => {
                tracing::warn!(%error, type_id, "type info price lookup failed");
                return None;
            }
        };
        let summary = summarize_scoped_orders(books.get(&type_id)?);
        Some(TypePrice {
            best_sell: summary.best_sell?,
            scope,
            observed_at: summary.observed_at,
        })
    }
}

#[cfg(test)]
mod tests;
