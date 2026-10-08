use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    Build, BuildCoverageReport, BuildId, DraftPlanningBatchUpdate, DraftPlanningSnapshot,
    DraftUpdate, FulfillmentScope, FulfillmentScopeOverride, IndustryError, IndustryRepository,
    MaterialCostQuality, MaterialCoverage, Money, NewBuild, NewWorkspace, OwnerId, PriceSource,
    PriceSourceId, PriceSourceKind, ProductionError, ProductionRepository, QuantityCoverageState,
    UpdatePriceSourceCommand, WorkspaceState,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, ReactionFormulaRecipe, RecipeLine,
    SdeError, SdeReadRepository, TypeSearchResult,
};
use tower::ServiceExt;

#[path = "../support/mod.rs"]
mod support;
use support::inventory::EmptyInventoryRepository;
use support::workspace::{configured_workspace_owned_by, ConfiguredWorkspaceRepository};

#[derive(Default)]
struct FixtureIndustryRepository {
    build: Option<Build>,
    /// Additional builds beyond `build` -- pre-seeded, and also the target
    /// of `create_build`/`update_draft`, which the linked-build flow
    /// genuinely exercises (unlike most fixture methods, which only ever
    /// serve fixed data). `Mutex` for interior mutability since
    /// `IndustryRepository` methods take `&self`.
    linked_builds: std::sync::Mutex<Vec<Build>>,
    price_source: Option<PriceSource>,
    market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
    #[allow(clippy::type_complexity)]
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
    blueprint_observations:
        std::collections::HashMap<uuid::Uuid, iskworks_core::BlueprintObservation>,
    /// Market-evidence coherence fixtures (Part A). When
    /// `live_market_items` is non-empty it is the price source; a
    /// `resolve_market_evidence` freezes a copy under a fresh batch id in
    /// `market_price_snapshots`, and a `derive_market_price_items` given
    /// that evidence prices from the frozen copy -- so a mid-projection
    /// `set_live_price` can't move a later node. Every resolved scope is
    /// recorded in `resolved_evidence_scopes`.
    live_market_items:
        std::sync::Mutex<std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>>,
    #[allow(clippy::type_complexity)]
    market_price_snapshots: std::sync::Mutex<
        std::collections::HashMap<
            uuid::Uuid,
            std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
        >,
    >,
    resolved_evidence_scopes: std::sync::Mutex<Vec<iskworks_core::MarketScope>>,
    /// A price change applied to `live_market_items` on the *first*
    /// `derive_market_price_items` of a request -- models a market refresh
    /// landing after node A was priced but before node B. A pinned read
    /// must ignore it (it prices from the pre-request frozen snapshot).
    mid_request_price_bump: std::sync::Mutex<Option<iskworks_core::PriceSourceItem>>,
    /// When set, `build` is the root of a
    /// canonical-authoritative plan whose Builds are `build` +
    /// `linked_builds` and whose persisted demand edges are these rows.
    canonical: std::sync::Mutex<Option<CanonicalFixture>>,
    /// Counts `load_root_plan` calls (the bounded canonical loader).
    root_plan_loads: std::sync::atomic::AtomicUsize,
}

thread_local! {
    /// Which consumer component each fixture producer Build was created
    /// for -- what `derived_canonical_edges` turns into Produce edges.
    /// `#[tokio::test]` runs on one thread, so this is per test.
    static PRODUCER_OF: std::cell::RefCell<std::collections::HashMap<BuildId, (BuildId, i64)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Register `build` as the producer of `consumer`'s `component`.
fn fixture_link(build: Build, consumer: BuildId, component: i64) -> Build {
    PRODUCER_OF.with(|links| links.borrow_mut().insert(build.id, (consumer, component)));
    build
}

/// Re-point a fixture producer at another component of the same consumer.
fn fixture_relink(build: &Build, component: i64) {
    PRODUCER_OF.with(|links| {
        if let Some(slot) = links.borrow_mut().get_mut(&build.id) {
            slot.1 = component;
        }
    });
}

/// Make a fixture Build a root again (no producer slot).
fn fixture_unlink(build: BuildId) {
    PRODUCER_OF.with(|links| links.borrow_mut().remove(&build));
}

fn producer_of(build: BuildId) -> Option<(BuildId, i64)> {
    PRODUCER_OF.with(|links| links.borrow().get(&build).copied())
}

/// Canonical demand edges for a fixture plan: one per distinct material of
/// every Build, `Produce` (pointing at the parent-linked child Build, or a
/// `shared` override) where the consumer's draft resolves it, else `Buy`.
fn derived_canonical_edges(
    root: &Build,
    builds: &[Build],
    shared: &[(BuildId, i64, BuildId)],
) -> Vec<iskworks_core::production_dependency::PersistedProductionDependency> {
    use iskworks_core::production_dependency::{
        DependencySourcing, PersistedProductionDependency, ProductionMethod,
    };
    let all: Vec<&Build> = std::iter::once(root).chain(builds).collect();
    let mut edges = Vec::new();
    for build in &all {
        let draft = build.draft_planning.as_ref().map(|d| &d.input);
        let mut seen = std::collections::BTreeSet::new();
        for line in build.recipe.materials() {
            if !seen.insert(line.type_id) {
                continue;
            }
            let resolution = draft.and_then(|d| {
                d.component_resolutions
                    .iter()
                    .rev()
                    .find(|r| r.type_id == line.type_id)
            });
            let full = draft.is_some_and(|d| {
                d.fulfillment_scopes
                    .iter()
                    .any(|s| s.type_id == line.type_id && s.scope == FulfillmentScope::Full)
            });
            let linked_child = all
                .iter()
                .find(|c| producer_of(c.id) == Some((build.id, line.type_id)));
            let (sourcing, producer) = match (resolution, linked_child) {
                (Some(resolution), _) => {
                    let method = ProductionMethod::from(resolution.recipe);
                    let child = all.iter().find(|c| {
                        producer_of(c.id) == Some((build.id, line.type_id))
                            && ProductionMethod::from(iskworks_core::recipe_selection_of(&c.recipe))
                                == method
                    });
                    (DependencySourcing::Produce { method }, child.map(|c| c.id))
                }
                // A fixture producer-slot child with no saved resolution:
                // the child is that component's producer.
                (None, Some(child)) => (
                    DependencySourcing::Produce {
                        method: ProductionMethod::from(iskworks_core::recipe_selection_of(
                            &child.recipe,
                        )),
                    },
                    Some(child.id),
                ),
                (None, None) => (DependencySourcing::Buy, None),
            };
            let producer = shared
                .iter()
                .find(|(consumer, component, _)| {
                    *consumer == build.id && *component == line.type_id
                })
                .map(|(_, _, producer)| Some(*producer))
                .unwrap_or(producer);
            edges.push(PersistedProductionDependency {
                id: uuid::Uuid::from_u128(
                    build.id.0.as_u128()
                        ^ u128::from(line.type_id.unsigned_abs())
                            .wrapping_mul(0x9E37_79B9_7F4A_7C15),
                ),
                plan_root_build_id: root.id,
                consumer_build_id: build.id,
                component_type_id: line.type_id,
                sourcing,
                producer_build_id: producer,
                fulfillment_scope: if full {
                    FulfillmentScope::Full
                } else {
                    FulfillmentScope::Missing
                },
                revision: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            });
        }
    }
    edges
}

/// A canonical root plan's persisted edges (see
/// `FixtureIndustryRepository::canonical`).
#[derive(Clone, Default)]
struct CanonicalFixture {
    edges: Vec<iskworks_core::production_dependency::PersistedProductionDependency>,
    retired: Vec<BuildId>,
}

impl FixtureIndustryRepository {
    fn all_builds(&self) -> Vec<Build> {
        self.build
            .iter()
            .cloned()
            .chain(self.linked_builds.lock().unwrap().iter().cloned())
            .collect()
    }

    /// This plan's canonical edges, materialized once (stable edge ids across
    /// a load and the write that references them). `None` for a fixture
    /// without a root.
    fn canonical_fixture(&self) -> Option<CanonicalFixture> {
        let root = self.build.as_ref()?;
        if let Some(explicit) = self.canonical.lock().unwrap().clone() {
            return Some(explicit);
        }
        // Not yet written to: re-derived on every read (deterministic edge
        // ids keep a load and a later write consistent), so producer Builds
        // a test adds mid-way are picked up.
        let builds = self.linked_builds.lock().unwrap().clone();
        Some(CanonicalFixture {
            edges: derived_canonical_edges(root, &builds, &[]),
            retired: Vec::new(),
        })
    }

    fn canonical_records(&self) -> Option<iskworks_core::production_dependency::RootPlanRecords> {
        let canonical = self.canonical_fixture()?;
        Some(iskworks_core::production_dependency::RootPlanRecords {
            root: self.build.clone()?,
            producers: self.linked_builds.lock().unwrap().clone(),
            dependencies: canonical.edges,
            retired_producers: canonical.retired,
        })
    }

    fn set_live_price(&self, item: iskworks_core::PriceSourceItem) {
        self.live_market_items
            .lock()
            .unwrap()
            .insert(item.type_id, item);
    }
    fn arm_mid_request_bump(&self, item: iskworks_core::PriceSourceItem) {
        *self.mid_request_price_bump.lock().unwrap() = Some(item);
    }
    fn resolved_evidence_scopes(&self) -> Vec<iskworks_core::MarketScope> {
        self.resolved_evidence_scopes.lock().unwrap().clone()
    }
    fn live_price(&self, type_id: i64) -> Option<iskworks_core::Money> {
        self.live_market_items
            .lock()
            .unwrap()
            .get(&type_id)
            .map(|item| item.price)
    }
}

#[async_trait]
impl IndustryRepository for FixtureIndustryRepository {
    async fn list_builds(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
    ) -> Result<Vec<Build>, IndustryError> {
        Ok(self
            .build
            .iter()
            .cloned()
            .chain(self.linked_builds.lock().unwrap().iter().cloned())
            .filter(|build| producer_of(build.id).is_none())
            .collect())
    }

    async fn get_build(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        build_id: BuildId,
    ) -> Result<Build, IndustryError> {
        self.build
            .iter()
            .cloned()
            .chain(self.linked_builds.lock().unwrap().iter().cloned())
            .find(|build| build.id == build_id)
            .ok_or(IndustryError::BuildNotFound)
    }

    async fn create_build(&self, new_build: NewBuild) -> Result<Build, IndustryError> {
        let build = new_build.build;
        self.linked_builds.lock().unwrap().push(build.clone());
        Ok(build)
    }

    async fn plan_root_of(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        build_id: BuildId,
    ) -> Result<Option<BuildId>, IndustryError> {
        if self.canonical_fixture().is_none() {
            return Ok(None);
        }
        let root = self.build.as_ref().map(|build| build.id);
        Ok(self
            .all_builds()
            .iter()
            .any(|build| build.id == build_id)
            .then_some(root)
            .flatten())
    }

    async fn load_root_plan(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _root_build_id: BuildId,
    ) -> Result<iskworks_core::production_dependency::RootPlanRecords, IndustryError> {
        self.root_plan_loads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.canonical_records()
            .ok_or_else(|| IndustryError::Persistence("fixture has no canonical plan".to_string()))
    }

    /// In-memory mirror of `PgIndustryRepository::apply_canonical_consumer_write`:
    /// plan-state check, consumer revision check, producers created once,
    /// edge writes -- all or nothing.
    async fn apply_canonical_consumer_write(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        write: iskworks_core::canonical_planner::CanonicalConsumerWrite,
    ) -> Result<Build, IndustryError> {
        use iskworks_core::canonical_planner::{CanonicalWriteError, PlannedProducer};
        use iskworks_core::production_dependency::{
            DependencySourcing, PersistedProductionDependency,
        };
        let records = self
            .canonical_records()
            .ok_or_else(|| IndustryError::Persistence("not canonical".to_string()))?;
        if iskworks_core::plan_state::plan_state(&records) != write.expected_plan_state {
            return Err(CanonicalWriteError::PlanChanged.into());
        }
        let derived = self.canonical_fixture().expect("checked above");
        let mut builds = self.linked_builds.lock().unwrap();
        let mut canonical = self.canonical.lock().unwrap();
        // The first write materializes derived edges; from then on they're
        // stored state.
        let canonical = canonical.get_or_insert(derived);
        let mut root = self.build.clone().expect("canonical fixture has a root");
        let consumer = if root.id == write.consumer {
            &mut root
        } else {
            builds
                .iter_mut()
                .find(|build| build.id == write.consumer)
                .ok_or(IndustryError::BuildNotFound)?
        };
        if consumer.revision != write.update.expected_revision {
            return Err(IndustryError::RevisionConflict);
        }
        consumer.name = write.update.name.clone();
        consumer.runs = write.update.runs;
        consumer.notes = write.update.notes.clone();
        if let Some(recipe) = write.update.replacement_recipe.clone() {
            consumer.recipe = recipe;
        }
        consumer.draft_planning = write.update.draft_planning.clone();
        consumer.revision += 1;
        let updated = consumer.clone();
        if root.id == write.consumer {
            // `self.build` is immutable in this fixture; root writes are
            // reflected through the returned Build only.
        }
        let mut created: Vec<(
            iskworks_core::production_dependency::CanonicalProducerKey,
            BuildId,
        )> = Vec::new();
        for producer in &write.new_producers {
            if created.iter().any(|(key, _)| *key == producer.key) {
                continue;
            }
            builds.push(producer.build.clone());
            let mut seen = std::collections::BTreeSet::new();
            for line in producer.build.recipe.materials() {
                if !seen.insert(line.type_id) {
                    continue;
                }
                canonical.edges.push(PersistedProductionDependency {
                    id: uuid::Uuid::new_v4(),
                    plan_root_build_id: write.root,
                    consumer_build_id: producer.build.id,
                    component_type_id: line.type_id,
                    sourcing: DependencySourcing::Buy,
                    producer_build_id: None,
                    fulfillment_scope: FulfillmentScope::Missing,
                    revision: 1,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                });
            }
            created.push((producer.key, producer.build.id));
        }
        for edge_write in &write.edge_writes {
            let edge = canonical
                .edges
                .iter_mut()
                .find(|edge| edge.id == edge_write.dependency_id)
                .ok_or_else(|| IndustryError::from(CanonicalWriteError::PlanChanged))?;
            edge.sourcing = edge_write.sourcing;
            edge.fulfillment_scope = edge_write.fulfillment_scope;
            edge.producer_build_id = match edge_write.producer {
                PlannedProducer::None => None,
                PlannedProducer::Existing { producer } => Some(producer),
                PlannedProducer::Create { key } => created
                    .iter()
                    .find(|(created_key, _)| *created_key == key)
                    .map(|(_, id)| *id),
            };
            edge.revision += 1;
        }
        Ok(updated)
    }

    async fn update_draft(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        build_id: BuildId,
        update: DraftUpdate,
    ) -> Result<Build, IndustryError> {
        let mut linked = self.linked_builds.lock().unwrap();
        let build = linked
            .iter_mut()
            .find(|build| build.id == build_id)
            .ok_or(IndustryError::RevisionConflict)?;
        if build.revision != update.expected_revision {
            return Err(IndustryError::RevisionConflict);
        }
        build.name = update.name;
        build.runs = update.runs;
        build.notes = update.notes;
        if let Some(recipe) = update.replacement_recipe {
            build.recipe = recipe;
        }
        build.draft_planning = update.draft_planning;
        build.revision += 1;
        Ok(build.clone())
    }

    async fn update_draft_planning_batch(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        updates: Vec<DraftPlanningBatchUpdate>,
    ) -> Result<Vec<Build>, IndustryError> {
        let mut linked = self.linked_builds.lock().unwrap();
        // Validate every member's revision before mutating any of them --
        // mirrors `PgIndustryRepository`'s own one-transaction, no-partial-
        // application contract.
        for update in &updates {
            let build = linked
                .iter()
                .find(|build| build.id == update.build_id)
                .ok_or(IndustryError::RevisionConflict)?;
            if build.revision != update.expected_revision {
                return Err(IndustryError::RevisionConflict);
            }
        }
        let mut results = Vec::with_capacity(updates.len());
        for update in updates {
            let build = linked
                .iter_mut()
                .find(|build| build.id == update.build_id)
                .expect("just validated above");
            build.draft_planning = Some(DraftPlanningSnapshot {
                input: update.input,
                updated_at: chrono::Utc::now(),
            });
            build.revision += 1;
            results.push(build.clone());
        }
        Ok(results)
    }

    async fn rename_build(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _build_id: BuildId,
        _name: String,
    ) -> Result<Build, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn delete_build(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _build_id: BuildId,
        _expected_revision: u64,
        _force: bool,
    ) -> Result<(), IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn list_price_sources(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
    ) -> Result<Vec<PriceSource>, IndustryError> {
        Ok(Vec::new())
    }

    async fn get_price_source(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        self.price_source
            .clone()
            .ok_or(IndustryError::PriceSourceNotFound)
    }

    async fn create_price_source(
        &self,
        _source: PriceSource,
    ) -> Result<PriceSource, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn update_price_source(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
        _command: UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn upsert_price_items(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
        _expected_revision: u64,
        _items: Vec<iskworks_core::PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn remove_price_item(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
        _type_id: i64,
        _expected_revision: u64,
    ) -> Result<PriceSource, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn delete_price_source(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
        _expected_revision: u64,
    ) -> Result<(), IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn derive_market_price_items(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _scope: iskworks_core::MarketScope,
        requests: Vec<iskworks_core::MarketPriceRequest>,
        evidence: Option<&iskworks_core::MarketScopeEvidence>,
    ) -> Result<Vec<iskworks_core::PriceSourceItem>, IndustryError> {
        // A refresh "lands" mid-request: mutate live prices once. A pinned
        // read below must not see this.
        if let Some(bump) = self.mid_request_price_bump.lock().unwrap().take() {
            self.live_market_items
                .lock()
                .unwrap()
                .insert(bump.type_id, bump);
        }
        // Evidence-coherence fixture path: price from the frozen snapshot
        // this evidence pinned, never from live prices.
        if let Some(evidence) = evidence {
            if let Some(batch_id) = evidence.observation_batch_ids.first() {
                if let Some(frozen) = self.market_price_snapshots.lock().unwrap().get(&batch_id.0) {
                    return Ok(requests
                        .into_iter()
                        .filter_map(|request| frozen.get(&request.type_id).cloned())
                        .collect());
                }
            }
        }
        let live = self.live_market_items.lock().unwrap();
        let source = if live.is_empty() {
            &self.market_items
        } else {
            &*live
        };
        Ok(requests
            .into_iter()
            .filter_map(|request| source.get(&request.type_id).cloned())
            .collect())
    }

    async fn resolve_market_evidence(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<iskworks_core::MarketScopeEvidence, IndustryError> {
        self.resolved_evidence_scopes.lock().unwrap().push(scope);
        let as_of = chrono::Utc::now();
        // Freeze whatever prices are live right now under a fresh batch id.
        let live = self.live_market_items.lock().unwrap().clone();
        if live.is_empty() {
            return Ok(iskworks_core::MarketScopeEvidence::unpinned(scope, as_of));
        }
        let batch_id = uuid::Uuid::new_v4();
        self.market_price_snapshots
            .lock()
            .unwrap()
            .insert(batch_id, live);
        Ok(iskworks_core::MarketScopeEvidence {
            scope,
            observation_batch_ids: vec![iskworks_core::MarketObservationBatchId(batch_id)],
            import_batch_id: None,
            observed_at: Some(as_of),
            as_of,
        })
    }

    async fn get_facility_profile(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        facility_id: iskworks_core::FacilityProfileId,
    ) -> Result<iskworks_core::IndustryFacilityProfile, IndustryError> {
        self.facility_profiles
            .get(&facility_id)
            .cloned()
            .ok_or(IndustryError::Facility(
                iskworks_core::FacilityError::NotFound,
            ))
    }

    async fn get_blueprint_observation(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        observation_id: uuid::Uuid,
    ) -> Result<iskworks_core::BlueprintObservation, IndustryError> {
        self.blueprint_observations
            .get(&observation_id)
            .cloned()
            .ok_or(IndustryError::Blueprint(
                iskworks_core::BlueprintError::ObservationNotFound,
            ))
    }
}

/// Root blueprint 6_830 needs Tritanium (buy) and Rifter Hull Section
/// (90_001, resolvable via blueprint 90_002, which itself needs Pyerite) --
/// a real two-level chain for testing component_resolutions end to end.
struct FixtureSdeRepository;

#[async_trait]
impl SdeReadRepository for FixtureSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: uuid::Uuid::nil(),
            source_version: "123456".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: chrono::Utc::now(),
            counts: iskworks_sde::ImportCounts {
                types: 4,
                blueprints: 2,
                material_lines: 3,
                product_lines: 2,
                skipped_blueprints: 0,
                ..Default::default()
            },
        }))
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let recipe = match blueprint_type_id {
            6_830 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Rifter Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![
                    RecipeLine {
                        type_id: 34,
                        type_name: "Tritanium".to_string(),
                        quantity: 100,
                    },
                    RecipeLine {
                        type_id: 90_001,
                        type_name: "Rifter Hull Section".to_string(),
                        quantity: 2,
                    },
                ],
                products: vec![RecipeLine {
                    type_id: 5_876,
                    type_name: "Rifter".to_string(),
                    quantity: 1,
                }],
            },
            90_002 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Rifter Hull Section Blueprint".to_string(),
                duration_seconds: Some(300),
                materials: vec![RecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity: 50,
                }],
                products: vec![RecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity: 1,
                }],
            },
            // A fictional third level, only used by the recursive
            // plan-graph chain test -- a component that itself resolves
            // to Build, needing Tritanium (34), the same type_id the root
            // Rifter blueprint also needs directly, so that test doubles
            // as coverage for "a type_id only present at the deepest
            // level still merges with the root's own demand."
            91_002 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Pyerite Reprocessing Blueprint".to_string(),
                duration_seconds: Some(120),
                materials: vec![RecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity: 10,
                }],
                products: vec![RecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity: 1,
                }],
            },
            // Alternate Rifter Hull Section blueprint used only by the
            // graph<->worksheet fidelity suite: two direct BUY materials at
            // very different per-run quantities (50 and 1), so ME rounding
            // and a `max(runs, ...)` floor are both exercised, and an
            // accidental Need-vs-Making substitution is easy to spot.
            90_010 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Rifter Hull Section Blueprint (alt)".to_string(),
                duration_seconds: Some(300),
                materials: vec![
                    RecipeLine {
                        type_id: 35,
                        type_name: "Pyerite".to_string(),
                        quantity: 50,
                    },
                    RecipeLine {
                        type_id: 60_001,
                        type_name: "Fidelity Isotope".to_string(),
                        quantity: 1,
                    },
                ],
                products: vec![RecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity: 1,
                }],
            },
            // Discrete-output surplus fixture: parent bp 92_010 consumes 250
            // of the batched component 92_001 per run; the surplus-via-graph
            // test drives the requirement straight from this SDE recipe, and
            // the run-boundary test overrides it (250/500/501) through each
            // parent's embedded CapturedRecipe. Child bp 92_002 produces
            // 92_001 in lots of 500 per run.
            92_010 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Batch Widget Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![RecipeLine {
                    type_id: 92_001,
                    type_name: "Batched Component".to_string(),
                    quantity: 250,
                }],
                products: vec![RecipeLine {
                    type_id: 92_100,
                    type_name: "Batch Widget".to_string(),
                    quantity: 1,
                }],
            },
            92_002 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Batched Component Blueprint".to_string(),
                duration_seconds: Some(300),
                materials: vec![RecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity: 10,
                }],
                products: vec![RecipeLine {
                    type_id: 92_001,
                    type_name: "Batched Component".to_string(),
                    quantity: 500,
                }],
            },
            // Realistic multi-operation fixture: a cruiser-scale
            // hull needing three materials, two of them Build-resolved
            // (one -- Hull Section -- itself Build-resolving a deeper
            // material, the other -- Batched Component -- discrete-output),
            // alongside a plain Buy leaf, so one root demonstrates a wide,
            // multi-branch tree without inventing a whole new item chain.
            99_000 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Cruiser Hull Blueprint".to_string(),
                duration_seconds: Some(1_200),
                materials: vec![
                    RecipeLine {
                        type_id: 34,
                        type_name: "Tritanium".to_string(),
                        quantity: 20,
                    },
                    RecipeLine {
                        type_id: 90_001,
                        type_name: "Rifter Hull Section".to_string(),
                        quantity: 2,
                    },
                    RecipeLine {
                        type_id: 92_001,
                        type_name: "Batched Component".to_string(),
                        quantity: 3,
                    },
                ],
                products: vec![RecipeLine {
                    type_id: 99_100,
                    type_name: "Cruiser Hull".to_string(),
                    quantity: 1,
                }],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }

    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let known: std::collections::BTreeMap<i64, &str> = std::collections::BTreeMap::from([
            (34, "Tritanium"),
            (35, "Pyerite"),
            (5_876, "Rifter"),
            (90_001, "Rifter Hull Section"),
            (60_001, "Fidelity Isotope"),
            (92_001, "Batched Component"),
            (92_100, "Batch Widget"),
        ]);
        Ok(type_ids
            .iter()
            .filter_map(|id| known.get(id).map(|name| (*id, (*name).to_string())))
            .collect())
    }

    /// Reverse recipe lookup over this fixture's own recipe table, so the
    /// graph's root-buildability probe (`resolve_buildable_recipe`) can tell
    /// a raw material (Tritanium) from one with a recipe (Rifter Hull
    /// Section, Pyerite).
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok(match product_type_id {
            5_876 => Some(6_830),
            90_001 => Some(90_002),
            35 => Some(91_002),
            92_001 => Some(92_002),
            92_100 => Some(92_010),
            _ => None,
        })
    }

    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        // Alternate reaction-formula route to Rifter Hull Section (90_001),
        // instead of manufacturing blueprint 90_002 -- used only by the
        // blueprint-selection-on-a-reaction rejection test, to prove the
        // validation fires before ever getting to material math, not
        // because this specific recipe matters.
        let recipe = match reaction_formula_type_id {
            90_003 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Rifter Hull Section Reaction Formula".to_string(),
                duration_seconds: Some(50),
                materials: vec![RecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity: 50,
                }],
                products: vec![RecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity: 1,
                }],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }

    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::TypeReference>, SdeError> {
        // Enough of the recipe chain's types for the verification workbook's
        // `Types` sheet + name lookups. Unknown ids are simply omitted.
        let name = |id: i64| -> Option<(&'static str, &'static str, &'static str)> {
            Some(match id {
                34 => ("Tritanium", "Mineral", "Material"),
                35 => ("Pyerite", "Mineral", "Material"),
                90_001 => ("Rifter Hull Section", "Ship Modules", "Module"),
                5_876 | 5_875 => ("Rifter", "Frigate", "Ship"),
                6_830 => ("Rifter Blueprint", "Ship Blueprint", "Blueprint"),
                90_002 => (
                    "Rifter Hull Section Blueprint",
                    "Ship Blueprint",
                    "Blueprint",
                ),
                90_003 => (
                    "Rifter Hull Section Reaction Formula",
                    "Reaction Formulas",
                    "Reaction",
                ),
                91_002 => (
                    "Pyerite Reprocessing Blueprint",
                    "Ship Blueprint",
                    "Blueprint",
                ),
                92_001 => ("Batched Component", "Components", "Material"),
                92_002 => ("Batched Component Blueprint", "Ship Blueprint", "Blueprint"),
                92_010 => ("Batch Widget Blueprint", "Ship Blueprint", "Blueprint"),
                92_100 => ("Batch Widget", "Frigate", "Ship"),
                _ => return None,
            })
        };
        Ok(type_ids
            .iter()
            .filter_map(|id| {
                name(*id).map(|(type_name, group, category)| {
                    (
                        *id,
                        iskworks_sde::TypeReference {
                            type_name: Some(type_name.to_string()),
                            group_id: Some(1),
                            group_name: Some(group.to_string()),
                            category_id: Some(2),
                            category_name: Some(category.to_string()),
                            packaged_volume_m3: Some(rust_decimal::Decimal::new(1, 2)), // 0.01
                        },
                    )
                })
            })
            .collect())
    }
}

fn price_item(type_id: i64, name: &str, price: &str) -> iskworks_core::PriceSourceItem {
    iskworks_core::PriceSourceItem {
        type_id,
        type_name: name.to_string(),
        price: iskworks_core::Money::parse(price).unwrap(),
        note: String::new(),
        updated_at: chrono::Utc::now(),
    }
}

fn app() -> axum::Router {
    let new_workspace = NewWorkspace::manual("Industry".to_string());
    let workspace_state = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
    build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: workspace_state,
        }))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: None,
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        })),
    )
}

fn app_with_price_source_and_sde(price_source: PriceSource) -> axum::Router {
    let new_workspace = NewWorkspace::manual("Industry".to_string());
    let workspace_state = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
    build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: workspace_state,
        }))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: None,
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository)),
    )
}

fn app_with_order_book_price_source_and_sde(
    price_source: PriceSource,
    market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
) -> axum::Router {
    let new_workspace = NewWorkspace::manual("Industry".to_string());
    let workspace_state = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
    build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: workspace_state,
        }))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: None,
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: Some(price_source),
            market_items,
            facility_profiles: std::collections::HashMap::new(),
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository)),
    )
}

/// A manual price source with no items -- for previews that only need to
/// exercise quantity/facility calculation, not pricing completeness.
fn empty_manual_price_source() -> PriceSource {
    PriceSource {
        id: PriceSourceId::new(),
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Home Market".to_string(),
        description: String::new(),
        kind: PriceSourceKind::Manual,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn app_with_price_source_sde_and_facilities(
    price_source: PriceSource,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> axum::Router {
    let new_workspace = NewWorkspace::manual("Industry".to_string());
    let workspace_state = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
    build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: workspace_state,
        }))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: None,
            linked_builds: std::sync::Mutex::new(Vec::new()),
            price_source: Some(price_source),
            market_items: std::collections::BTreeMap::new(),
            facility_profiles,
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository)),
    )
}

/// Router for the linked-build tests -- pins `owner_id` (needed since
/// `create_or_reuse_linked_build` checks the loaded parent's `owner_id`
/// against the caller's), and seeds `linked_builds` with any
/// already-existing children the test wants in play.
fn app_with_linked_build_fixture(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
) -> axum::Router {
    app_with_linked_build_fixture_facilities(
        parent,
        owner_id,
        linked_builds,
        std::collections::HashMap::new(),
    )
}

fn app_with_linked_build_fixture_facilities(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles,
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        // `Missing` is the default fulfillment scope now, so every
        // `create_linked_build` call needs a working `coverage()` lookup
        // even when the test isn't exercising fulfillment scope itself --
        // an empty/no-inventory answer for a type_id no fixture build
        // actually has is a harmless default.
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

/// An `AdjustedPriceRepository` stand-in wired via
/// `AppState::with_adjusted_price_repository` -- the testability seam that
/// lets an interactive-preview test give `BuildCostProjection`'s EIV/
/// installation math real adjusted prices without a full
/// `EsiApplicationService`/Postgres. Returns exactly the fixed map it was
/// constructed with; a `type_id` absent from that map is reported missing,
/// never substituted.
struct FixtureAdjustedPriceRepository {
    prices: std::collections::BTreeMap<i64, rust_decimal::Decimal>,
}

#[async_trait]
impl iskworks_core::AdjustedPriceRepository for FixtureAdjustedPriceRepository {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<std::collections::BTreeMap<i64, rust_decimal::Decimal>, iskworks_core::InventoryError>
    {
        Ok(type_ids
            .iter()
            .filter_map(|type_id| self.prices.get(type_id).map(|price| (*type_id, *price)))
            .collect())
    }
}

/// A `ProductionRepository` stand-in that only implements `coverage` --
/// every other method panics if called, since no test exercising this
/// fixture is expected to reach them. `coverage` always returns a single
/// `MaterialCoverage` row for whichever `(type_id, available_to_this_build)`
/// the test configures.
struct FixtureProductionRepository {
    type_id: i64,
    available_to_this_build: u64,
    average_historical_unit_cost: Option<Money>,
}

#[async_trait]
impl ProductionRepository for FixtureProductionRepository {
    async fn coverage(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        build_id: BuildId,
    ) -> Result<BuildCoverageReport, ProductionError> {
        Ok(BuildCoverageReport {
            build_id,
            owner_id: OwnerId::new(),
            build_revision: 1,
            recipe_fingerprint: "recipe".to_string(),
            runs: 1,
            complete_quantity_coverage: false,
            complete_cost_coverage: false,
            material_lines: vec![MaterialCoverage {
                type_id: self.type_id,
                type_name: "Fixture Material".to_string(),
                sort_order: 0,
                required_quantity: 0,
                accounted_owned_quantity: self.available_to_this_build,
                reserved_for_this_build: 0,
                reserved_by_other_builds: 0,
                unreserved_available_quantity: self.available_to_this_build,
                available_to_this_build: self.available_to_this_build,
                reservable_additional_quantity: self.available_to_this_build,
                covered_quantity: 0,
                missing_quantity: 0,
                average_historical_unit_cost: self.average_historical_unit_cost,
                projected_historical_cost: None,
                cost_quality: if self.average_historical_unit_cost.is_some() {
                    MaterialCostQuality::Known
                } else {
                    MaterialCostQuality::Unresolved
                },
                quantity_coverage_state: QuantityCoverageState::NoInventory,
                esi_observed_quantity: None,
                esi_reconciliation_difference: None,
                esi_observed_at: None,
                explanation: String::new(),
                warnings: Vec::new(),
                inventory_revision: 1,
            }],
            warnings: Vec::new(),
        })
    }

    async fn reserved_quantity(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<u64, ProductionError> {
        // `preview_create_build_plan_candidate` calls this for every
        // material line once a production repository is wired in at all --
        // not specific to fulfillment scope, just something every
        // candidate-preview test that provides one needs a real answer for.
        Ok(0)
    }

    async fn list_reservations(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<Vec<iskworks_core::InventoryReservation>, ProductionError> {
        unimplemented!("not exercised by any test using this fixture")
    }

    async fn list_esi_observations(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<std::collections::BTreeMap<i64, iskworks_core::EsiObservation>, ProductionError>
    {
        unimplemented!("not exercised by any test using this fixture")
    }

    async fn esi_holdings(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<iskworks_core::EsiHoldings, ProductionError> {
        unimplemented!("not exercised by any test using this fixture")
    }
}

/// Every method panics -- wired in for tests asserting that an explicit
/// `Full` fulfillment scope skips the inventory-coverage lookup entirely,
/// since a `Full`-scoped row never needs real availability data.
struct NeverCalledProductionRepository;

#[async_trait]
impl ProductionRepository for NeverCalledProductionRepository {
    async fn coverage(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _build_id: BuildId,
    ) -> Result<BuildCoverageReport, ProductionError> {
        unimplemented!("must not be called for an explicitly Full-scoped row")
    }

    async fn reserved_quantity(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<u64, ProductionError> {
        unimplemented!("must not be called for an explicitly Full-scoped row")
    }

    async fn list_reservations(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<Vec<iskworks_core::InventoryReservation>, ProductionError> {
        unimplemented!("must not be called for an explicitly Full-scoped row")
    }

    async fn list_esi_observations(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<std::collections::BTreeMap<i64, iskworks_core::EsiObservation>, ProductionError>
    {
        unimplemented!("must not be called for an explicitly Full-scoped row")
    }

    async fn esi_holdings(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<iskworks_core::EsiHoldings, ProductionError> {
        unimplemented!("must not be called for an explicitly Full-scoped row")
    }
}

mod build_settings;
mod candidate_preview_costing;
mod candidate_preview_overrides;
mod graph_fidelity;
mod graph_market_evidence;
mod graph_test_matrix;
mod linked_builds;
mod materials;
mod pricing_policy;
mod whole_tree_allocation;

// ===========================================================================
// Build verification-workbook export
// (`POST /api/builds/:id/export-verification`) -- the first engineering /
// audit `.xlsx` export. Reuses the Materials fixtures (`nested_chain`,
// `overlay`, `hull_section_built`, `materials_app` recipe chain): Rifter
// 6_830 -> Hull Section 90_001 [BUILD] -> Pyerite 35 [BUILD] -> Tritanium 34.
// ===========================================================================

mod verification_export;

/// Integration coverage for the allocation-aware
/// `BuildCostProjection` (`POST /api/builds/:id/cost-projection`). The API
/// fixtures wire no `EsiApplicationService`, so `own_installation` is always
/// incomplete here -- installation arithmetic is covered by the pure
/// `iskworks_core::build_cost` unit tests. These prove the plumbing: one
/// `list_balances`, no inventory writes, real allocation-aware evidence, the
/// overlay (not persisted state) drives it, and the material-side conservation
/// identities hold end-to-end over the real traversal.
mod build_cost_projection;

// =============================================================================
// Execution Plan -- POST /api/builds/:build_id/execution-plan
// =============================================================================
//
// Reuses the Rifter / nested-chain fixtures (`rifter_root`, `nested_chain`,
// `overlay`, `hull_section_built`, `FixtureIndustryRepository`,
// `FixtureSdeRepository`) as-is for unsaved-overlay parity and the
// inventory-threshold regression, so those tests prove parity against the
// exact same evidence `/materials` already exercises. The
// grouping-specific tests (Neo Mercurite, incompatible facility, Reaction
// chain) need topology the shared fixture doesn't have (a shared
// intermediate consumed by two branches, a deep Reaction chain), so they
// use a small, self-contained `ExecutionPlanSde` + hand-built linked
// builds instead of extending the shared `FixtureSdeRepository`.
mod execution_plan_api;

mod build_worksheet_api;

// ---------------------------------------------------------------------
// Stages descendant-production-configuration mutation: `PATCH /api/builds/:build_id/descendant-production-
// configuration` atomically applies one facility/blueprint patch to every
// canonical member Build a production operation currently represents, after
// re-validating the requested members still form exactly one current
// operation under the live plan.
// ---------------------------------------------------------------------
mod stages_descendant_production_configuration;

// ===========================================================================
// The canonical dependency-graph planner,
// end to end through the HTTP routes (fake repository, no Postgres).
// ===========================================================================
mod canonical_producers;

// ---------------------------------------------------------------------------
// Linked-build fixtures
// ---------------------------------------------------------------------------

/// An app where every build (root + children) lives in the mutable
/// `linked_builds` store, so `PUT /api/builds/:id` and the follow-up
/// descendant resync can both persist. No inventory (coverage answers 0).
fn app_with_mutable_build_tree(owner_id: OwnerId, builds: Vec<Build>) -> axum::Router {
    app_with_mutable_build_tree_and_facilities(owner_id, builds, std::collections::HashMap::new())
}

/// [`app_with_mutable_build_tree_and_facilities`] plus the blueprint
/// observations the fixture's `get_blueprint_observation` serves.
fn app_with_mutable_build_tree_facilities_and_observations(
    owner_id: OwnerId,
    builds: Vec<Build>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
    blueprint_observations: std::collections::HashMap<
        uuid::Uuid,
        iskworks_core::BlueprintObservation,
    >,
) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: None,
            linked_builds: std::sync::Mutex::new(builds),
            price_source: None,
            market_items: std::collections::BTreeMap::new(),
            facility_profiles,
            blueprint_observations,
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

async fn get_build_json(app: &axum::Router, id: BuildId) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/builds/{}", id.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

/// A linked child for 90_001 (recipe 90_002) that itself Build-resolves its
/// own Pyerite (35) via blueprint 91_002 -- for the nested-chain test.
fn hull_child_resolving_pyerite(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_id: BuildId,
    runs: u64,
) -> Build {
    let now = chrono::Utc::now();
    let mut child = linked_child_build(workspace_id, owner_id, parent_id, 90_001, runs);
    child.draft_planning = Some(iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: vec![iskworks_core::ComponentResolution {
                type_id: 35,
                recipe: iskworks_core::RecipeSelection::Manufacturing {
                    blueprint_type_id: 91_002,
                },
                facility_override: None,
                blueprint_selection: None,
            }],
            fulfillment_scopes: Vec::new(),
        },
        updated_at: now,
    });
    child
}

/// `parent_build_resolving_hull_section_to_build`, but at a caller-chosen
/// run count and blueprint selection so ME actually moves the requirement.
fn hull_parent(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    runs: u64,
    blueprint_selection: Option<iskworks_core::BlueprintSelection>,
) -> Build {
    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent.runs = runs;
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .blueprint_selection = blueprint_selection;
    parent
}

fn linked_child_build(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_build_id: BuildId,
    parent_component_type_id: i64,
    runs: u64,
) -> Build {
    let now = chrono::Utc::now();
    fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Rifter Hull Section build".to_string(),
            recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                blueprint_type_id: 90_002,
                blueprint_name: "Rifter Hull Section Blueprint".to_string(),
                duration_seconds_per_run: Some(300),
                materials: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                    quantity_per_run: 50,
                    sort_order: 0,
                }],
                products: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity_per_run: 1,
                    sort_order: 0,
                }],
                fingerprint: "recipe".to_string(),
            }),
            runs,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: None,
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        parent_build_id,
        parent_component_type_id,
    )
}

/// A Draft "Rifter batch" build (blueprint 6_830, 1 run) resolving its
/// Rifter Hull Section sub-component (90_001, needs 2 per run) to Build
/// via blueprint 90_002 (produces 1 per run) -- the same two-level
/// `FixtureSdeRepository` chain used by the blueprint-efficiency tests.
/// Expect the linked build's `runs` to resolve to 2.
fn parent_build_resolving_hull_section_to_build(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
) -> Build {
    let now = chrono::Utc::now();
    Build {
        id: BuildId::new(),
        workspace_id,
        owner_id,
        name: "Rifter batch".to_string(),
        recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
            source_sde_dataset_id: uuid::Uuid::new_v4(),
            source_sde_version: "test".to_string(),
            blueprint_type_id: 6_830,
            blueprint_name: "Rifter Blueprint".to_string(),
            duration_seconds_per_run: Some(600),
            materials: vec![
                iskworks_core::CapturedRecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity_per_run: 100,
                    sort_order: 0,
                },
                iskworks_core::CapturedRecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity_per_run: 2,
                    sort_order: 1,
                },
            ],
            products: vec![iskworks_core::CapturedRecipeLine {
                type_id: 5_876,
                type_name: "Rifter".to_string(),
                quantity_per_run: 1,
                sort_order: 0,
            }],
            fingerprint: "recipe".to_string(),
        }),
        runs: 1,
        notes: String::new(),
        revision: 1,
        created_at: now,
        updated_at: now,
        draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
            input: iskworks_core::DraftPlanningInput {
                material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                manual_price_list_id: None,
                expected_manual_price_list_revision: None,
                material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                pricing_selections: Vec::new(),
                blueprint_selection: None,
                manufacturing_facility: None,
                reaction_facility: None,
                facility_eiv_manual: false,
                component_resolutions: vec![iskworks_core::ComponentResolution {
                    type_id: 90_001,
                    recipe: iskworks_core::RecipeSelection::Manufacturing {
                        blueprint_type_id: 90_002,
                    },
                    facility_override: None,
                    blueprint_selection: None,
                }],
                fulfillment_scopes: Vec::new(),
            },
            updated_at: now,
        }),
        recipe_currency: iskworks_core::RecipeCurrency::Current,
        active_sde_version: Some("test".to_string()),
        product_category_name: Some("Ship".to_string()),
        product_group_name: Some("Battleship".to_string()),
        selected_blueprint_origin: None,
        has_owned_blueprint: true,
    }
}

/// [`app_with_mutable_build_tree`] plus a set of live facility profiles the
/// fixture's `get_facility_profile` serves -- for resync tests that need a
/// parent facility whose current revision differs from the one the parent's
/// persisted planning input recorded.
fn app_with_mutable_build_tree_and_facilities(
    owner_id: OwnerId,
    builds: Vec<Build>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> axum::Router {
    app_with_mutable_build_tree_facilities_and_observations(
        owner_id,
        builds,
        facility_profiles,
        std::collections::HashMap::new(),
    )
}

// ---------------------------------------------------------------------------
// Build settings and facility fixtures
// ---------------------------------------------------------------------------

/// A persisted parent Build for bp 92_010 whose embedded recipe needs
/// `component_qty_per_run` of 92_001 per run (runs = 1), resolving 92_001 to
/// Build via bp 92_002.
fn batch_parent(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    component_qty_per_run: u64,
) -> Build {
    let now = chrono::Utc::now();
    Build {
        id: BuildId::new(),
        workspace_id,
        owner_id,
        name: "Batch Widget build".to_string(),
        recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
            source_sde_dataset_id: uuid::Uuid::new_v4(),
            source_sde_version: "test".to_string(),
            blueprint_type_id: 92_010,
            blueprint_name: "Batch Widget Blueprint".to_string(),
            duration_seconds_per_run: Some(600),
            materials: vec![iskworks_core::CapturedRecipeLine {
                type_id: 92_001,
                type_name: "Batched Component".to_string(),
                quantity_per_run: component_qty_per_run,
                sort_order: 0,
            }],
            products: vec![iskworks_core::CapturedRecipeLine {
                type_id: 92_100,
                type_name: "Batch Widget".to_string(),
                quantity_per_run: 1,
                sort_order: 0,
            }],
            fingerprint: "recipe".to_string(),
        }),
        runs: 1,
        notes: String::new(),
        revision: 1,
        created_at: now,
        updated_at: now,
        draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
            input: iskworks_core::DraftPlanningInput {
                material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                manual_price_list_id: None,
                expected_manual_price_list_revision: None,
                material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                pricing_selections: Vec::new(),
                blueprint_selection: None,
                manufacturing_facility: None,
                reaction_facility: None,
                facility_eiv_manual: false,
                component_resolutions: vec![iskworks_core::ComponentResolution {
                    type_id: 92_001,
                    recipe: iskworks_core::RecipeSelection::Manufacturing {
                        blueprint_type_id: 92_002,
                    },
                    facility_override: None,
                    blueprint_selection: None,
                }],
                fulfillment_scopes: Vec::new(),
            },
            updated_at: now,
        }),
        recipe_currency: iskworks_core::RecipeCurrency::Current,
        active_sde_version: Some("test".to_string()),
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: true,
    }
}

fn fixture_facility_profile(
    role: iskworks_core::FacilityRole,
) -> iskworks_core::IndustryFacilityProfile {
    let now = chrono::Utc::now();
    iskworks_core::IndustryFacilityProfile {
        id: iskworks_core::FacilityProfileId::new(),
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Test Facility".to_string(),
        kind: iskworks_core::FacilityKind::Manual,
        role,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: None,
        solar_system_name: String::new(),
        security_class: iskworks_core::SecurityClass::Unknown,
        material_reduction_percent: rust_decimal::Decimal::ZERO,
        time_reduction_percent: rust_decimal::Decimal::ZERO,
        job_cost_reduction_percent: rust_decimal::Decimal::ZERO,
        facility_tax_percent: rust_decimal::Decimal::ZERO,
        scc_surcharge_percent: rust_decimal::Decimal::ZERO,
        alliance_surcharge_percent: rust_decimal::Decimal::ZERO,
        fixed_supplemental_cost: iskworks_core::Money::parse("0").unwrap(),
        manual_system_cost_index: Some(rust_decimal::Decimal::new(5, 2)),
        notes: String::new(),
        rigs: Vec::new(),
        archived_at: None,
        revision: 1,
        created_at: now,
        updated_at: now,
    }
}

/// A manufacturing facility profile at a caller-chosen id, `revision`, and
/// whole-percent `material_reduction_percent` -- so a test can prove a
/// live-Build calculation resolved the *current* profile (the observable is
/// the reduction the current profile applied).
fn manufacturing_facility_at(
    id: iskworks_core::FacilityProfileId,
    revision: u64,
    material_reduction_percent: i64,
) -> iskworks_core::IndustryFacilityProfile {
    let mut profile = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    profile.id = id;
    profile.revision = revision;
    profile.material_reduction_percent = rust_decimal::Decimal::from(material_reduction_percent);
    profile
}

/// A bare, walkable `DraftPlanningSnapshot` -- no facility, no
/// manual price list, no component resolutions or fulfillment overrides.
/// `linked_child_build`'s own default `draft_planning: None` is a genuinely
/// unconfigured child, and `project_build_materials` (which Graph now
/// shares with Materials/Worksheet) fails the *whole* projection rather
/// than degrade just that one node when it hits one -- the same contract
/// Materials/Worksheet already had. Attach this to a fixture child that
/// only needs to be *walkable*, not specifically configured, so the test
/// still exercises real dynamic sizing/allocation instead of tripping that
/// all-or-nothing gate incidentally.
fn walkable_draft_planning(
    now: chrono::DateTime<chrono::Utc>,
) -> iskworks_core::DraftPlanningSnapshot {
    iskworks_core::DraftPlanningSnapshot {
        input: iskworks_core::DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
            output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: now,
    }
}

// ---------------------------------------------------------------------------
// Build Graph fixtures (POST /api/builds/:build_id/graph)
// ---------------------------------------------------------------------------

/// The graph endpoint reuses the linked-build fixture harness: a seeded
/// persisted root + any seeded children, `FixtureSdeRepository`, empty
/// inventory, a no-op production repo, and (unless `market_items` is
/// populated) no price source -- so cost comes back `incomplete`, which the
/// topology-focused tests want. `market_items` populated drives cost
/// enrichment.
fn graph_app(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
) -> axum::Router {
    graph_app_with_facilities(
        parent,
        owner_id,
        linked_builds,
        market_items,
        std::collections::HashMap::new(),
    )
}

fn graph_app_with_facilities(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    market_items: std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            price_source: None,
            market_items,
            facility_profiles,
            blueprint_observations: std::collections::HashMap::new(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

fn hull_section_built() -> serde_json::Value {
    serde_json::json!([
        {"typeId": 90_001, "recipe": {"mode": "manufacturing", "blueprintTypeId": 90_002}}
    ])
}

/// A `PreviewBuildPlanCommand` overlay body for root blueprint 6_830. No
/// facility (so the preview prelude never needs ESI).
fn overlay(runs: u64, component_resolutions: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": runs,
        "pricingSelections": [],
        "componentResolutions": component_resolutions,
        "fulfillmentScopes": [],
    })
}

async fn post_graph(
    app: axum::Router,
    build_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/graph", build_id.0))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

// ---------------------------------------------------------------------------
// Build Materials fixtures (POST /api/builds/:id/materials)
// ---------------------------------------------------------------------------

fn captured_recipe(
    blueprint_type_id: i64,
    materials: &[(i64, &str, u64)],
    product: (i64, &str, u64),
) -> iskworks_core::CapturedRecipe {
    iskworks_core::CapturedRecipe {
        source_sde_dataset_id: uuid::Uuid::new_v4(),
        source_sde_version: "test".to_string(),
        blueprint_type_id,
        blueprint_name: format!("bp {blueprint_type_id}"),
        duration_seconds_per_run: Some(300),
        materials: materials
            .iter()
            .enumerate()
            .map(
                |(index, (type_id, name, qpr))| iskworks_core::CapturedRecipeLine {
                    type_id: *type_id,
                    type_name: (*name).to_string(),
                    quantity_per_run: *qpr,
                    sort_order: index as u32,
                },
            )
            .collect(),
        products: vec![iskworks_core::CapturedRecipeLine {
            type_id: product.0,
            type_name: product.1.to_string(),
            quantity_per_run: product.2,
            sort_order: 0,
        }],
        fingerprint: "recipe".to_string(),
    }
}

/// A linked child Build **with** a persisted draft (so its per-node
/// `BuildPlanRevision` reconstructs). `linked_child_build` deliberately has
/// no draft; the Materials projection needs one for every walked node.
fn linked_child_with_draft(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_build_id: BuildId,
    parent_component_type_id: i64,
    runs: u64,
    recipe: iskworks_core::CapturedRecipe,
    component_resolutions: Vec<iskworks_core::ComponentResolution>,
) -> Build {
    let now = chrono::Utc::now();
    fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "linked child".to_string(),
            recipe: iskworks_core::BuildRecipe::Manufacturing(recipe),
            runs,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
                input: iskworks_core::DraftPlanningInput {
                    material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    manual_price_list_id: None,
                    expected_manual_price_list_revision: None,
                    material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                    output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                    pricing_selections: Vec::new(),
                    blueprint_selection: None,
                    manufacturing_facility: None,
                    reaction_facility: None,
                    facility_eiv_manual: false,
                    component_resolutions,
                    fulfillment_scopes: Vec::new(),
                },
                updated_at: now,
            }),
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        parent_build_id,
        parent_component_type_id,
    )
}

fn materials_app(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    balances: Vec<(i64, u64)>,
) -> (
    axum::Router,
    Arc<support::inventory::SeededInventoryRepository>,
) {
    let inventory = Arc::new(support::inventory::SeededInventoryRepository::new(balances));
    let router = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(inventory.clone())
        // Deliberately NO production repository: the Materials path must
        // never touch `ProductionRepository::coverage` (or anything else on
        // that port). `AppState` returns `ProductionUnavailable` if it does,
        // failing the test loudly.
        .with_production_repository(Arc::new(NeverCalledProductionRepository)),
    );
    (router, inventory)
}

fn nested_chain(runs: u64) -> (Build, Vec<Build>, BuildId, OwnerId) {
    let (parent, ws, owner_id) = rifter_root(runs);
    let parent_id = parent.id;
    // 90_001 built from Pyerite; Pyerite itself built from Tritanium (34) --
    // the same type_id the root needs directly.
    let hull_child = linked_child_with_draft(
        ws,
        owner_id,
        parent_id,
        90_001,
        2 * runs,
        captured_recipe(
            90_002,
            &[(35, "Pyerite", 50)],
            (90_001, "Rifter Hull Section", 1),
        ),
        vec![resolution(35, 91_002)],
    );
    let hull_child_id = hull_child.id;
    let pyerite_child = linked_child_with_draft(
        ws,
        owner_id,
        hull_child_id,
        35,
        100 * runs,
        captured_recipe(91_002, &[(34, "Tritanium", 10)], (35, "Pyerite", 1)),
        Vec::new(),
    );
    (parent, vec![hull_child, pyerite_child], parent_id, owner_id)
}

fn node_alloc(
    json: &serde_json::Value,
    build_id: BuildId,
    type_id: i64,
) -> Option<&serde_json::Value> {
    json["nodeAllocations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["buildId"] == build_id.0.to_string() && a["typeId"] == type_id)
}

async fn post_materials(
    app: axum::Router,
    build_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/materials", build_id.0))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

fn resolution(type_id: i64, blueprint_type_id: i64) -> iskworks_core::ComponentResolution {
    iskworks_core::ComponentResolution {
        type_id,
        recipe: iskworks_core::RecipeSelection::Manufacturing { blueprint_type_id },
        facility_override: None,
        blueprint_selection: None,
    }
}

/// The Rifter root Build-resolving Hull Section, at a chosen run count and
/// with a fresh workspace/owner.
fn rifter_root(runs: u64) -> (Build, iskworks_core::WorkspaceId, OwnerId) {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    let mut parent = parent_build_resolving_hull_section_to_build(workspace_id, owner_id);
    parent.runs = runs;
    (parent, workspace_id, owner_id)
}

fn row(json: &serde_json::Value, type_id: i64) -> Option<&serde_json::Value> {
    json["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["typeId"] == type_id)
}

// ---------------------------------------------------------------------------
// Whole-tree allocation assertions
// ---------------------------------------------------------------------------

/// Every row (covered or short): `required == allocated + shortage`,
/// `allocated <= available`, `fullyCovered == (shortage == 0)`, and the row
/// equals the sum of **all** its per-node allocations (every boundary now
/// contributes to both `by_type` and `nodeAllocations` in lockstep). (Matrix W.)
fn assert_materials_invariants(json: &serde_json::Value) {
    for line in json["rows"].as_array().unwrap() {
        let type_id = line["typeId"].clone();
        let req = line["requiredQuantity"].as_u64().unwrap();
        let alloc = line["allocatedQuantity"].as_u64().unwrap();
        let short = line["shortageQuantity"].as_u64().unwrap();
        assert_eq!(
            req,
            alloc + short,
            "row {type_id}: required == allocated + shortage"
        );
        assert_eq!(
            line["fullyCovered"].as_bool().unwrap(),
            short == 0,
            "row {type_id}: fullyCovered == (shortage == 0)"
        );
        assert!(alloc <= line["availableQuantity"].as_u64().unwrap());

        let nodes: Vec<&serde_json::Value> = json["nodeAllocations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["typeId"] == type_id)
            .collect();
        let sum = |k: &str| -> u64 { nodes.iter().map(|n| n[k].as_u64().unwrap()).sum() };
        assert_eq!(
            sum("requiredQuantity"),
            req,
            "Σ node required == row for {type_id}"
        );
        assert_eq!(
            sum("allocatedQuantity"),
            alloc,
            "Σ node allocated == row for {type_id}"
        );
        assert_eq!(
            sum("shortageQuantity"),
            short,
            "Σ node shortage == row for {type_id}"
        );
    }
    for n in json["nodeAllocations"].as_array().unwrap() {
        assert_eq!(
            n["requiredQuantity"].as_u64().unwrap(),
            n["allocatedQuantity"].as_u64().unwrap() + n["shortageQuantity"].as_u64().unwrap(),
            "node alloc required == allocated + shortage"
        );
    }
}

// ---------------------------------------------------------------------------
// Graph / linked-Build worksheet fidelity fixtures
// ---------------------------------------------------------------------------

/// Prices covering the whole Rifter -> Hull Section chain, so both the graph
/// and the worksheet report `pricingComplete`.
fn chain_prices() -> std::collections::BTreeMap<i64, iskworks_core::PriceSourceItem> {
    [
        (34, price_item(34, "Tritanium", "5.0000")),
        (35, price_item(35, "Pyerite", "9.0000")),
        (60_001, price_item(60_001, "Fidelity Isotope", "1000.0000")),
        (90_001, price_item(90_001, "Rifter Hull Section", "60.0000")),
        (5_876, price_item(5_876, "Rifter", "100000.0000")),
    ]
    .into_iter()
    .collect()
}

/// A node's `children` entry for `type_id` (children are ordered by
/// `type_id`, and every BUY requirement is its own child, so a positional
/// index is not stable).
fn child_by_type(node: &serde_json::Value, type_id: i64) -> &serde_json::Value {
    node["children"]
        .as_array()
        .unwrap_or_else(|| panic!("no children array in {node:?}"))
        .iter()
        .find(|c| c["typeId"] == type_id)
        .unwrap_or_else(|| panic!("no child for type {type_id} in {node:?}"))
}

/// A persisted Rifter Hull Section (90_001) linked build with a real draft:
/// `blueprint_selection` (so ME/TE resolve), no facility, Jita scope, and
/// whatever `component_resolutions` it Build-resolves itself.
#[allow(clippy::too_many_arguments)]
fn hull_section_linked_with_draft(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    parent_build_id: BuildId,
    runs: u64,
    blueprint_type_id: i64,
    hull_materials: Vec<iskworks_core::CapturedRecipeLine>,
    blueprint_selection: Option<iskworks_core::BlueprintSelection>,
    component_resolutions: Vec<iskworks_core::ComponentResolution>,
) -> Build {
    let now = chrono::Utc::now();
    fixture_link(
        Build {
            id: BuildId::new(),
            workspace_id,
            owner_id,
            name: "Rifter Hull Section build".to_string(),
            recipe: iskworks_core::BuildRecipe::Manufacturing(iskworks_core::CapturedRecipe {
                source_sde_dataset_id: uuid::Uuid::new_v4(),
                source_sde_version: "test".to_string(),
                blueprint_type_id,
                blueprint_name: "Rifter Hull Section Blueprint".to_string(),
                duration_seconds_per_run: Some(300),
                materials: hull_materials,
                products: vec![iskworks_core::CapturedRecipeLine {
                    type_id: 90_001,
                    type_name: "Rifter Hull Section".to_string(),
                    quantity_per_run: 1,
                    sort_order: 0,
                }],
                fingerprint: "recipe".to_string(),
            }),
            runs,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: Some(iskworks_core::DraftPlanningSnapshot {
                input: iskworks_core::DraftPlanningInput {
                    material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
                    manual_price_list_id: None,
                    expected_manual_price_list_revision: None,
                    material_pricing_policy: iskworks_core::MarketPricingPolicy::HighestBuy,
                    output_pricing_policy: iskworks_core::MarketPricingPolicy::LowestSell,
                    pricing_selections: Vec::new(),
                    blueprint_selection,
                    manufacturing_facility: None,
                    reaction_facility: None,
                    facility_eiv_manual: false,
                    component_resolutions,
                    fulfillment_scopes: Vec::new(),
                },
                updated_at: now,
            }),
            recipe_currency: iskworks_core::RecipeCurrency::Current,
            active_sde_version: Some("test".to_string()),
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        },
        parent_build_id,
        90_001,
    )
}

/// A quantity by `type_id` from a worksheet `materialLines` (`totalQuantity`)
/// row, or -- for the historical `"buyMaterials"` key, now that every BUY
/// requirement is a first-class graph child -- from the graph node's
/// `children` acquisition node (`requiredQuantity`).
fn line_qty(json: &serde_json::Value, key: &str, type_id: i64) -> i64 {
    if key == "buyMaterials" {
        let acq = json["children"]
            .as_array()
            .unwrap_or_else(|| panic!("no children array in {json:?}"))
            .iter()
            .find(|c| c["nodeKind"] == "acquisition" && c["typeId"] == type_id)
            .unwrap_or_else(|| panic!("no acquisition child for type {type_id} in {json:?}"));
        return acq["requiredQuantity"]
            .as_i64()
            .unwrap_or_else(|| panic!("no requiredQuantity on acquisition {type_id}: {acq:?}"));
    }
    let line = json[key]
        .as_array()
        .unwrap_or_else(|| panic!("no array at {key} in {json:?}"))
        .iter()
        .find(|line| line["typeId"] == type_id)
        .unwrap_or_else(|| panic!("no {key} line for type {type_id} in {json:?}"));
    line["requiredQuantity"]
        .as_i64()
        .or_else(|| line["totalQuantity"].as_i64())
        .unwrap_or_else(|| panic!("no quantity on {key} line {type_id}: {line:?}"))
}

fn manual_bpc(me: u8, te: u8) -> iskworks_core::BlueprintSelection {
    iskworks_core::BlueprintSelection::Manual {
        kind: iskworks_core::BlueprintKind::Copy,
        material_efficiency: me,
        time_efficiency: te,
        licensed_runs: Some(100_000),
        notes: String::new(),
    }
}

async fn post_preview(app: axum::Router, body: serde_json::Value) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/preview")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{json:?}");
    json
}

/// A node's first `production` child.
fn prod_child(node: &serde_json::Value) -> &serde_json::Value {
    node["children"]
        .as_array()
        .unwrap_or_else(|| panic!("no children array in {node:?}"))
        .iter()
        .find(|c| c["nodeKind"] == "production")
        .unwrap_or_else(|| panic!("no production child in {node:?}"))
}

fn pyerite_50() -> Vec<iskworks_core::CapturedRecipeLine> {
    vec![iskworks_core::CapturedRecipeLine {
        type_id: 35,
        type_name: "Pyerite".to_string(),
        quantity_per_run: 50,
        sort_order: 0,
    }]
}

fn rifter_parent(workspace_id: iskworks_core::WorkspaceId, owner_id: OwnerId) -> Build {
    parent_build_resolving_hull_section_to_build(workspace_id, owner_id)
}

/// A root Rifter batch whose overlay/persisted state Build-resolves the
/// Hull Section (90_001), at `runs` root runs -> `runs * 2` Hull Sections
/// demanded.
/// One planner: sourcing is planned from *saved* state. A fixture that means
/// "this component is produced by recipe R" saves that resolution on the
/// consumer, rather than relying on an unsaved request overlay to re-match a
/// producer child.
fn with_saved_resolution(
    mut build: Build,
    type_id: i64,
    recipe: iskworks_core::RecipeSelection,
) -> Build {
    let input = &mut build
        .draft_planning
        .as_mut()
        .expect("fixture has a draft")
        .input;
    input.component_resolutions.retain(|r| r.type_id != type_id);
    input
        .component_resolutions
        .push(iskworks_core::ComponentResolution {
            type_id,
            recipe,
            facility_override: None,
            blueprint_selection: None,
        });
    build
}
