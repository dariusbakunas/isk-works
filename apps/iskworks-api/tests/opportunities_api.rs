use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, TimeZone, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    AdjustedPriceRepository, FacilityKind, FacilityProfileId, FacilityRole, IndustryError,
    IndustryFacilityProfile, IndustryRepository, InventoryError, MarketCoverageItem,
    MarketCoverageRegistration, MarketError, MarketOrderBook, MarketOrderSide, MarketOrderView,
    MarketPriceSource, MarketPriceSourceConfig, MarketPricingPolicy, MarketRefreshState,
    MarketRepository, Money, NewWorkspace, PriceSource, PriceSourceId, PriceSourceKind,
    SecurityClass, WorkspaceId, WorkspaceState,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, CandidateProductClassification, CandidateRecipeIdentity,
    ImportCounts, ManufacturableCandidateRecipe, ManufacturableCandidateScope, ManufacturingRecipe,
    RecipeLine, SdeError, SdeReadRepository, TypeSearchResult,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::inventory::SeededInventoryRepository;
use support::workspace::ConfiguredWorkspaceRepository;

fn app() -> axum::Router {
    let workspace = NewWorkspace::manual("Opportunities".to_string());
    build_router(AppState::new(Arc::new(ConfiguredWorkspaceRepository {
        state: WorkspaceState::configured(workspace.workspace, workspace.owner),
    })))
}

#[tokio::test]
async fn scopes_returns_the_server_owned_scope_catalog() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/opportunities/scopes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let scopes = body.as_array().unwrap();
    assert_eq!(
        scopes
            .iter()
            .map(|scope| scope["id"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("t1-frigates"),
            json!("t1-destroyers"),
            json!("t1-cruisers"),
            json!("t1-battlecruisers"),
            json!("t1-battleships"),
            json!("t1-industrial-ships"),
            json!("t1-rigs"),
            json!("reactions"),
        ]
    );
    let reactions = scopes
        .iter()
        .find(|scope| scope["id"] == "reactions")
        .unwrap();
    assert_eq!(reactions["recipeKind"], "reaction");
    assert_eq!(reactions["family"], "Industry");
    let frigates = scopes
        .iter()
        .find(|scope| scope["id"] == "t1-frigates")
        .unwrap();
    assert_eq!(frigates["recipeKind"], "manufacturing");
    assert_eq!(frigates["family"], "Ships");
}

#[tokio::test]
async fn evaluate_is_registered_and_reports_missing_runtime_dependencies() {
    let response = app()
        .oneshot(json_request(json!({
            "scopeId": "t1-frigates",
            "facilityProfileId": "00000000-0000-0000-0000-000000000001",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "marketScope": {
                "regionId": 10000002,
                "locationId": 60003760
            }
        })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "persistence_unavailable"
    );
}

#[tokio::test]
async fn evaluate_rejects_unknown_scope_json() {
    let response = app()
        .oneshot(json_request(json!({
            "scopeId": "t2-frigates",
            "facilityProfileId": "00000000-0000-0000-0000-000000000001",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "marketScope": {
                "regionId": 10000002,
                "locationId": 60003760
            }
        })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn refresh_is_registered_and_returns_before_runtime_evidence_is_available() {
    let response = app()
        .oneshot(json_request_to(
            "/api/opportunities/refresh",
            json!({
                "scopeId": "t1-frigates",
                "facilityProfileId": "00000000-0000-0000-0000-000000000001",
                "materialEfficiency": 10,
                "timeEfficiency": 20,
                "marketScope": {
                    "regionId": 10000002,
                    "locationId": 60003760
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "persistence_unavailable"
    );
}

struct FixtureIndustryRepository {
    price_source: PriceSource,
    facility: IndustryFacilityProfile,
}

#[async_trait]
impl IndustryRepository for FixtureIndustryRepository {
    async fn list_builds(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::Build>, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn create_build(
        &self,
        _: iskworks_core::NewBuild,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn update_draft(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: iskworks_core::DraftUpdate,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn rename_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: String,
    ) -> Result<iskworks_core::Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn delete_build(
        &self,
        _: WorkspaceId,
        _: iskworks_core::BuildId,
        _: u64,
        _force: bool,
    ) -> Result<(), IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn list_price_sources(&self, _: WorkspaceId) -> Result<Vec<PriceSource>, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        assert_eq!(workspace_id, self.price_source.workspace_id);
        assert_eq!(source_id, self.price_source.id);
        Ok(self.price_source.clone())
    }
    async fn create_price_source(&self, _: PriceSource) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn update_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: iskworks_core::UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn upsert_price_items(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: u64,
        _: Vec<iskworks_core::PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn remove_price_item(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: i64,
        _: u64,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn delete_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: u64,
    ) -> Result<(), IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_facility_profile(
        &self,
        _: WorkspaceId,
        _: FacilityProfileId,
    ) -> Result<IndustryFacilityProfile, IndustryError> {
        Ok(self.facility.clone())
    }
}

struct FixtureMarketRepository {
    market_source: MarketPriceSource,
    books: BTreeMap<i64, MarketOrderBook>,
}

#[async_trait]
impl MarketRepository for FixtureMarketRepository {
    async fn resolve_type_name(&self, _: i64) -> Result<Option<String>, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<PriceSourceId, MarketError> {
        assert_eq!(workspace_id, self.market_source.workspace_id);
        assert_eq!(scope.region_id, self.market_source.config.region_id);
        assert_eq!(
            scope.location_id,
            Some(self.market_source.config.location_id)
        );
        Ok(self.market_source.id)
    }
    async fn location_names(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn imported_file_checksums(
        &self,
        _: WorkspaceId,
        _: &[String],
    ) -> Result<std::collections::BTreeSet<String>, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn commit_import(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn list_imports(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::MarketImportBatch>, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: iskworks_core::MarketImportBatchId,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        assert_eq!(source_id, self.market_source.id);
        Ok(type_ids
            .iter()
            .filter_map(|type_id| self.books.get(type_id).map(|book| (*type_id, book.clone())))
            .collect())
    }
    async fn get_market_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        assert_eq!(workspace_id, self.market_source.workspace_id);
        assert_eq!(source_id, self.market_source.id);
        Ok(self.market_source.clone())
    }
    async fn register_market_coverage(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<MarketCoverageRegistration>,
    ) -> Result<Vec<MarketCoverageItem>, MarketError> {
        assert_eq!(source_id, self.market_source.id);
        Ok(items
            .into_iter()
            .map(|item| MarketCoverageItem {
                type_id: item.type_id,
                type_name: item.type_name,
                refresh_state: MarketRefreshState::Current,
                observed_at: None,
                last_attempted_at: None,
                next_refresh_at: None,
                last_error: None,
                prior_etag: None,
                revalidated_at: None,
                order_count: 0,
                buy_order_count: 0,
                sell_order_count: 0,
            })
            .collect())
    }
}

struct FixtureAdjustedPriceRepository {
    adjusted_prices: BTreeMap<i64, Decimal>,
}

#[async_trait]
impl AdjustedPriceRepository for FixtureAdjustedPriceRepository {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _: DateTime<Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, InventoryError> {
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                self.adjusted_prices
                    .get(type_id)
                    .map(|price| (*type_id, *price))
            })
            .collect())
    }
}

struct FixtureSdeRepository {
    active_sde: ActiveSde,
    candidates: Vec<ManufacturableCandidateRecipe>,
}

#[async_trait]
impl SdeReadRepository for FixtureSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(self.active_sde.clone()))
    }
    async fn manufacturable_candidates(
        &self,
        _: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        Ok(self.candidates.clone())
    }
    async fn search_manufacturing_blueprints(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn search_types(&self, _: &str, _: u32) -> Result<Vec<TypeSearchResult>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
}

fn order_book(
    type_id: i64,
    type_name: &str,
    sell_levels: &[(&str, u64)],
    buy_levels: &[(&str, u64)],
    now: DateTime<Utc>,
) -> MarketOrderBook {
    let side_orders = |side: MarketOrderSide, levels: &[(&str, u64)], offset: usize| {
        levels
            .iter()
            .enumerate()
            .map(|(index, (price, volume))| MarketOrderView {
                observation_id: None,
                import_batch_id: None,
                imported_file_id: None,
                order_id: i64::try_from(offset + index + 1).unwrap(),
                type_id,
                type_name: type_name.to_string(),
                side,
                price: Money::parse(price).unwrap(),
                remaining_volume: *volume,
                entered_volume: *volume,
                minimum_volume: 1,
                order_range: -1,
                issued_at: now,
                duration_days: 90,
                observed_at: now,
                revalidated_at: None,
                location_id: 60_003_760,
                solar_system_id: 30_000_142,
                region_id: 10_000_002,
                jumps: 0,
            })
            .collect::<Vec<_>>()
    };
    let mut orders = side_orders(MarketOrderSide::Sell, sell_levels, 0);
    orders.extend(side_orders(
        MarketOrderSide::Buy,
        buy_levels,
        sell_levels.len(),
    ));
    MarketOrderBook {
        type_id,
        type_name: type_name.to_string(),
        location_id: 60_003_760,
        location_name: "Jita 4-4".to_string(),
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        observed_at: now,
        revalidated_at: None,
        observation_batch_id: iskworks_core::MarketObservationBatchId(Uuid::from_u128(
            u128::try_from(type_id).unwrap(),
        )),
        import_batch_id: None,
        imported_file_id: None,
        buy_order_count: u64::try_from(buy_levels.len()).unwrap(),
        sell_order_count: u64::try_from(sell_levels.len()).unwrap(),
        total_buy_volume: buy_levels.iter().map(|(_, volume)| volume).sum(),
        total_sell_volume: sell_levels.iter().map(|(_, volume)| volume).sum(),
        lowest_sell: sell_levels
            .first()
            .map(|(price, _)| Money::parse(price).unwrap()),
        highest_buy: buy_levels
            .first()
            .map(|(price, _)| Money::parse(price).unwrap()),
        orders,
    }
}

fn successful_evaluation_app() -> axum::Router {
    build_router(successful_evaluation_state())
}

/// Same fixture as [`successful_evaluation_app`], but returned as an
/// unwrapped `AppState` so callers can layer an `InventoryRepository` onto
/// it -- see `evaluate_stays_inventory_neutral_regardless_of_inventory_state`.
/// `OpportunityQueryService` (`crates/iskworks-core/src/opportunity/query_service.rs`)
/// has no `InventoryRepository` field at all, so this state deliberately
/// carries none by default: Opportunities' economic cost/profit is computed
/// purely from market/adjusted-price evidence, never from inventory balances
/// or historical basis.
fn successful_evaluation_state() -> AppState {
    let workspace = NewWorkspace::manual("Opportunities".to_string());
    let workspace_id = workspace.workspace.id;
    let owner_id = workspace.owner.id;
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();

    let facility_profile_id = FacilityProfileId(Uuid::from_u128(101));
    let price_source_id = PriceSourceId(Uuid::from_u128(102));

    let facility = IndustryFacilityProfile {
        id: facility_profile_id,
        workspace_id,
        name: "Test Assembly Array".to_string(),
        kind: FacilityKind::Manual,
        role: FacilityRole::Manufacturing,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: None,
        solar_system_name: String::new(),
        security_class: SecurityClass::Unknown,
        material_reduction_percent: Decimal::ZERO,
        time_reduction_percent: Decimal::ZERO,
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ZERO,
        scc_surcharge_percent: Decimal::ZERO,
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::zero(),
        manual_system_cost_index: Some(Decimal::new(5, 2)),
        notes: String::new(),
        rigs: Vec::new(),
        archived_at: None,
        revision: 1,
        created_at: now,
        updated_at: now,
    };

    let price_source = PriceSource {
        id: price_source_id,
        workspace_id,
        name: "Jita".to_string(),
        description: String::new(),
        kind: PriceSourceKind::EsiMarketOrders,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: now,
        updated_at: now,
    };

    let market_source = MarketPriceSource {
        id: price_source_id,
        workspace_id,
        name: "Jita".to_string(),
        description: String::new(),
        kind: PriceSourceKind::EsiMarketOrders,
        revision: 1,
        item_count: 0,
        config: MarketPriceSourceConfig {
            price_source_id,
            workspace_id,
            location_id: 60_003_760,
            solar_system_id: 30_000_142,
            region_id: 10_000_002,
            location_alias: "Jita 4-4".to_string(),
            pricing_policy: MarketPricingPolicy::LowestSell,
            coverage_policy: iskworks_core::MarketCoveragePolicy::AllowPartialWithWarning,
            observation_mode: iskworks_core::MarketObservationSetMode::LatestCompatibleImport,
            pinned_batch_id: None,
            fresh_after_hours: 6,
            stale_after_hours: 24,
            archived_at: None,
            last_snapshot_at: Some(now),
        },
        created_at: now,
        updated_at: now,
    };

    let rifter = ManufacturableCandidateRecipe {
        import_id: Uuid::from_u128(9),
        source_version: "2026.08".to_string(),
        identity: CandidateRecipeIdentity::Manufacturing {
            blueprint_type_id: 68_357,
        },
        recipe_name: "Rifter Blueprint".to_string(),
        duration_seconds: Some(6_000),
        materials: vec![RecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity: 2_111,
        }],
        products: vec![RecipeLine {
            type_id: 5_876,
            type_name: "Rifter".to_string(),
            quantity: 1,
        }],
        primary_product_type_id: 5_876,
        primary_product_published: true,
        recipe_type_published: true,
        classification: CandidateProductClassification {
            category_id: Some(6),
            category_name: Some("Ship".to_string()),
            group_id: Some(25),
            group_name: Some("Frigate".to_string()),
            meta_group_id: Some(1),
            meta_group_name: Some("Tech I".to_string()),
            market_group_id: None,
            market_group_name: None,
            market_group_ancestry: Vec::new(),
        },
        has_additional_products: false,
    };

    let mut books = BTreeMap::new();
    books.insert(
        34,
        order_book(34, "Tritanium", &[("5", 1_000_000)], &[], now),
    );
    books.insert(
        5_876,
        // Deliberately thin: exactly the output quantity (1) visible on
        // each side, so the candidate's valuations stay fully computable
        // while the thin-output-book warning still fires (weak quality).
        order_book(5_876, "Rifter", &[("1000000", 1)], &[("900000", 1)], now),
    );

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(500, 2));

    let state = AppState::new(Arc::new(ConfiguredWorkspaceRepository {
        state: WorkspaceState::configured(workspace.workspace, workspace.owner),
    }))
    .with_sde_repository(Arc::new(FixtureSdeRepository {
        active_sde: ActiveSde {
            import_id: Uuid::from_u128(9),
            source_version: "2026.08".to_string(),
            source_label: "test".to_string(),
            source_checksum: "checksum".to_string(),
            completed_at: now,
            counts: ImportCounts {
                types: 1,
                categories: 1,
                groups: 1,
                meta_groups: 1,
                market_groups: 1,
                classified_types: 1,
                blueprints: 1,
                material_lines: 1,
                product_lines: 1,
                skipped_blueprints: 0,
                reaction_formulas: 0,
                reaction_material_lines: 0,
                reaction_product_lines: 0,
                skipped_reaction_formulas: 0,
            },
        },
        candidates: vec![rifter],
    }))
    .with_industry_repository(Arc::new(FixtureIndustryRepository {
        price_source,
        facility,
    }))
    .with_market_repository(Arc::new(FixtureMarketRepository {
        market_source,
        books,
    }))
    .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository { adjusted_prices }));

    let _ = owner_id;
    state
}

#[tokio::test]
async fn evaluate_returns_camel_case_result_quality_projection() {
    let response = successful_evaluation_app()
        .oneshot(json_request(json!({
            "scopeId": "t1-frigates",
            "facilityProfileId": "00000000-0000-0000-0000-000000000065",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "marketScope": {
                "regionId": 10000002,
                "locationId": 60003760
            }
        })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let candidate = &body["candidates"][0];

    assert_eq!(candidate["productTypeId"], 5_876);
    assert_eq!(candidate["eligibility"]["status"], "eligibleWithWarnings");
    assert!(candidate["eligibility"]["exclusionReasons"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(candidate["quality"]["evidenceQuality"], "weak");
    assert!(candidate["valuations"]["sellSide"]["grossProfit"]
        .as_str()
        .is_some());
    assert!(
        candidate["valuations"]["immediateLiquidation"]["grossProfit"]
            .as_str()
            .is_some()
    );
    assert!(candidate["metrics"]["estimatedGrossProfit"].is_string());
    assert_eq!(candidate["eivBasis"]["complete"], true);
    assert!(candidate["eivBasis"]["missingMaterials"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(candidate["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "thinOutputBook"));
    assert_eq!(
        candidate["outputMarketEvidence"]["bestSellLevelQuantity"],
        1
    );
    assert!(body["rankings"]["sellSideGrossProfit"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["productTypeId"] == 5_876));
    assert_eq!(body["excludedCosts"].as_array().unwrap().len(), 6);
}

/// **Inventory-neutrality golden guard**: Opportunities' economic material cost / gross
/// profit must stay byte-for-byte identical whether or not inventory exists
/// for the candidate's materials, and regardless of that inventory's
/// historical basis. `OpportunityQueryService` has no `InventoryRepository`
/// dependency at all (see `successful_evaluation_state`'s doc comment) --
/// this proves that invariant end-to-end through the HTTP route, so a future
/// change that wires inventory into the Opportunities path (accidentally or
/// otherwise) fails loudly here rather than silently shifting Opportunities'
/// numbers the way inventory intentionally shifts Build planning cost.
#[tokio::test]
async fn evaluate_stays_inventory_neutral_regardless_of_inventory_state() {
    let baseline_response = build_router(successful_evaluation_state())
        .oneshot(json_request(json!({
            "scopeId": "t1-frigates",
            "facilityProfileId": "00000000-0000-0000-0000-000000000065",
            "materialEfficiency": 10,
            "timeEfficiency": 20,
            "marketScope": {
                "regionId": 10000002,
                "locationId": 60003760
            }
        })))
        .await
        .unwrap();
    assert_eq!(baseline_response.status(), StatusCode::OK);
    let baseline = response_json(baseline_response).await;

    // Deliberately absurd: if this basis ever leaked into Opportunities'
    // material cost, the assertion below would catch it immediately (the
    // fixture's own market price for Tritanium is 5 ISK/unit -- 9_999 ISK/
    // unit inventory would swing estimatedMaterialCost by orders of
    // magnitude). Two variants -- present-with-basis and present-with-none
    // -- both must leave every number untouched.
    for inventory in [
        Arc::new(SeededInventoryRepository::new(vec![(34, 5_000_000)]).with_unit_basis(34, "9999"))
            as Arc<dyn iskworks_core::InventoryRepository>,
        Arc::new(SeededInventoryRepository::new(vec![(34, 5_000_000)])),
    ] {
        let response =
            build_router(successful_evaluation_state().with_inventory_repository(inventory))
                .oneshot(json_request(json!({
                    "scopeId": "t1-frigates",
                    "facilityProfileId": "00000000-0000-0000-0000-000000000065",
                    "materialEfficiency": 10,
                    "timeEfficiency": 20,
                    "marketScope": {
                        "regionId": 10000002,
                        "locationId": 60003760
                    }
                })))
                .await
                .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let with_inventory = response_json(response).await;

        let baseline_candidate = &baseline["candidates"][0];
        let candidate = &with_inventory["candidates"][0];
        assert_eq!(
            candidate["metrics"]["materialCost"],
            baseline_candidate["metrics"]["materialCost"]
        );
        assert_eq!(
            candidate["metrics"]["estimatedGrossProfit"],
            baseline_candidate["metrics"]["estimatedGrossProfit"]
        );
        assert_eq!(
            candidate["valuations"]["sellSide"]["grossProfit"],
            baseline_candidate["valuations"]["sellSide"]["grossProfit"]
        );
        assert_eq!(
            candidate["valuations"]["immediateLiquidation"]["grossProfit"],
            baseline_candidate["valuations"]["immediateLiquidation"]["grossProfit"]
        );
        assert_eq!(candidate["eivBasis"], baseline_candidate["eivBasis"]);
        assert_eq!(
            candidate["completeness"],
            baseline_candidate["completeness"]
        );
        // Not a full-document `assert_eq!`: `calculatedAt` /
        // `elapsedMilliseconds` / `readiness.localReadAt` /
        // `readiness.registeredAt` are wall-clock, not economic, so they
        // legitimately differ between the two calls.
    }
}

fn json_request(body: Value) -> Request<Body> {
    json_request_to("/api/opportunities/evaluate", body)
}

fn json_request_to(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
