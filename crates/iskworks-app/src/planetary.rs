//! Read model for the Planetary Interaction page: every connected
//! character's synced colonies, derived through `iskworks_core::planetary`
//! and decorated with SDE names and best-buy prices in the workspace's
//! default market scope. No ESI calls happen here -- freshness comes from
//! the character sync loop (or the per-character manual sync).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::planetary::{
    derive_planet, parse_planets_summary, rollup, Attention, ExcludedExport, PlanetLayout,
    PlanetView, PlanetaryInputs, PlanetaryPreferences, PlanetaryPreferencesRepository,
    SchematicRecipe, StorageKind, TypeFacts,
};
use iskworks_core::{
    summarize_scoped_orders, CharacterSourceKind, CharacterSourceSyncState, ConnectedCharacterId,
    InventoryError, MarketRefreshState, MarketRepository, MarketScope, WorkspaceId,
};
use iskworks_esi::PLANETS_SCOPE;
use iskworks_sde::{PlanetReference, SdeReadRepository};
use iskworks_storage::PgEsiRepository;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

use crate::character_roster::current_trained_level;

/// Interplanetary Consolidation: how many planets a character may colonize.
const INTERPLANETARY_CONSOLIDATION_SKILL_ID: i64 = 2_495;

/// One connected character's PI-relevant sync state.
#[derive(Debug, Clone)]
pub struct PlanetaryCharacter {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub name: String,
    pub scope_granted: bool,
    pub planets_source: Option<CharacterSourceSyncState>,
    pub skills_summary: Option<Value>,
}

#[async_trait]
pub trait PlanetaryCharacterSource: Send + Sync {
    /// Connected (not disconnected) characters in the workspace.
    async fn planetary_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, InventoryError>;
}

#[async_trait]
impl PlanetaryCharacterSource for PgEsiRepository {
    async fn planetary_characters(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryCharacter>, InventoryError> {
        let connections = self.list_connections(workspace_id).await?;
        let mut characters = Vec::with_capacity(connections.len());
        for connection in connections
            .into_iter()
            .filter(|connection| connection.disconnected_at.is_none())
        {
            let sources = self.character_source_state(connection.id).await?;
            let find = |kind| {
                sources
                    .iter()
                    .find(|source| source.source_kind == kind)
                    .cloned()
            };
            characters.push(PlanetaryCharacter {
                connection_id: connection.id,
                eve_character_id: connection.eve_character_id,
                name: connection.character_name.clone(),
                scope_granted: connection
                    .granted_scopes
                    .iter()
                    .any(|scope| scope == PLANETS_SCOPE),
                planets_source: find(CharacterSourceKind::Planets),
                skills_summary: find(CharacterSourceKind::Skills).and_then(|state| state.summary),
            });
        }
        Ok(characters)
    }
}

#[derive(Clone)]
pub struct PlanetaryService {
    characters: Arc<dyn PlanetaryCharacterSource>,
    preferences: Arc<dyn PlanetaryPreferencesRepository>,
    sde: Arc<dyn SdeReadRepository>,
    market: Arc<dyn MarketRepository>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryOverview {
    pub price_observed_at: Option<DateTime<Utc>>,
    pub summary: PlanetarySummaryDto,
    pub characters: Vec<PlanetaryCharacterDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetarySummaryDto {
    pub isk_per_month: String,
    pub planet_count: usize,
    pub character_count: usize,
    pub next_action: Option<NextActionDto>,
    pub alerts: AlertCountsDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextActionDto {
    pub character_name: String,
    pub planet_name: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertCountsDto {
    pub expired: usize,
    pub storage_full: usize,
    pub starved: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetarySyncDto {
    pub observed_at: Option<DateTime<Utc>>,
    pub refresh_state: MarketRefreshState,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryCharacterDto {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub name: String,
    pub pi_skill_level: Option<i64>,
    pub scope_granted: bool,
    pub sync: PlanetarySyncDto,
    pub isk_per_month: String,
    pub next_expiry_at: Option<DateTime<Utc>>,
    pub alert_count: usize,
    pub planets: Vec<PlanetDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetDto {
    pub planet_id: i64,
    pub name: String,
    pub planet_type: String,
    pub solar_system_id: i64,
    pub solar_system_name: Option<String>,
    pub security: Option<String>,
    pub upgrade_level: i64,
    pub last_update: DateTime<Utc>,
    pub attention: Option<Attention>,
    pub isk_per_month: String,
    pub extractors: Vec<ExtractorDto>,
    pub production: Vec<ProductionDto>,
    pub imports: Vec<ImportDto>,
    pub exports: Vec<ExportDto>,
    pub storage: Vec<StorageDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractorDto {
    pub pin_id: i64,
    pub product_type_id: i64,
    pub product_name: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub units_per_hour: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionDto {
    pub schematic_id: i64,
    pub name: String,
    pub factory_count: i64,
    pub output_type_id: i64,
    pub output_per_hour: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDto {
    pub type_id: i64,
    pub name: String,
    pub depletes_at: DateTime<Utc>,
    pub qty_per_hour: String,
    pub lasts_hours: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDto {
    pub type_id: i64,
    pub name: String,
    pub units_per_hour: String,
    pub isk_per_month: Option<String>,
    pub excluded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDto {
    pub pin_id: i64,
    pub kind: StorageKind,
    pub capacity_m3: String,
    pub used_m3: String,
    pub fill_percent: String,
    pub value: String,
    pub contents: Vec<StorageItemDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageItemDto {
    pub type_id: i64,
    pub name: String,
    pub quantity: i64,
    pub volume_m3: String,
    pub value: Option<String>,
}

fn fmt(value: Decimal) -> String {
    value.round_dp(2).normalize().to_string()
}

fn sde_error(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

/// Parsed colonies for one character, or `None` when it has no usable
/// planets summary (never synced, missing scope, or unreadable).
fn layouts(character: &PlanetaryCharacter) -> Vec<PlanetLayout> {
    character
        .planets_source
        .as_ref()
        .and_then(|source| source.summary.as_ref())
        .and_then(|summary| match parse_planets_summary(summary) {
            Ok(planets) => Some(planets),
            Err(error) => {
                tracing::warn!(%error, character = character.eve_character_id, "unreadable planets summary");
                None
            }
        })
        .unwrap_or_default()
}

impl PlanetaryService {
    #[must_use]
    pub fn new(
        characters: Arc<dyn PlanetaryCharacterSource>,
        preferences: Arc<dyn PlanetaryPreferencesRepository>,
        sde: Arc<dyn SdeReadRepository>,
        market: Arc<dyn MarketRepository>,
    ) -> Self {
        Self {
            characters,
            preferences,
            sde,
            market,
        }
    }

    pub async fn preferences(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<PlanetaryPreferences, InventoryError> {
        self.preferences.planetary_preferences(workspace_id).await
    }

    pub async fn save_preferences(
        &self,
        workspace_id: WorkspaceId,
        mut preferences: PlanetaryPreferences,
    ) -> Result<PlanetaryPreferences, InventoryError> {
        let mut seen = BTreeSet::new();
        preferences
            .excluded_exports
            .retain(|item| seen.insert(*item));
        let mut seen = BTreeSet::new();
        preferences.character_order.retain(|id| seen.insert(*id));
        self.preferences
            .save_planetary_preferences(workspace_id, &preferences)
            .await?;
        Ok(preferences)
    }

    pub async fn overview(
        &self,
        workspace_id: WorkspaceId,
        scope: MarketScope,
    ) -> Result<PlanetaryOverview, InventoryError> {
        self.assemble(workspace_id, Some(scope)).await
    }

    /// Extractor expiries and projected factory-input run-outs for every
    /// connected character, for the Calendar. Unpriced: timers don't need
    /// market data.
    pub async fn timers(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PlanetaryTimer>, InventoryError> {
        let overview = self.assemble(workspace_id, None).await?;
        Ok(planetary_timers(&overview))
    }

    async fn assemble(
        &self,
        workspace_id: WorkspaceId,
        scope: Option<MarketScope>,
    ) -> Result<PlanetaryOverview, InventoryError> {
        let now = Utc::now();
        let mut characters = self.characters.planetary_characters(workspace_id).await?;
        let preferences = self.preferences.planetary_preferences(workspace_id).await?;
        let position = |id: i64| {
            preferences
                .character_order
                .iter()
                .position(|ordered| *ordered == id)
                .unwrap_or(usize::MAX)
        };
        characters.sort_by(|a, b| {
            position(a.eve_character_id)
                .cmp(&position(b.eve_character_id))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let layouts: Vec<Vec<PlanetLayout>> = characters.iter().map(layouts).collect();

        let all_planets = || layouts.iter().flatten();
        let schematic_ids: Vec<i64> = all_planets()
            .flat_map(|planet| planet.pins.iter().filter_map(|pin| pin.schematic_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let sde_schematics = self
            .sde
            .planet_schematics(&schematic_ids)
            .await
            .map_err(sde_error)?;
        let schematics: BTreeMap<i64, SchematicRecipe> = sde_schematics
            .iter()
            .map(|(id, schematic)| {
                let lines = |lines: &[iskworks_sde::PlanetSchematicLine]| {
                    lines
                        .iter()
                        .map(|line| (line.type_id, line.quantity))
                        .collect()
                };
                (
                    *id,
                    SchematicRecipe {
                        cycle_time_seconds: schematic.cycle_time_seconds,
                        inputs: lines(&schematic.inputs),
                        outputs: lines(&schematic.outputs),
                    },
                )
            })
            .collect();

        let mut type_ids = BTreeSet::new();
        for planet in all_planets() {
            for pin in &planet.pins {
                type_ids.insert(pin.type_id);
                type_ids.extend(pin.contents.iter().map(|item| item.type_id));
                if let Some(extractor) = pin.extractor {
                    type_ids.insert(extractor.product_type_id);
                }
            }
        }
        for recipe in schematics.values() {
            type_ids.extend(recipe.inputs.iter().map(|(id, _)| *id));
            type_ids.extend(recipe.outputs.iter().map(|(id, _)| *id));
        }
        let type_ids: Vec<i64> = type_ids.into_iter().collect();
        let references = self
            .sde
            .type_reference(&type_ids)
            .await
            .map_err(sde_error)?;
        let types: BTreeMap<i64, TypeFacts> = references
            .iter()
            .map(|(id, reference)| {
                (
                    *id,
                    TypeFacts {
                        group_id: reference.group_id,
                        volume_m3: reference.packaged_volume_m3,
                    },
                )
            })
            .collect();
        let type_name = |type_id: i64| {
            references
                .get(&type_id)
                .and_then(|reference| reference.type_name.clone())
                .unwrap_or_else(|| format!("Type {type_id}"))
        };

        // Only commodities need prices; pins are structures.
        let priced_ids: Vec<i64> = type_ids
            .iter()
            .copied()
            .filter(|id| {
                types
                    .get(id)
                    .and_then(|facts| facts.group_id)
                    .and_then(StorageKind::from_group_id)
                    .is_none()
            })
            .collect();
        let books = match scope {
            Some(scope) => self
                .market
                .scoped_order_books(workspace_id, scope, &priced_ids)
                .await
                .map_err(sde_error)?,
            None => BTreeMap::new(),
        };
        let mut price_observed_at: Option<DateTime<Utc>> = None;
        let mut prices = BTreeMap::new();
        for (type_id, orders) in &books {
            let data = summarize_scoped_orders(orders);
            if let Some(best_buy) = data.best_buy {
                prices.insert(*type_id, best_buy.0);
            }
            price_observed_at = price_observed_at.max(data.observed_at);
        }

        let planet_ids: Vec<i64> = all_planets().map(|planet| planet.planet_id).collect();
        let planet_refs = self
            .sde
            .planet_references(&planet_ids)
            .await
            .map_err(sde_error)?;

        let excluded_by_planet: BTreeMap<(i64, i64), BTreeSet<i64>> =
            preferences.excluded_exports.iter().fold(
                BTreeMap::new(),
                |mut map,
                 ExcludedExport {
                     character_id,
                     planet_id,
                     type_id,
                 }| {
                    map.entry((*character_id, *planet_id))
                        .or_insert_with(BTreeSet::new)
                        .insert(*type_id);
                    map
                },
            );
        let empty = BTreeSet::new();

        let mut character_dtos = Vec::with_capacity(characters.len());
        let mut total_isk = Decimal::ZERO;
        let mut alerts = AlertCountsDto::default();
        let mut next_action: Option<NextActionDto> = None;
        let mut planet_count = 0;
        for (character, planets) in characters.iter().zip(layouts.iter()) {
            let mut views: Vec<(&PlanetLayout, PlanetView)> = planets
                .iter()
                .map(|layout| {
                    let excluded = excluded_by_planet
                        .get(&(character.eve_character_id, layout.planet_id))
                        .unwrap_or(&empty);
                    let view = derive_planet(
                        layout,
                        &PlanetaryInputs {
                            schematics: &schematics,
                            types: &types,
                            prices: &prices,
                            excluded_exports: excluded,
                            now,
                        },
                    );
                    (layout, view)
                })
                .collect();
            views.sort_by(|(a, _), (b, _)| {
                let system = |layout: &PlanetLayout| {
                    planet_refs
                        .get(&layout.planet_id)
                        .map(|reference| reference.solar_system_name.clone())
                };
                system(a)
                    .cmp(&system(b))
                    .then(a.planet_id.cmp(&b.planet_id))
            });
            let character_rollup = rollup(
                &views
                    .iter()
                    .map(|(_, view)| view.clone())
                    .collect::<Vec<_>>(),
            );
            total_isk += character_rollup.isk_per_month;
            alerts.expired += character_rollup.expired_count;
            alerts.storage_full += character_rollup.storage_full_count;
            alerts.starved += character_rollup.starved_count;
            planet_count += views.len();
            if let Some((planet_id, at)) = character_rollup.next_expiry {
                if next_action.as_ref().map_or(true, |current| at < current.at) {
                    next_action = Some(NextActionDto {
                        character_name: character.name.clone(),
                        planet_name: planet_name(planet_refs.get(&planet_id), planet_id),
                        at,
                    });
                }
            }
            let schematic_name = |id: i64| {
                sde_schematics
                    .get(&id)
                    .map_or_else(|| format!("Schematic {id}"), |s| s.name.clone())
            };
            character_dtos.push(PlanetaryCharacterDto {
                connection_id: character.connection_id,
                eve_character_id: character.eve_character_id,
                name: character.name.clone(),
                pi_skill_level: current_trained_level(
                    character.skills_summary.as_ref(),
                    INTERPLANETARY_CONSOLIDATION_SKILL_ID,
                ),
                scope_granted: character.scope_granted,
                sync: PlanetarySyncDto {
                    observed_at: character
                        .planets_source
                        .as_ref()
                        .and_then(|source| source.observed_at),
                    refresh_state: character
                        .planets_source
                        .as_ref()
                        .map_or(MarketRefreshState::Missing, |source| source.refresh_state),
                    last_error: character
                        .planets_source
                        .as_ref()
                        .and_then(|source| source.last_error.clone()),
                },
                isk_per_month: fmt(character_rollup.isk_per_month),
                next_expiry_at: character_rollup.next_expiry.map(|(_, at)| at),
                alert_count: character_rollup.alert_count,
                planets: views
                    .into_iter()
                    .map(|(layout, view)| {
                        planet_dto(
                            layout,
                            view,
                            planet_refs.get(&layout.planet_id),
                            &type_name,
                            &schematic_name,
                        )
                    })
                    .collect(),
            });
        }

        Ok(PlanetaryOverview {
            price_observed_at,
            summary: PlanetarySummaryDto {
                isk_per_month: fmt(total_isk),
                planet_count,
                character_count: character_dtos.len(),
                next_action,
                alerts,
            },
            characters: character_dtos,
        })
    }
}

/// One planetary moment worth putting on a calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryTimer {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub planet_id: i64,
    pub planet_name: String,
    pub planet_type: String,
    pub solar_system_name: Option<String>,
    pub occurs_at: DateTime<Utc>,
    pub event: PlanetaryTimerEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum PlanetaryTimerEvent {
    /// Every extractor on the planet ending at this instant (ECUs set up
    /// together share an expiry, so they form one timer).
    #[serde(rename_all = "camelCase")]
    ExtractorExpiry {
        extractor_count: usize,
        products: Vec<TimerProduct>,
    },
    /// A factory input projected to run out (from the in-game snapshot, so an
    /// estimate -- unlike extractor expiries, which ESI reports exactly).
    #[serde(rename_all = "camelCase")]
    ImportDepleted {
        type_id: i64,
        type_name: String,
        qty_per_hour: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerProduct {
    pub type_id: i64,
    pub name: String,
}

fn planetary_timers(overview: &PlanetaryOverview) -> Vec<PlanetaryTimer> {
    let mut timers = Vec::new();
    for character in &overview.characters {
        for planet in &character.planets {
            let timer = |occurs_at, event| PlanetaryTimer {
                connection_id: character.connection_id,
                eve_character_id: character.eve_character_id,
                character_name: character.name.clone(),
                planet_id: planet.planet_id,
                planet_name: planet.name.clone(),
                planet_type: planet.planet_type.clone(),
                solar_system_name: planet.solar_system_name.clone(),
                occurs_at,
                event,
            };
            let mut by_expiry: BTreeMap<DateTime<Utc>, Vec<&ExtractorDto>> = BTreeMap::new();
            for extractor in &planet.extractors {
                if let Some(expires_at) = extractor.expires_at {
                    by_expiry.entry(expires_at).or_default().push(extractor);
                }
            }
            for (expires_at, extractors) in by_expiry {
                let mut products: Vec<TimerProduct> = Vec::new();
                for extractor in &extractors {
                    if !products
                        .iter()
                        .any(|p| p.type_id == extractor.product_type_id)
                    {
                        products.push(TimerProduct {
                            type_id: extractor.product_type_id,
                            name: extractor.product_name.clone(),
                        });
                    }
                }
                timers.push(timer(
                    expires_at,
                    PlanetaryTimerEvent::ExtractorExpiry {
                        extractor_count: extractors.len(),
                        products,
                    },
                ));
            }
            for import in &planet.imports {
                timers.push(timer(
                    import.depletes_at,
                    PlanetaryTimerEvent::ImportDepleted {
                        type_id: import.type_id,
                        type_name: import.name.clone(),
                        qty_per_hour: import.qty_per_hour.clone(),
                    },
                ));
            }
        }
    }
    timers.sort_by_key(|timer| timer.occurs_at);
    timers
}

fn planet_name(reference: Option<&PlanetReference>, planet_id: i64) -> String {
    reference.map_or_else(|| format!("Planet {planet_id}"), |r| r.name.clone())
}

fn planet_dto(
    layout: &PlanetLayout,
    view: PlanetView,
    reference: Option<&PlanetReference>,
    type_name: &dyn Fn(i64) -> String,
    schematic_name: &dyn Fn(i64) -> String,
) -> PlanetDto {
    PlanetDto {
        planet_id: layout.planet_id,
        name: planet_name(reference, layout.planet_id),
        planet_type: layout.planet_type.clone(),
        solar_system_id: layout.solar_system_id,
        solar_system_name: reference.map(|r| r.solar_system_name.clone()),
        security: reference
            .and_then(|r| r.security_status)
            .map(|value| value.round_dp(2).to_string()),
        upgrade_level: layout.upgrade_level,
        last_update: layout.last_update,
        attention: view.attention,
        isk_per_month: fmt(view.isk_per_month),
        extractors: view
            .extractors
            .into_iter()
            .map(|extractor| ExtractorDto {
                pin_id: extractor.pin_id,
                product_type_id: extractor.product_type_id,
                product_name: type_name(extractor.product_type_id),
                expires_at: extractor.expires_at,
                units_per_hour: fmt(extractor.units_per_hour),
            })
            .collect(),
        production: view
            .production
            .into_iter()
            .map(|production| ProductionDto {
                schematic_id: production.schematic_id,
                name: schematic_name(production.schematic_id),
                factory_count: production.factory_count,
                output_type_id: production.output_type_id,
                output_per_hour: fmt(production.output_per_hour),
            })
            .collect(),
        imports: view
            .imports
            .into_iter()
            .map(|import| ImportDto {
                type_id: import.type_id,
                name: type_name(import.type_id),
                depletes_at: import.depletes_at,
                qty_per_hour: fmt(import.qty_per_hour),
                lasts_hours: fmt(import.lasts_hours),
            })
            .collect(),
        exports: view
            .exports
            .into_iter()
            .map(|export| ExportDto {
                type_id: export.type_id,
                name: type_name(export.type_id),
                units_per_hour: fmt(export.units_per_hour),
                isk_per_month: export.isk_per_month.map(fmt),
                excluded: export.excluded,
            })
            .collect(),
        storage: view
            .storage
            .into_iter()
            .map(|storage| StorageDto {
                pin_id: storage.pin_id,
                kind: storage.kind,
                capacity_m3: fmt(storage.capacity_m3),
                used_m3: fmt(storage.used_m3),
                fill_percent: fmt(storage.fill_percent),
                value: fmt(storage.value),
                contents: storage
                    .contents
                    .into_iter()
                    .map(|item| StorageItemDto {
                        type_id: item.type_id,
                        name: type_name(item.type_id),
                        quantity: item.quantity,
                        volume_m3: fmt(item.volume_m3),
                        value: item.value.map(fmt),
                    })
                    .collect(),
            })
            .collect(),
    }
}
