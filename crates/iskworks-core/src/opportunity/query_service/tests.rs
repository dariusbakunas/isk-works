use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use iskworks_sde::{
    CandidateRecipeIdentity, ManufacturableCandidateRecipe, ManufacturableCandidateScope,
    ManufacturingRecipe, SdeError,
};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::super::tests_common::*;
use super::*;
use crate::opportunity::*;
use crate::{
    AdjustedPriceRepository, Build, BuildId, FacilityProfileId, IndustryError, IndustryRepository,
    InventoryError, Money, PriceSource, PriceSourceId, PriceSourceItem,
};

struct FixtureIndustryRepository {
    price_source: PriceSource,
    facility: crate::IndustryFacilityProfile,
    get_price_source_calls: std::sync::atomic::AtomicU64,
    get_facility_profile_calls: std::sync::atomic::AtomicU64,
}

#[async_trait::async_trait]
impl IndustryRepository for FixtureIndustryRepository {
    async fn list_builds(&self, _: WorkspaceId) -> Result<Vec<Build>, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_build(&self, _: WorkspaceId, _: BuildId) -> Result<Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn create_build(&self, _: crate::NewBuild) -> Result<Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn update_draft(
        &self,
        _: WorkspaceId,
        _: BuildId,
        _: crate::DraftUpdate,
    ) -> Result<Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn rename_build(
        &self,
        _: WorkspaceId,
        _: BuildId,
        _: String,
    ) -> Result<Build, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn delete_build(
        &self,
        _: WorkspaceId,
        _: BuildId,
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
        self.get_price_source_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
        _: crate::UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn upsert_price_items(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: u64,
        _: Vec<PriceSourceItem>,
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
    ) -> Result<crate::IndustryFacilityProfile, IndustryError> {
        self.get_facility_profile_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.facility.clone())
    }
}

struct FixtureMarketRepository {
    market_source: crate::MarketPriceSource,
    books: BTreeMap<i64, crate::MarketOrderBook>,
    ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64,
    get_market_price_source_calls: std::sync::atomic::AtomicU64,
    get_source_order_books_calls: std::sync::atomic::AtomicU64,
    register_market_coverage_calls: std::sync::atomic::AtomicU64,
    // `(region_id, type_ids, prioritize)` per app-wide demand call.
    public_demand: std::sync::Mutex<Vec<(i64, Vec<i64>, bool)>>,
}

#[async_trait::async_trait]
impl crate::MarketRepository for FixtureMarketRepository {
    async fn resolve_type_name(&self, _: i64) -> Result<Option<String>, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn ensure_esi_price_source_for_scope(
        &self,
        workspace_id: WorkspaceId,
        scope: crate::MarketScope,
    ) -> Result<PriceSourceId, crate::MarketError> {
        self.ensure_esi_price_source_for_scope_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
    ) -> Result<BTreeMap<i64, String>, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<crate::ResolvedMarketLocation>,
    ) -> Result<(), crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn imported_file_checksums(
        &self,
        _: WorkspaceId,
        _: &[String],
    ) -> Result<std::collections::BTreeSet<String>, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn commit_import(
        &self,
        _: WorkspaceId,
        _: Vec<crate::ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<crate::MarketImportBatch, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn list_imports(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<crate::MarketImportBatch>, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: crate::MarketImportBatchId,
    ) -> Result<crate::MarketImportBatch, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<crate::MarketImportBatchId>,
    ) -> Result<crate::MarketOrderBook, crate::MarketError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        type_ids: &[i64],
        _: i64,
        _: Option<crate::MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, crate::MarketOrderBook>, crate::MarketError> {
        self.get_source_order_books_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
    ) -> Result<crate::MarketPriceSource, crate::MarketError> {
        self.get_market_price_source_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(workspace_id, self.market_source.workspace_id);
        assert_eq!(source_id, self.market_source.id);
        Ok(self.market_source.clone())
    }
    async fn classify_location(
        &self,
        _: WorkspaceId,
        _: i64,
    ) -> Result<crate::MarketLocationClassification, crate::MarketError> {
        Ok(crate::MarketLocationClassification::NpcStation)
    }
    async fn register_public_market_demand(
        &self,
        region_id: i64,
        items: Vec<crate::MarketCoverageRegistration>,
        prioritize: bool,
        _: DateTime<Utc>,
    ) -> Result<(), crate::MarketError> {
        let mut type_ids: Vec<i64> = items.iter().map(|item| item.type_id).collect();
        type_ids.sort_unstable();
        self.public_demand
            .lock()
            .unwrap()
            .push((region_id, type_ids, prioritize));
        Ok(())
    }
    async fn register_market_coverage(
        &self,
        _: WorkspaceId,
        source_id: PriceSourceId,
        items: Vec<crate::MarketCoverageRegistration>,
    ) -> Result<Vec<crate::MarketCoverageItem>, crate::MarketError> {
        self.register_market_coverage_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(source_id, self.market_source.id);
        Ok(items
            .into_iter()
            .map(|item| crate::MarketCoverageItem {
                type_id: item.type_id,
                type_name: item.type_name,
                refresh_state: crate::MarketRefreshState::Current,
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
    latest_adjusted_prices_calls: std::sync::atomic::AtomicU64,
}

#[async_trait::async_trait]
impl AdjustedPriceRepository for FixtureAdjustedPriceRepository {
    async fn latest_adjusted_prices(
        &self,
        type_ids: &[i64],
        _: DateTime<Utc>,
    ) -> Result<BTreeMap<i64, Decimal>, InventoryError> {
        self.latest_adjusted_prices_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
    active_sde: iskworks_sde::ActiveSde,
    candidates: Vec<ManufacturableCandidateRecipe>,
    manufacturable_candidates_calls: std::sync::atomic::AtomicU64,
}

#[async_trait::async_trait]
impl SdeReadRepository for FixtureSdeRepository {
    async fn active_sde(&self) -> Result<Option<iskworks_sde::ActiveSde>, SdeError> {
        Ok(Some(self.active_sde.clone()))
    }
    async fn manufacturable_candidates(
        &self,
        _: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        self.manufacturable_candidates_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.candidates.clone())
    }
    async fn search_manufacturing_blueprints(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<iskworks_sde::BlueprintSearchResult>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
    async fn search_types(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, SdeError> {
        unimplemented!("not used by opportunity evaluation")
    }
}

fn fixture_facility(now: DateTime<Utc>) -> crate::IndustryFacilityProfile {
    crate::IndustryFacilityProfile {
        id: FacilityProfileId(Uuid::from_u128(1)),
        workspace_id: WorkspaceId(Uuid::from_u128(3)),
        name: "Test Assembly Array".to_string(),
        kind: crate::FacilityKind::Manual,
        role: crate::FacilityRole::Manufacturing,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: None,
        solar_system_name: String::new(),
        security_class: crate::SecurityClass::Unknown,
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
        revision: 3,
        created_at: now,
        updated_at: now,
    }
}

fn fixture_price_source(now: DateTime<Utc>) -> PriceSource {
    PriceSource {
        id: PriceSourceId(Uuid::from_u128(2)),
        workspace_id: WorkspaceId(Uuid::from_u128(3)),
        name: "Jita".to_string(),
        description: String::new(),
        kind: crate::PriceSourceKind::EsiMarketOrders,
        revision: 4,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

fn output_book(
    type_id: i64,
    type_name: &str,
    now: chrono::DateTime<Utc>,
) -> crate::MarketOrderBook {
    two_sided_book(
        type_id,
        type_name,
        &[("1000000", 100)],
        &[("900000", 100)],
        now,
    )
}

fn fixture_active_sde(now: DateTime<Utc>) -> iskworks_sde::ActiveSde {
    iskworks_sde::ActiveSde {
        import_id: Uuid::from_u128(9),
        source_version: "2026.08".to_string(),
        source_label: "test".to_string(),
        source_checksum: "checksum".to_string(),
        completed_at: now,
        counts: iskworks_sde::ImportCounts {
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
    }
}

fn fixture_reaction_facility(now: DateTime<Utc>) -> crate::IndustryFacilityProfile {
    let mut facility = fixture_facility(now);
    facility.id = FacilityProfileId(Uuid::from_u128(11));
    facility.name = "Test Refinery".to_string();
    facility.role = crate::FacilityRole::Reaction;
    facility
}

fn reaction_command() -> EvaluateOpportunitiesCommand {
    EvaluateOpportunitiesCommand {
        scope_id: ProfitabilityScopeId::Reactions,
        facility_profile_id: FacilityProfileId(Uuid::from_u128(11)),
        material_efficiency: None,
        time_efficiency: None,
        market_scope: jita_scope(),
    }
}

fn recipe_fixture(
    product_type_id: i64,
    product_name: &str,
    blueprint_type_id: i64,
    market_group_ancestry: Vec<(i64, &str)>,
) -> ManufacturableCandidateRecipe {
    let mut recipe = candidate_recipe(CandidateRecipeIdentity::Manufacturing { blueprint_type_id });
    recipe.recipe_name = format!("{product_name} Blueprint");
    recipe.primary_product_type_id = product_type_id;
    recipe.products = vec![iskworks_sde::RecipeLine {
        type_id: product_type_id,
        type_name: product_name.to_string(),
        quantity: 1,
    }];
    recipe.classification.market_group_ancestry = market_group_ancestry
        .into_iter()
        .map(|(id, name)| iskworks_sde::CandidateMarketGroup {
            market_group_id: id,
            name: name.to_string(),
        })
        .collect();
    recipe
}

#[tokio::test]
async fn insufficient_output_depth_is_always_explained_by_a_warning() {
    // The output book has only 1 unit visible while the candidate needs
    // 2. The LowestSell-based depth check (driving
    // `warnings`/`missing_price_type_ids`) only ever asks for 1 unit and
    // so considers this "fully covered"; the depth-aware
    // AcquireQuantityFromSellOrders check driving `completeness` and
    // `valuations.sell_side` correctly does not. A candidate must never
    // be reported Incomplete without a warning that explains why.
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let mut rifter = recipe_fixture(5_876, "Rifter", 68_357, Vec::new());
    rifter.products[0].quantity = 2;

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(5, 0));

    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(
        5_876,
        two_sided_book(5_876, "Rifter", &[("1000000", 1)], &[("900000", 1)], now),
    );

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![rifter],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );
    let evaluation = service
        .evaluate(workspace_id, owner_id, valid_command(), now)
        .await
        .unwrap();

    let candidate = &evaluation.candidates[0];
    assert_eq!(candidate.completeness, OpportunityCompleteness::Incomplete);
    assert_eq!(candidate.valuations.sell_side.revenue, None);
    assert!(
    candidate.missing_price_type_ids.contains(&5_876),
    "an incomplete candidate must list the output type as missing/insufficient priced, got {:?}",
    candidate.missing_price_type_ids
);
    assert!(
    candidate.warnings.iter().any(|warning| warning.kind
        == OpportunityWarningKind::InsufficientMarketDepth
        && warning.type_ids == vec![5_876]),
    "an incomplete candidate must carry a warning explaining the insufficient output depth, got {:?}",
    candidate.warnings
);
}

#[tokio::test]
async fn evaluating_registers_app_wide_demand_for_a_public_scope() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(5_876, output_book(5_876, "Rifter", now));
    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(5, 0));
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let service = OpportunityQueryService::new(
        std::sync::Arc::new(FixtureIndustryRepository {
            price_source: fixture_price_source(now),
            facility: fixture_facility(now),
            get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
            get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
        }),
        std::sync::Arc::new(FixtureSdeRepository {
            active_sde: fixture_active_sde(now),
            candidates: vec![candidate_recipe(CandidateRecipeIdentity::Manufacturing {
                blueprint_type_id: 68_357,
            })],
            manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
        }),
        market_repository.clone(),
        std::sync::Arc::new(FixtureAdjustedPriceRepository {
            adjusted_prices,
            latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
        }),
    );

    let evaluation = service
        .evaluate(
            WorkspaceId(Uuid::from_u128(3)),
            OwnerId(Uuid::from_u128(4)),
            valid_command(),
            now,
        )
        .await
        .unwrap();

    let mut expected = evaluation.required_market_type_ids.clone();
    expected.sort_unstable();
    assert_eq!(
        *market_repository.public_demand.lock().unwrap(),
        vec![(jita_scope().region_id, expected, false)],
        "evaluation registers demand without jumping the queue"
    );
    assert_eq!(
        market_repository
            .register_market_coverage_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a public scope keeps no per-workspace coverage"
    );
}

#[tokio::test]
async fn service_evaluation_batches_repository_calls_and_projects_result_quality() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let rifter = candidate_recipe(CandidateRecipeIdentity::Manufacturing {
        blueprint_type_id: 68_357,
    });
    let mut merlin = candidate_recipe(CandidateRecipeIdentity::Manufacturing {
        blueprint_type_id: 68_358,
    });
    merlin.recipe_name = "Merlin Blueprint".to_string();
    merlin.primary_product_type_id = 5_923;
    merlin.products = vec![iskworks_sde::RecipeLine {
        type_id: 5_923,
        type_name: "Merlin".to_string(),
        quantity: 1,
    }];

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(5, 0));

    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(5_876, output_book(5_876, "Rifter", now));
    // Merlin's output book is intentionally absent.

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![rifter, merlin],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository.clone(),
        sde_repository.clone(),
        market_repository.clone(),
        adjusted_price_repository.clone(),
    );

    let evaluation = service
        .evaluate(workspace_id, owner_id, valid_command(), now)
        .await
        .unwrap();

    // Batching guarantee: exactly one call per repository operation,
    // never once per candidate.
    assert_eq!(
        sde_repository
            .manufacturable_candidates_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        market_repository
            .get_source_order_books_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        adjusted_price_repository
            .latest_adjusted_prices_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        industry_repository
            .get_facility_profile_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    // No `get_price_source`/PriceSource-ownership validation --
    // `MarketScope` has no ownership concept to check.
    assert_eq!(
        industry_repository
            .get_price_source_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(
        market_repository
            .ensure_esi_price_source_for_scope_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        market_repository
            .get_market_price_source_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    // Jita is public: demand goes app-wide, once, and no per-workspace
    // coverage is registered.
    assert_eq!(
        market_repository
            .register_market_coverage_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(market_repository.public_demand.lock().unwrap().len(), 1);

    assert_eq!(evaluation.candidate_count, 2);
    assert_eq!(evaluation.complete_count, 1);
    assert_eq!(evaluation.incomplete_count, 1);
    assert_eq!(evaluation.default_ranking_eligible_count, 2);
    assert_eq!(evaluation.excluded_count, 0);

    let rifter_candidate = evaluation
        .candidates
        .iter()
        .find(|candidate| candidate.product_type_id == 5_876)
        .unwrap();
    assert_eq!(
        rifter_candidate.completeness,
        OpportunityCompleteness::Complete
    );
    assert_eq!(
        rifter_candidate.eligibility.status,
        OpportunityEligibilityStatus::Eligible
    );
    assert!(rifter_candidate.valuations.sell_side.revenue.is_some());
    assert!(rifter_candidate
        .valuations
        .immediate_liquidation
        .revenue
        .is_some());
    assert!(rifter_candidate.output_market_evidence.is_some());

    let merlin_candidate = evaluation
        .candidates
        .iter()
        .find(|candidate| candidate.product_type_id == 5_923)
        .unwrap();
    assert_eq!(
        merlin_candidate.completeness,
        OpportunityCompleteness::Incomplete
    );
    assert!(merlin_candidate
        .warnings
        .iter()
        .any(|warning| warning.kind == OpportunityWarningKind::MissingOutputPrice));
    assert_eq!(merlin_candidate.output_market_evidence, None);

    assert!(evaluation
        .rankings
        .sell_side_gross_profit_per_manufacturing_hour
        .iter()
        .any(|entry| entry.product_type_id == 5_876));
    assert_eq!(evaluation.excluded_costs.len(), 6);
}

#[tokio::test]
async fn service_evaluation_projects_reaction_candidates_against_the_reaction_facility() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let fernite_alloy = reaction_candidate_recipe(CandidateRecipeIdentity::Reaction {
        reaction_formula_type_id: 46_171,
    });

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(16_633, Decimal::new(500, 0));

    let mut books = BTreeMap::new();
    books.insert(
        16_633,
        sell_book(16_633, "Titanium Chromide", &[("500", 1_000_000)], now),
    );
    books.insert(16_656, output_book(16_656, "Fernite Alloy", now));

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_reaction_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![fernite_alloy],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );

    let evaluation = service
        .evaluate(workspace_id, owner_id, reaction_command(), now)
        .await
        .unwrap();

    assert_eq!(evaluation.context.material_efficiency, None);
    assert_eq!(evaluation.context.time_efficiency, None);
    assert_eq!(evaluation.candidate_count, 1);

    let candidate = &evaluation.candidates[0];
    assert_eq!(candidate.product_type_id, 16_656);
    assert_eq!(candidate.material_efficiency, None);
    assert_eq!(candidate.time_efficiency, None);
    assert_eq!(
        candidate.facility_profile_id,
        FacilityProfileId(Uuid::from_u128(11))
    );
    assert!(matches!(
        candidate.recipe,
        OpportunityRecipeIdentity::Reaction {
            reaction_formula_type_id: 46_171,
            ..
        }
    ));
    // Reaction runs use facility rig time reduction only (no
    // blueprint TE) -- the fixture facility has no rigs, so the
    // effective duration equals the formula's base duration exactly.
    assert_eq!(candidate.base_duration_seconds, Some(3_600));
    assert_eq!(candidate.effective_duration_seconds, Some(3_600));
    assert_eq!(candidate.completeness, OpportunityCompleteness::Complete);
    assert_eq!(
        candidate.eligibility.status,
        OpportunityEligibilityStatus::Eligible
    );
    assert!(candidate.metrics.material_cost.is_some());
    assert!(candidate.metrics.installation_cost.is_some());
    assert!(candidate.valuations.sell_side.revenue.is_some());
    assert!(candidate.valuations.immediate_liquidation.revenue.is_some());
    assert!(candidate.output_market_evidence.is_some());
    assert_eq!(
        candidate.quality.evidence_quality,
        crate::OpportunityEvidenceQuality::Strong
    );
}

#[tokio::test]
async fn reaction_candidates_rank_by_the_same_profit_per_hour_metric_as_manufacturing() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let fernite_alloy = reaction_candidate_recipe(CandidateRecipeIdentity::Reaction {
        reaction_formula_type_id: 46_171,
    });
    let mut sulfuric_acid = reaction_candidate_recipe(CandidateRecipeIdentity::Reaction {
        reaction_formula_type_id: 46_181,
    });
    sulfuric_acid.recipe_name = "Sulfuric Acid Reaction Formula".to_string();
    sulfuric_acid.primary_product_type_id = 16_661;
    sulfuric_acid.materials = vec![iskworks_sde::RecipeLine {
        type_id: 34,
        type_name: "Tritanium".to_string(),
        quantity: 500,
    }];
    sulfuric_acid.products = vec![iskworks_sde::RecipeLine {
        type_id: 16_661,
        type_name: "Sulfuric Acid".to_string(),
        quantity: 1,
    }];

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(16_633, Decimal::new(500, 0));
    adjusted_prices.insert(34, Decimal::new(5, 0));

    let mut books = BTreeMap::new();
    books.insert(
        16_633,
        sell_book(16_633, "Titanium Chromide", &[("500", 1_000_000)], now),
    );
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(16_656, output_book(16_656, "Fernite Alloy", now));
    // Sulfuric Acid's output book is deliberately much cheaper than its
    // input cost, so it ranks last by sell-side profit/hour -- proves
    // ranking order isn't just insertion order.
    books.insert(
        16_661,
        two_sided_book(16_661, "Sulfuric Acid", &[("1", 100)], &[("1", 100)], now),
    );

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_reaction_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![sulfuric_acid, fernite_alloy],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );

    let evaluation = service
        .evaluate(workspace_id, owner_id, reaction_command(), now)
        .await
        .unwrap();

    assert_eq!(evaluation.candidate_count, 2);
    // Insertion order was [Sulfuric Acid, Fernite Alloy]; the backend's
    // own ranking (not insertion order) puts the higher profit/hour
    // reaction first.
    assert_eq!(
        evaluation
            .rankings
            .sell_side_gross_profit_per_manufacturing_hour
            .iter()
            .map(|entry| entry.product_type_id)
            .collect::<Vec<_>>(),
        vec![16_656, 16_661]
    );

    // The min-gross-margin/max-capital/max-duration filters the
    // frontend applies operate purely on the already-typed
    // candidate/valuation fields -- confirm those fields are populated
    // for a reaction candidate exactly like a manufacturing one, so
    // filtering behaves identically regardless of recipe kind.
    let fernite = evaluation
        .candidates
        .iter()
        .find(|candidate| candidate.product_type_id == 16_656)
        .unwrap();
    assert!(fernite.metrics.capital_required.is_some());
    assert!(fernite.valuations.sell_side.gross_margin_percent.is_some());
    assert!(fernite.effective_duration_seconds.is_some());
}

#[tokio::test]
async fn metamorphosis_regression_is_retained_but_excluded_from_every_ranking() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    // Real SDE identity, confirmed against a live t1-frigates
    // evaluation: product 77114 ("Metamorphosis"), blueprint 79214.
    let metamorphosis = recipe_fixture(
        77_114,
        "Metamorphosis",
        79_214,
        vec![
            (4, "Ships"),
            (1_612, "Special Edition Ships"),
            (1_619, "Special Edition Frigates"),
        ],
    );
    let rifter = recipe_fixture(5_876, "Rifter", 68_357, Vec::new());

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(5, 0));

    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(77_114, output_book(77_114, "Metamorphosis", now));
    books.insert(5_876, output_book(5_876, "Rifter", now));

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![metamorphosis, rifter],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );
    let evaluation = service
        .evaluate(workspace_id, owner_id, valid_command(), now)
        .await
        .unwrap();

    assert_eq!(evaluation.candidate_count, 2);
    assert_eq!(evaluation.excluded_count, 1);

    let metamorphosis_candidate = evaluation
        .candidates
        .iter()
        .find(|candidate| candidate.product_type_id == 77_114)
        .expect("excluded candidates remain in the collection for diagnostics");
    assert_eq!(
        metamorphosis_candidate.eligibility.status,
        OpportunityEligibilityStatus::ExcludedFromDefaultRanking
    );
    assert_eq!(
        metamorphosis_candidate.eligibility.exclusion_reasons,
        vec![crate::OpportunityExclusionReason::sde_special_edition(
            1_612,
            "Special Edition Ships"
        )]
    );

    let rankings = &evaluation.rankings;
    for entries in [
        &rankings.sell_side_gross_profit,
        &rankings.immediate_liquidation_gross_profit,
        &rankings.sell_side_gross_margin,
        &rankings.immediate_liquidation_gross_margin,
        &rankings.sell_side_gross_profit_per_manufacturing_hour,
        &rankings.immediate_liquidation_gross_profit_per_manufacturing_hour,
    ] {
        assert!(
            !entries.iter().any(|entry| entry.product_type_id == 77_114),
            "Metamorphosis must not lead any ranking despite strong evidence"
        );
    }
}

#[tokio::test]
async fn skybreaker_regression_is_thin_stale_and_incomplete_but_still_valued() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let stale_observed_at = now - chrono::Duration::hours(1);
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let mut skybreaker = recipe_fixture(99_001, "Skybreaker", 99_002, Vec::new());
    skybreaker.materials = vec![
        iskworks_sde::RecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity: 100,
        },
        iskworks_sde::RecipeLine {
            type_id: 35,
            type_name: "Pyerite".to_string(),
            quantity: 50,
        },
    ];

    // Pyerite has an explicit zero adjusted price (present, not missing);
    // Tritanium has none at all -- only Tritanium may appear as missing.
    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(35, Decimal::ZERO);

    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(35, sell_book(35, "Pyerite", &[("10", 1_000_000)], now));
    // One unit visible at the best sell price, deep divergent buy depth,
    // and a stale observation time.
    books.insert(
        99_001,
        two_sided_book(
            99_001,
            "Skybreaker",
            &[("2000000", 1)],
            &[("1800000", 50)],
            stale_observed_at,
        ),
    );

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        candidates: vec![skybreaker],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );
    let evaluation = service
        .evaluate(workspace_id, owner_id, valid_command(), now)
        .await
        .unwrap();

    let candidate = &evaluation.candidates[0];
    assert_eq!(candidate.product_type_id, 99_001);

    // Both valuations remain calculable despite thinness and staleness.
    assert_eq!(
        candidate.valuations.sell_side.revenue,
        Some(Money::parse("2000000").unwrap())
    );
    assert_eq!(
        candidate.valuations.immediate_liquidation.revenue,
        Some(Money::parse("1800000").unwrap())
    );

    // Compact depth evidence.
    let evidence = candidate.output_market_evidence.as_ref().unwrap();
    assert_eq!(evidence.best_sell_level_quantity, Some(1));
    assert_eq!(evidence.total_visible_sell_quantity, 1);
    assert_eq!(evidence.best_buy_level_quantity, Some(50));
    assert_eq!(evidence.total_visible_buy_quantity, 50);

    // All three named warning kinds fire.
    assert!(candidate.warnings.iter().any(|warning| warning.kind
        == OpportunityWarningKind::StaleMarketEvidence
        && warning.type_ids == vec![99_001]));
    assert!(candidate
        .warnings
        .iter()
        .any(|warning| warning.kind == OpportunityWarningKind::ThinOutputBook));
    let eiv_warning = candidate
        .warnings
        .iter()
        .find(|warning| warning.kind == OpportunityWarningKind::IncompleteEivBasis)
        .expect("Tritanium's missing adjusted price must surface as a warning");
    assert_eq!(
        eiv_warning.details,
        Some(OpportunityWarningDetails::IncompleteEivBasis {
            missing_materials: vec![OpportunityMissingMaterial {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        })
    );

    // Incomplete EIV basis: only Tritanium, never Pyerite.
    assert!(!candidate.eiv_basis.complete);
    assert_eq!(
        candidate.eiv_basis.missing_materials,
        vec![OpportunityMissingMaterial {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }]
    );

    assert_eq!(
        candidate.quality.evidence_quality,
        crate::OpportunityEvidenceQuality::Weak
    );
}

/// One clean Rifter candidate whose *only* possible warning is
/// market-age. Evaluated twice: with the physical order rows fetched 2h
/// ago and (a) never re-confirmed vs (b) re-confirmed by an ESI 304 5m
/// ago. Covers: effective freshness drives `StaleMarketEvidence` and
/// the evidence-quality downgrade; the readiness fresh/stale counts;
/// and that physical provenance (`oldest_market_observed_at`,
/// `StaleMarketEvidence.details.observed_at`) stays at the 2h mark
/// regardless.
#[tokio::test]
async fn effective_freshness_drives_stale_market_evidence_and_readiness_counts() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let fetched_at = now - chrono::Duration::hours(2);
    let confirmed_at = now - chrono::Duration::minutes(5);
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    let evaluate = |revalidated_at: Option<DateTime<Utc>>| async move {
        let rifter = candidate_recipe(CandidateRecipeIdentity::Manufacturing {
            blueprint_type_id: 68_357,
        });
        let mut adjusted_prices = BTreeMap::new();
        adjusted_prices.insert(34, Decimal::new(5, 0));

        // Material book fresh; output book physically fetched 2h ago.
        let mut output = output_book(5_876, "Rifter", fetched_at);
        output.revalidated_at = revalidated_at;
        for order in &mut output.orders {
            order.revalidated_at = revalidated_at;
        }
        let mut books = BTreeMap::new();
        books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
        books.insert(5_876, output);

        let service = OpportunityQueryService::new(
            std::sync::Arc::new(FixtureIndustryRepository {
                price_source: fixture_price_source(now),
                facility: fixture_facility(now),
                get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
                get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
            }),
            std::sync::Arc::new(FixtureSdeRepository {
                active_sde: fixture_active_sde(now),
                candidates: vec![rifter],
                manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
            }),
            std::sync::Arc::new(FixtureMarketRepository {
                market_source: market_source(now),
                books,
                ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
                get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
                get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
                register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
                public_demand: std::sync::Mutex::default(),
            }),
            std::sync::Arc::new(FixtureAdjustedPriceRepository {
                adjusted_prices,
                latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
            }),
        );
        service
            .evaluate(workspace_id, owner_id, valid_command(), now)
            .await
            .unwrap()
    };

    // (a) never re-confirmed -> stale.
    let stale = evaluate(None).await;
    let stale_candidate = &stale.candidates[0];
    let stale_warning = stale_candidate
        .warnings
        .iter()
        .find(|warning| warning.kind == OpportunityWarningKind::StaleMarketEvidence)
        .expect("2h-old, never-revalidated output book is stale");
    assert_eq!(
        stale_candidate.quality.evidence_quality,
        crate::OpportunityEvidenceQuality::Qualified
    );
    // Material book (observed `now`) is fresh; the 2h-old output book is
    // the only stale one.
    assert_eq!(stale.readiness.market_fresh_count, 1);
    assert_eq!(stale.readiness.market_stale_count, 1);
    // Provenance breadcrumb: physical fetch time, not the freshness gate.
    if let Some(OpportunityWarningDetails::StaleMarketEvidence { observed_at, .. }) =
        &stale_warning.details
    {
        assert_eq!(*observed_at, fetched_at);
    } else {
        panic!("StaleMarketEvidence details missing observed_at");
    }
    assert_eq!(stale.readiness.oldest_market_observed_at, Some(fetched_at));

    // (b) re-confirmed 5m ago via a 304 -> fresh, no age-driven downgrade.
    let fresh = evaluate(Some(confirmed_at)).await;
    let fresh_candidate = &fresh.candidates[0];
    assert!(
        !fresh_candidate
            .warnings
            .iter()
            .any(|warning| warning.kind == OpportunityWarningKind::StaleMarketEvidence),
        "a recently revalidated book must not warn as stale"
    );
    assert_eq!(
        fresh_candidate.quality.evidence_quality,
        crate::OpportunityEvidenceQuality::Strong
    );
    // Both books now count fresh (material by observation, output by
    // revalidation).
    assert_eq!(fresh.readiness.market_fresh_count, 2);
    assert_eq!(fresh.readiness.market_stale_count, 0);
    // Provenance still the physical fetch time despite the revalidation.
    assert_eq!(fresh.readiness.oldest_market_observed_at, Some(fetched_at));
    assert_eq!(fresh.readiness.newest_market_observed_at, Some(now));
}

#[tokio::test]
async fn battleship_regression_orders_by_selected_valuation_not_insertion_order() {
    let now = Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0).unwrap();
    let workspace_id = WorkspaceId(Uuid::from_u128(3));
    let owner_id = OwnerId(Uuid::from_u128(4));

    // All three share material cost, installation cost, and duration --
    // recipe_fixture reuses candidate_recipe's fixed materials/duration
    // for every one of them. Only the output books differ.
    let megathron = recipe_fixture(100, "Megathron", 68_400, Vec::new());
    let apocalypse = recipe_fixture(200, "Apocalypse", 68_401, Vec::new());
    let armageddon = recipe_fixture(300, "Armageddon", 68_402, Vec::new());

    let mut adjusted_prices = BTreeMap::new();
    adjusted_prices.insert(34, Decimal::new(5, 0));

    let mut books = BTreeMap::new();
    books.insert(34, sell_book(34, "Tritanium", &[("5", 1_000_000)], now));
    books.insert(
        100,
        two_sided_book(
            100,
            "Megathron",
            &[("500000000", 100)],
            &[("100000000", 100)],
            now,
        ),
    );
    books.insert(
        200,
        two_sided_book(
            200,
            "Apocalypse",
            &[("300000000", 100)],
            &[("300000000", 100)],
            now,
        ),
    );
    books.insert(
        300,
        two_sided_book(
            300,
            "Armageddon",
            &[("100000000", 100)],
            &[("500000000", 100)],
            now,
        ),
    );

    let industry_repository = std::sync::Arc::new(FixtureIndustryRepository {
        price_source: fixture_price_source(now),
        facility: fixture_facility(now),
        get_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_facility_profile_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let sde_repository = std::sync::Arc::new(FixtureSdeRepository {
        active_sde: fixture_active_sde(now),
        // Deliberately scrambled: matches neither ranking below.
        candidates: vec![apocalypse, armageddon, megathron],
        manufacturable_candidates_calls: std::sync::atomic::AtomicU64::new(0),
    });
    let market_repository = std::sync::Arc::new(FixtureMarketRepository {
        market_source: market_source(now),
        books,
        ensure_esi_price_source_for_scope_calls: std::sync::atomic::AtomicU64::new(0),
        get_market_price_source_calls: std::sync::atomic::AtomicU64::new(0),
        get_source_order_books_calls: std::sync::atomic::AtomicU64::new(0),
        register_market_coverage_calls: std::sync::atomic::AtomicU64::new(0),
        public_demand: std::sync::Mutex::default(),
    });
    let adjusted_price_repository = std::sync::Arc::new(FixtureAdjustedPriceRepository {
        adjusted_prices,
        latest_adjusted_prices_calls: std::sync::atomic::AtomicU64::new(0),
    });

    let service = OpportunityQueryService::new(
        industry_repository,
        sde_repository,
        market_repository,
        adjusted_price_repository,
    );
    let evaluation = service
        .evaluate(workspace_id, owner_id, valid_command(), now)
        .await
        .unwrap();

    // Same material cost, installation cost, and duration across all
    // three -- confirms the crossed rankings below are driven purely by
    // the selected output valuation, not by differing costs.
    let material_costs = evaluation
        .candidates
        .iter()
        .map(|candidate| candidate.metrics.material_cost)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(material_costs.len(), 1);
    let durations = evaluation
        .candidates
        .iter()
        .map(|candidate| candidate.effective_duration_seconds)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(durations.len(), 1);

    let sell_order = evaluation
        .rankings
        .sell_side_gross_profit_per_manufacturing_hour
        .iter()
        .map(|entry| entry.product_type_id)
        .collect::<Vec<_>>();
    let liquidation_order = evaluation
        .rankings
        .immediate_liquidation_gross_profit_per_manufacturing_hour
        .iter()
        .map(|entry| entry.product_type_id)
        .collect::<Vec<_>>();

    assert_eq!(sell_order, vec![100, 200, 300]);
    assert_eq!(liquidation_order, vec![300, 200, 100]);
    assert_ne!(sell_order, liquidation_order);
    // Neither ranking echoes the scrambled candidate-discovery order.
    assert_ne!(sell_order, vec![200, 300, 100]);
    assert_ne!(liquidation_order, vec![200, 300, 100]);
}
