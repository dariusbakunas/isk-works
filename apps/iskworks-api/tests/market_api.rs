use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_app::PublicMarketService;
use iskworks_core::{
    DraftPlanningInput, EsiMarketObservationBatch, EsiMarketOrder, IndustryRepository,
    InventoryRepository, MarketCoverageRegistration, MarketObservationBatchId, MarketOrderSide,
    MarketPricingPolicy, MarketRepository, ProductionRepository, WorkspaceId, WorkspaceRepository,
};
use iskworks_esi::HttpEsiTransport;
use iskworks_sde::SdeReadRepository;
use iskworks_storage::{
    PgIndustryRepository, PgInventoryRepository, PgMarketRepository, PgProductionRepository,
    PgSdeRepository, PgWorkspaceRepository,
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const EXPORT: &str = include_str!(
    "../../../crates/iskworks-core/tests/fixtures/market/Insmother-Tritanium-2026.07.26 192639.txt"
);

async fn fixture(pool: &PgPool) -> AppState {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Market API Test',$2,$3,$3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Market API Test',true,$3,$3)",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture','market-api-fixture','active',true,$2,$2)",
    )
    .bind(import_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES
          ($1,34,'Tritanium',true),
          ($1,35,'Pyerite',true),
          ($1,40,'Mexallon',true),
          ($1,5876,'Rifter',true),
          ($1,6830,'Rifter Blueprint',true),
          ($1,9999,'Pyerite Blueprint',true)
        "#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_blueprints (import_id,blueprint_type_id,name_en,duration_seconds) VALUES ($1,6830,'Rifter Blueprint',600),($1,9999,'Pyerite Blueprint',100)",
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO sde_blueprint_materials (
          import_id,blueprint_type_id,material_type_id,quantity,position
        ) VALUES ($1,6830,34,1000,0),($1,6830,35,200,1),($1,9999,40,10,0)
        "#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO sde_blueprint_products (
          import_id,blueprint_type_id,product_type_id,quantity,position
        ) VALUES ($1,6830,5876,1,0),($1,9999,35,5,0)
        "#,
    )
    .bind(import_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let workspace = PgWorkspaceRepository::new(pool.clone());
    let pg_market = Arc::new(PgMarketRepository::new(pool.clone()));
    let market = pg_market.clone() as Arc<dyn MarketRepository>;
    let industry = Arc::new(PgIndustryRepository::new(pool.clone())) as Arc<dyn IndustryRepository>;
    let inventory =
        Arc::new(PgInventoryRepository::new(pool.clone())) as Arc<dyn InventoryRepository>;
    let production =
        Arc::new(PgProductionRepository::new(pool.clone())) as Arc<dyn ProductionRepository>;
    AppState::new(Arc::new(workspace) as Arc<dyn WorkspaceRepository>)
        .with_market_repository(market)
        .with_sde_repository(
            Arc::new(PgSdeRepository::new(pool.clone())) as Arc<dyn SdeReadRepository>
        )
        .with_public_market_service(PublicMarketService::new(
            pg_market,
            Arc::new(HttpEsiTransport::public("http://127.0.0.1:9".to_string())),
        ))
        .with_industry_repository(industry)
        .with_inventory_repository(inventory)
        .with_production_repository(production)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn inventory_selection_registers_jita_coverage_without_blocking_response(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    let app = build_router(state);
    let opening = serde_json::json!({
        "typeId": 34,
        "typeName": "Tritanium",
        "quantity": 100,
        "unitCost": "3.5000",
        "costQuality": "known",
        "sourceReference": "Jita coverage fixture",
        "note": "",
        "effectiveAt": "2026-07-28T12:00:00Z",
        "expectedRevision": 0,
        "acknowledgeUnknownCost": false,
        "acknowledgeZeroCost": false
    });
    let posted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/opening-balance")
                .header("content-type", "application/json")
                .body(Body::from(opening.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/inventory?priceSourceId={}", source.id.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body[0]["balance"]["key"]["typeId"], 34);
    assert!(body[0]["currentValue"].is_null());
    assert!(body[0]["warnings"][0]
        .as_str()
        .unwrap()
        .contains("refreshed"));
    let coverage_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_source_coverage WHERE price_source_id=$1 AND type_id=34",
    )
    .bind(source.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(coverage_count, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn build_preview_registers_jita_materials_and_output_without_blocking(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    let app = build_router(state);
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6830},
        "runs": 1,
        "materialPricingPolicy": "acquireQuantityFromSellOrders",
        "outputPricingPolicy": "highestBuy"
    });

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
    assert_eq!(response.status(), StatusCode::OK);
    let response_body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(response_body["pricingComplete"], false);
    let covered: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM market_source_coverage WHERE price_source_id=$1 ORDER BY type_id",
    )
    .bind(source.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(covered, vec![34, 35, 5_876]);
    let summarized = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source.id)
        .await
        .unwrap();
    assert_eq!(summarized.item_count, 3);
}

/// `/api/market/regions`, `/api/market/regions/:region_id/locations`, and
/// `/api/market/categories` are pure reference-data reads over the real
/// `PgSdeRepository`/`PgMarketRepository` -- proves the route wiring
/// (region listing, NPC-station-plus-known-structure merging, region
/// scoping/leak prevention, and the market-group tree) end to end, with
/// zero `price_sources` rows ever created and no coverage registered.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn market_reference_endpoints_are_pure_reads_needing_no_price_source(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let import_id: Uuid =
        sqlx::query_scalar("SELECT id FROM sde_imports WHERE source_label='fixture'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge'),($1,10000009,'Insmother')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002),($1,30000772,'C-J6MT',10000009)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_market_groups (import_id,market_group_id,name_en,parent_group_id) VALUES ($1,4,'Ships',NULL),($1,1361,'Frigates',4)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    // A single published type under Frigates -- proves item_count rolls up
    // from real sde_types membership through to the parent ("Ships"), not
    // just returned as a static zero.
    sqlx::query(
        "INSERT INTO sde_types (import_id,type_id,name_en,market_group_id,published) VALUES ($1,587,'Rifter',1361,true)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174021,'Perimeter - The Bar',1,30000142,35825,NULL,now(),now())
        "#,
    )
    .bind(workspace_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let regions_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/regions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(regions_response.status(), StatusCode::OK);
    let regions: Value = serde_json::from_slice(
        &to_bytes(regions_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        regions,
        serde_json::json!([
            {"regionId": 10_000_009, "regionName": "Insmother"},
            {"regionId": 10_000_002, "regionName": "The Forge"},
        ])
    );

    let locations_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/regions/10000002/locations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(locations_response.status(), StatusCode::OK);
    let locations: Value = serde_json::from_slice(
        &to_bytes(locations_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let locations = locations.as_array().unwrap();
    assert_eq!(locations.len(), 2, "{locations:?}");
    let station = locations
        .iter()
        .find(|location| location["kind"] == "npcStation")
        .unwrap();
    assert_eq!(station["locationId"], 60_003_760);
    assert_eq!(
        station["locationName"],
        "Jita IV - Moon 4 - Caldari Navy Assembly Plant"
    );
    assert_eq!(station["solarSystemName"], "Jita");
    let structure = locations
        .iter()
        .find(|location| location["kind"] == "structure")
        .unwrap();
    assert_eq!(structure["locationId"], 1_049_588_174_021_i64);
    assert_eq!(structure["locationName"], "Perimeter - The Bar");
    assert_eq!(structure["structureTypeId"], 35_825);

    // Insmother has neither a known station nor a known structure in this
    // fixture -- an empty, not-erroring, response.
    let insmother_locations_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/regions/10000009/locations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(insmother_locations_response.status(), StatusCode::OK);
    let insmother_locations: Value = serde_json::from_slice(
        &to_bytes(insmother_locations_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(insmother_locations, serde_json::json!([]));

    let categories_response = app
        .oneshot(
            Request::builder()
                .uri("/api/market/categories")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(categories_response.status(), StatusCode::OK);
    let categories: Value = serde_json::from_slice(
        &to_bytes(categories_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        categories,
        serde_json::json!([
            {"marketGroupId": 4, "name": "Ships", "itemCount": 1, "children": [
                {"marketGroupId": 1361, "name": "Frigates", "itemCount": 1, "children": []}
            ]}
        ])
    );

    // None of this required a `price_sources` row to exist.
    let price_source_count: i64 = sqlx::query_scalar("SELECT count(*) FROM price_sources")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price_source_count, 0);
    let coverage_count: i64 = sqlx::query_scalar("SELECT count(*) FROM market_source_coverage")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(coverage_count, 0);
}

/// Market Scope Selector: `/api/market/locations/search` merges
/// hubs, NPC stations, workspace structures, and regions into one
/// kind-tagged list. Covers the property that's uniquely route-level (not
/// testable at the repository layer): a curated hub station is tagged
/// `"hub"` rather than `"npcStation"` purely by station-ID membership in
/// `MAJOR_TRADE_HUBS`, using the very same `search_npc_stations` result an
/// ordinary station also comes from. Also covers the `q`-too-short and
/// no-results empty-list behavior end to end through the real router.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn search_market_locations_tags_hubs_stations_structures_and_regions(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge'),($1,10000009,'Insmother')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002),($1,30000772,'C-J6MT',10000009),($1,30000900,'Structure System',10000009)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id)
        VALUES
          -- Jita 4-4: a real MAJOR_TRADE_HUBS station.
          ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529),
          -- An ordinary, non-hub NPC station.
          ($1,60000001,'C-J6MT I - Random Outpost',30000772,1000001,1928)
        "#,
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names
          (workspace_id,location_id,location_name,owner_id,solar_system_id,structure_type_id,resolved_by_connection_id,resolved_at,updated_at)
        VALUES ($1,1049588174021,'Test Citadel',1,30000900,35825,NULL,now(),now())
        "#,
    )
    .bind(workspace_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    async fn search(app: axum::Router, q: &str) -> Vec<Value> {
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/market/locations/search?q={q}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        body.as_array().unwrap().clone()
    }

    // Jita's station is a curated hub -- tagged "hub", not "npcStation",
    // even though it comes from the exact same search_npc_stations call an
    // ordinary station would.
    let hub_results = search(app.clone(), "Jita").await;
    assert_eq!(hub_results.len(), 1, "{hub_results:?}");
    assert_eq!(hub_results[0]["kind"], "hub");
    assert_eq!(hub_results[0]["locationId"], 60_003_760);
    assert_eq!(hub_results[0]["displayName"], "Jita 4-4");

    // An ordinary NPC station is tagged "npcStation".
    let station_results = search(app.clone(), "Random%20Outpost").await;
    assert_eq!(station_results.len(), 1, "{station_results:?}");
    assert_eq!(station_results[0]["kind"], "npcStation");
    assert_eq!(station_results[0]["locationId"], 60_000_001);

    // A workspace structure is tagged "structure".
    let structure_results = search(app.clone(), "Test%20Citadel").await;
    assert_eq!(structure_results.len(), 1, "{structure_results:?}");
    assert_eq!(structure_results[0]["kind"], "structure");
    assert_eq!(structure_results[0]["locationId"], 1_049_588_174_021_i64);

    // A region name with no matching station/structure is tagged "region"
    // alone.
    let region_results = search(app.clone(), "Forge").await;
    assert_eq!(region_results.len(), 1, "{region_results:?}");
    assert_eq!(region_results[0]["kind"], "region");
    assert_eq!(region_results[0]["regionId"], 10_000_002);
    assert_eq!(region_results[0]["displayName"], "The Forge");

    // Too short to search.
    assert_eq!(search(app.clone(), "J").await, Vec::<Value>::new());

    // No matches anywhere.
    assert_eq!(search(app, "Nonexistent").await, Vec::<Value>::new());
}

/// `/api/market/items` and `/api/market/items/:type_id/orders` against a
/// real Jita `esi_market_orders` source with actual refreshed
/// observations -- proves the `MarketScope` -> `price_source_id` shim
/// resolves real data end to end, that a type with no market group never
/// appears in the summary table, and that fetching from either endpoint
/// registers no *additional* coverage beyond what the test itself set up
/// (both are pure reads).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn market_item_endpoints_resolve_real_scoped_order_book_data(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    // Tritanium (34, already in the base fixture) needs a market group to
    // be browsable; Rifter (5876) deliberately keeps none, to prove it
    // never appears in the item summary table.
    sqlx::query("UPDATE sde_types SET market_group_id=1857 WHERE type_id=34")
        .execute(&pool)
        .await
        .unwrap();

    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    let observed_at = chrono::Utc::now();
    market
        .register_market_coverage(
            workspace_id,
            source.id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let claim = market
        .begin_market_refresh(
            workspace_id,
            source.id,
            34,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    market
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(15),
            EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id: source.id,
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at,
                etag: None,
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![
                    EsiMarketOrder::new(
                        341,
                        MarketOrderSide::Sell,
                        "4.2500",
                        100_000,
                        100_000,
                        1,
                        "station".to_string(),
                        observed_at,
                        90,
                        60_003_760,
                        30_000_142,
                        Some(60_003_760),
                    )
                    .unwrap(),
                    EsiMarketOrder::new(
                        342,
                        MarketOrderSide::Buy,
                        "4.0000",
                        75_000,
                        75_000,
                        1,
                        "region".to_string(),
                        observed_at,
                        90,
                        60_003_760,
                        30_000_142,
                        Some(60_003_760),
                    )
                    .unwrap(),
                ],
            },
        )
        .await
        .unwrap();
    let coverage_before: i64 = sqlx::query_scalar("SELECT count(*) FROM market_source_coverage")
        .fetch_one(&pool)
        .await
        .unwrap();

    let app = build_router(state);

    let items_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/items?regionId=10000002&locationId=60003760&marketGroupId=1857")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(items_response.status(), StatusCode::OK);
    let items: Value = serde_json::from_slice(
        &to_bytes(items_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(items["totalCount"], 1);
    assert_eq!(items["page"], 1);
    let rows = items["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["typeId"], 34);
    assert_eq!(rows[0]["typeName"], "Tritanium");
    assert_eq!(rows[0]["bestSell"], "4.2500");
    assert_eq!(rows[0]["bestBuy"], "4.0000");
    assert_eq!(rows[0]["spread"], "0.2500");
    assert_eq!(rows[0]["sellOrderCount"], 1);
    assert_eq!(rows[0]["buyOrderCount"], 1);
    assert!(!rows[0]["observedAt"].is_null());

    let orders_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/items/34/orders?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(orders_response.status(), StatusCode::OK);
    let orders: Value = serde_json::from_slice(
        &to_bytes(orders_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(orders["typeId"], 34);
    assert_eq!(orders["typeName"], "Tritanium");
    assert_eq!(orders["summary"]["bestSell"], "4.2500");
    assert_eq!(orders["summary"]["bestBuy"], "4.0000");
    let sell_orders = orders["sellOrders"].as_array().unwrap();
    assert_eq!(sell_orders.len(), 1);
    assert_eq!(sell_orders[0]["price"], "4.2500");
    assert_eq!(sell_orders[0]["quantity"], 100_000);
    let buy_orders = orders["buyOrders"].as_array().unwrap();
    assert_eq!(buy_orders.len(), 1);
    assert_eq!(buy_orders[0]["price"], "4.0000");

    // An unregistered/never-refreshed type (35, Pyerite) resolves with an
    // explicit no-data summary, not an error.
    let missing_response = app
        .oneshot(
            Request::builder()
                .uri("/api/market/items/35/orders?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing: Value = serde_json::from_slice(
        &to_bytes(missing_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(missing["summary"]["bestSell"].is_null());
    assert!(missing["sellOrders"].as_array().unwrap().is_empty());

    // Reading these endpoints registered no additional coverage.
    let coverage_after: i64 = sqlx::query_scalar("SELECT count(*) FROM market_source_coverage")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(coverage_before, coverage_after);
}

/// SDE geography the market-data request routes validate scopes against:
/// The Forge with Jita 4-4, and Domain (no stations needed).
async fn seed_request_geography(pool: &PgPool) {
    let import_id: Uuid =
        sqlx::query_scalar("SELECT id FROM sde_imports WHERE source_label='fixture'")
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge'),($1,10000043,'Domain')",
    )
    .bind(import_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002)",
    )
    .bind(import_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529)",
    )
    .bind(import_id)
    .execute(pool)
    .await
    .unwrap();
}

/// The request routes take `regionId`/`locationId` straight from the
/// caller. A scope ESI would reject must be refused up front: once
/// registered, its coverage would be retried against ESI forever, spending
/// the per-IP error budget every tenant shares.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn request_market_data_rejects_an_unknown_scope_before_provisioning(pool: PgPool) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let app = build_router(state);

    for uri in [
        // Not a market region.
        "/api/market/items/34/request?regionId=1",
        // A station the SDE doesn't know, and no known structure either.
        "/api/market/items/34/request?regionId=10000002&locationId=60012345",
        // A real station, but in another region.
        "/api/market/items/34/request?regionId=10000043&locationId=60003760",
        "/api/market/groups/9001/request?regionId=1",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }

    let provisioned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_price_source_configs WHERE source_kind='esi_market_orders'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        provisioned, 0,
        "no price source may be provisioned for a rejected scope"
    );
}

/// With the synchronous single-item refresh, `POST /api/market/items/:type_id/request`
/// now awaits the real ESI fetch instead of spawning it, so with this
/// fixture's always-unreachable transport (`http://127.0.0.1:9`) every real
/// request here genuinely fails and must report that honestly (502, not a
/// blanket 200) -- but registration itself is unconditional and happens
/// before the fetch is attempted, so the bounded-fetch contract (requesting one type registers coverage for exactly that type,
/// never more) still holds and is still verified directly against
/// `market_source_coverage`, independent of the fetch's own outcome.
/// Requesting an unconfigured scope still auto-provisions its source before
/// attempting the fetch; requesting an unknown type still 404s before ever
/// touching coverage.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn request_market_data_registers_exactly_the_one_requested_type(pool: PgPool) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    let app = build_router(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/34/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "the fixture's transport is unreachable, so the awaited fetch must fail honestly"
    );

    // Jita 4-4 is an NPC station: its demand is app-wide, one row per
    // (region, type), and no per-workspace coverage is kept.
    let covered: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM public_market_coverage WHERE region_id=10000002 ORDER BY type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        covered,
        vec![34],
        "must register exactly the requested type, never more, regardless of the fetch's own outcome"
    );
    let per_workspace: i64 =
        sqlx::query_scalar("SELECT count(*) FROM market_source_coverage WHERE price_source_id=$1")
            .bind(source.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(per_workspace, 0);

    // Requesting a second type adds exactly one more row -- still bounded,
    // never a batch/scan.
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/35/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let covered_after_second: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM public_market_coverage WHERE region_id=10000002 ORDER BY type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(covered_after_second, vec![34, 35]);

    // A scope no PriceSource existed for yet: auto-provisions one instead of
    // erroring -- a workspace holds one ESI source per scope, so
    // browsing/requesting a brand-new region needs no manual
    // "create Price Source" step first. Provisioning happens before the
    // (still-failing) fetch is attempted.
    let newly_provisioned = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/34/request?regionId=10000043")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(newly_provisioned.status(), StatusCode::BAD_GATEWAY);
    let provisioned_source_id: Uuid = sqlx::query_scalar(
        "SELECT price_source_id FROM market_price_source_configs WHERE workspace_id=$1 AND region_id=10000043 AND source_kind='esi_market_orders'",
    )
    .bind(workspace_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(
        provisioned_source_id, source.id.0,
        "must provision its own source, not reuse the Jita fixture's"
    );

    // Requesting the same brand-new scope again reuses the same
    // auto-provisioned source rather than creating a second one.
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/35/request?regionId=10000043")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let sources_for_region: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_price_source_configs WHERE workspace_id=$1 AND region_id=10000043 AND source_kind='esi_market_orders'",
    )
    .bind(workspace_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sources_for_region, 1);

    // An unknown type: 404, and it must not have touched coverage at all --
    // never reaches the fetch attempt.
    let unknown_type = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/999999999/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_type.status(), StatusCode::NOT_FOUND);
    let coverage_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public_market_coverage WHERE region_id=10000002")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(coverage_count, 2, "an unknown type touches no coverage");
}

/// The awaited, synchronous
/// single-item refresh reports a real ESI failure as a distinct, clearly
/// coded error -- not a blanket 200, and not conflated with "fetch
/// succeeded, item genuinely has no orders" (that distinction is the whole
/// point of making this endpoint synchronous).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn request_market_data_reports_a_failed_fetch_as_a_distinct_error(pool: PgPool) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/items/34/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "market_refresh_failed");
}

/// `POST /api/market/groups/:market_group_id/request` -- proves the
/// descendant-inclusive resolution (a root category's request also covers
/// its child group's items), that registration is unbounded up to the
/// defensive ceiling (no product-level "category too large" cap), and that
/// the response's `alreadyCurrentCount` reflects real prior state rather
/// than always reporting zero.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn request_market_data_for_group_registers_the_whole_descendant_inclusive_subtree(
    pool: PgPool,
) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO sde_market_groups (import_id,market_group_id,name_en,parent_group_id) VALUES
          ($1,9001,'Minerals Root',NULL),
          ($1,9002,'Minerals Child',9001)
        "#,
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    // Tritanium/Pyerite in the root group, Mexallon in the child -- the
    // request must pick up all three via the child.
    sqlx::query("UPDATE sde_types SET market_group_id=9001 WHERE type_id IN (34,35)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sde_types SET market_group_id=9002 WHERE type_id=40")
        .execute(&pool)
        .await
        .unwrap();
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    // Tritanium already has a current app-wide book before the bulk request
    // -- proves `alreadyCurrentCount` reflects real prior state.
    let now = chrono::Utc::now();
    market
        .register_public_market_demand(
            10_000_002,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
            true,
            now,
        )
        .await
        .unwrap();
    let claim = market
        .begin_public_market_refresh(10_000_002, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert!(market
        .complete_public_market_refresh(
            claim,
            now + chrono::Duration::minutes(15),
            iskworks_core::PublicMarketObservationBatch {
                id: iskworks_core::MarketObservationBatchId::new(),
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                observed_at: now,
                etag: None,
                expires_at: None,
                orders: vec![],
            },
        )
        .await
        .unwrap());
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/groups/9001/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["requestedCount"], 3);
    assert_eq!(body["alreadyCurrentCount"], 1);

    let covered: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM public_market_coverage WHERE region_id=10000002 ORDER BY type_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        covered,
        vec![34, 35, 40],
        "must register every type in the root group and its child, exactly once each"
    );
    let per_workspace: i64 =
        sqlx::query_scalar("SELECT count(*) FROM market_source_coverage WHERE price_source_id=$1")
            .bind(source.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        per_workspace, 0,
        "a public scope keeps no per-workspace coverage"
    );
}

/// The bulk-request ceiling is defensive-only (far above any real category
/// size), not a product cap: a genuinely huge subtree (well beyond the
/// largest real EVE market category) must be rejected with a clear 400
/// rather than silently truncated or accepted into an unbounded write.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn request_market_data_for_group_rejects_a_pathologically_large_subtree(pool: PgPool) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_market_groups (import_id,market_group_id,name_en,parent_group_id) VALUES ($1,9101,'Pathological Group',NULL)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    // One bulk insert, well past MAX_MARKET_GROUP_BULK_REQUEST_ITEMS
    // (20,000) -- far larger than "Ship SKINs", the real SDE's largest
    // rolled-up category at 5,003 items.
    sqlx::query(
        r#"
        INSERT INTO sde_types (import_id,type_id,name_en,market_group_id,published)
        SELECT $1, 800000 + i, 'Pathological Item ' || i, 9101, true
        FROM generate_series(1, 20001) AS i
        "#,
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/market/groups/9101/request?regionId=10000002&locationId=60003760")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let coverage_count: i64 = sqlx::query_scalar("SELECT count(*) FROM market_source_coverage")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        coverage_count, 0,
        "a rejected request must not have registered any coverage"
    );
}

/// `GET /api/market/items/freshness` -- the polling signal the Market
/// Browser will check on an interval instead of re-running
/// `list_market_items`. Proves the happy path (a real completed refresh's
/// timestamp comes back), the no-data path (`null`, not an error or a
/// fabricated timestamp), and the defensive cap on `typeIds` length.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn get_market_items_freshness_reports_the_latest_observation_or_null(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let app = build_router(state);

    // No coverage at all yet for type 34: null, not an error.
    let before = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/market/items/freshness?regionId=10000002&locationId=60003760&typeIds=34")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(before.status(), StatusCode::OK);
    let before_body: Value =
        serde_json::from_slice(&to_bytes(before.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(before_body["mostRecentUpdatedAt"].is_null());

    // Request it, which registers coverage and dispatches an immediate
    // (mocked/failing-fast against 127.0.0.1:9) refresh attempt -- doesn't
    // matter whether it succeeds, we complete one ourselves directly below
    // to get a deterministic, real observed_at to assert against.
    market
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();
    let observed_at = chrono::Utc::now();
    let batch_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO market_observation_batches (
          id,workspace_id,price_source_id,origin,status,type_id,captured_type_name,
          region_id,solar_system_id,location_id,observed_at,attempted_at,completed_at
        ) VALUES ($1,$2,$3,'esi_market_orders','completed',34,'Tritanium',10000002,30000142,60003760,$4,$4,$4)
        "#,
    )
    .bind(batch_id)
    .bind(workspace_id.0)
    .bind(source_id.0)
    .bind(observed_at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE market_source_coverage SET refresh_state='current',last_completed_batch_id=$2 WHERE price_source_id=$1 AND type_id=34",
    )
    .bind(source_id.0)
    .bind(batch_id)
    .execute(&pool)
    .await
    .unwrap();

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/market/items/freshness?regionId=10000002&locationId=60003760&typeIds=34,999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_body: Value =
        serde_json::from_slice(&to_bytes(after.into_body(), usize::MAX).await.unwrap()).unwrap();
    let reported: chrono::DateTime<chrono::Utc> = after_body["mostRecentUpdatedAt"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(reported.timestamp_millis(), observed_at.timestamp_millis());

    // A typeIds list past the page-size cap is rejected rather than
    // silently accepted.
    let too_many_ids = (1..=201)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let over_cap = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/market/items/freshness?regionId=10000002&locationId=60003760&typeIds={too_many_ids}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(over_cap.status(), StatusCode::BAD_REQUEST);
}

/// Previewing a plan's root registers market coverage for its producers'
/// own materials too: core prices them but cannot reach the market layer,
/// so without this their prices never resolve. Mexallon (40) is only in the
/// Pyerite producer's recipe, so its coverage can only come from following
/// the root's Produce edge to that producer.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn build_preview_registers_a_producers_own_materials_for_jita_coverage(pool: PgPool) {
    let state = fixture(&pool).await;
    let workspace_id = WorkspaceId(
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    let market = PgMarketRepository::new(pool.clone());
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let source = PgIndustryRepository::new(pool.clone())
        .get_price_source(workspace_id, source_id)
        .await
        .unwrap();
    // A plan whose root builds Pyerite (35) through its producer Build.
    let draft = DraftPlanningInput {
        material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
        output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
        manual_price_list_id: None,
        expected_manual_price_list_revision: None,
        material_pricing_policy: MarketPricingPolicy::HighestBuy,
        output_pricing_policy: MarketPricingPolicy::LowestSell,
        pricing_selections: Vec::new(),
        blueprint_selection: None,
        manufacturing_facility: None,
        reaction_facility: None,
        facility_eiv_manual: false,
        component_resolutions: vec![iskworks_core::ComponentResolution {
            type_id: 35,
            recipe: iskworks_core::RecipeSelection::Manufacturing {
                blueprint_type_id: 9_999,
            },
            facility_override: None,
            blueprint_selection: None,
        }],
        fulfillment_scopes: Vec::new(),
    };
    let app = build_router(state);
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/builds")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "Rifter batch",
                        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
                        "runs": 1,
                        "draftPlanning": draft,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let parent: serde_json::Value =
        serde_json::from_slice(&to_bytes(created.into_body(), usize::MAX).await.unwrap()).unwrap();
    let parent_id = parent["id"].as_str().unwrap().to_string();

    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": 1,
        "pricingSelections": [],
        "buildId": parent_id,
        "componentResolutions": [
            {"typeId": 35, "recipe": {"mode": "manufacturing", "blueprintTypeId": 9_999}}
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/candidate-preview")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let covered: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM market_source_coverage WHERE price_source_id=$1 ORDER BY type_id",
    )
    .bind(source.id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    // 34/35/5876 are the root's own materials/output; 40 (Mexallon) is only
    // in the Pyerite producer's recipe -- its presence here is the proof.
    assert_eq!(covered, vec![34, 35, 40, 5_876]);
}

fn multipart_request(path: &str) -> Request<Body> {
    multipart_request_with_export(path, EXPORT)
}

fn multipart_request_with_export(path: &str, export: &str) -> Request<Body> {
    let boundary = "iskworks-market-test-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"Insmother-Tritanium-2026.07.26 192639.txt\"\r\nContent-Type: text/plain\r\n\r\n{export}\r\n--{boundary}--\r\n"
    );
    Request::builder()
        .method("POST")
        .uri(path)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn mixed_location_export_previews_and_imports_each_order_book(pool: PgPool) {
    let app = build_router(fixture(&pool).await);
    let mixed = EXPORT.replacen("1049588174021", "60003760", 1);

    let response = app
        .clone()
        .oneshot(multipart_request_with_export(
            "/api/industry/market-imports/preview",
            &mixed,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["validFiles"], 2);
    assert_eq!(body["locationCount"], 2);
    assert_eq!(body["totalRows"], 6);
    assert_eq!(body["files"][0]["locationId"], 60_003_760);
    assert_eq!(body["files"][1]["locationId"], 1_049_588_174_021_i64);

    let imported = app
        .oneshot(multipart_request_with_export(
            "/api/industry/market-imports",
            &mixed,
        ))
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(imported.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["importedFiles"], 2);
    assert_eq!(body["importedObservations"], 6);

    let imported_groups: i64 = sqlx::query_scalar("SELECT count(*) FROM market_import_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(imported_groups, 2);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn location_resolution_endpoint_is_non_blocking_without_esi_and_cached_names_enrich_preview(
    pool: PgPool,
) {
    let app = build_router(fixture(&pool).await);
    let request = Request::builder()
        .method("POST")
        .uri("/api/industry/market-locations/resolve")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "locationIds": [1_049_588_174_021_i64] }).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["configured"], false);
    assert_eq!(body["unresolvedLocationIds"][0], 1_049_588_174_021_i64);

    let workspace_id: Uuid =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        r#"
        INSERT INTO market_location_names (
          workspace_id,location_id,location_name,owner_id,solar_system_id,
          structure_type_id,resolved_by_connection_id,resolved_at,updated_at
        ) VALUES ($1,$2,'C-J6MT - GEZ - Industry',98000001,30000772,35832,NULL,now(),now())
        "#,
    )
    .bind(workspace_id)
    .bind(1_049_588_174_021_i64)
    .execute(&pool)
    .await
    .unwrap();

    let preview = app
        .oneshot(multipart_request("/api/industry/market-imports/preview"))
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(preview.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["files"][0]["locationName"], "C-J6MT - GEZ - Industry");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn multipart_preview_import_and_duplicate_detection(pool: PgPool) {
    let app = build_router(fixture(&pool).await);
    let response = app
        .clone()
        .oneshot(multipart_request("/api/industry/market-imports/preview"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["files"][0]["typeName"], "Tritanium");
    assert_eq!(body["files"][0]["lowestSell"], "3.9700");
    assert_eq!(body["files"][0]["highestBuy"], "3.8100");

    let imported = app
        .clone()
        .oneshot(multipart_request("/api/industry/market-imports"))
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);

    let duplicate = app
        .clone()
        .oneshot(multipart_request("/api/industry/market-imports/preview"))
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(duplicate.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    assert_eq!(body["files"][0]["alreadyImported"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM market_order_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 6);

    // Imported observations resolve directly by scope -- no `PriceSource`
    // needs to be created for them: the workspace's default
    // `MarketScope` points at the same region/location the import landed
    // in, so both the Market Browser's per-type read and Inventory's
    // default-scope valuation see the imported orders without any extra
    // setup step.
    sqlx::query(
        "UPDATE workspaces SET default_market_region_id=10000009, default_market_location_id=1049588174021 WHERE display_name='Market API Test'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let imported_items = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/market/items/34/orders?regionId=10000009&locationId=1049588174021")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(imported_items.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &to_bytes(imported_items.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["typeName"], "Tritanium");
    assert_eq!(body["summary"]["bestSell"], "3.9700");
    assert_eq!(
        body["sellOrders"].as_array().unwrap().len() + body["buyOrders"].as_array().unwrap().len(),
        6
    );
    let opening = serde_json::json!({
        "typeId": 34,
        "typeName": "Tritanium",
        "quantity": 100,
        "unitCost": "3.5000",
        "costQuality": "known",
        "sourceReference": "Market API fixture",
        "note": "",
        "effectiveAt": "2026-07-26T19:30:00Z",
        "expectedRevision": 0,
        "acknowledgeUnknownCost": false,
        "acknowledgeZeroCost": false
    });
    let posted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/opening-balance")
                .header("content-type", "application/json")
                .body(Body::from(opening.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let valued = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valued.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(valued.into_body(), usize::MAX).await.unwrap()).unwrap();
    // Default-`MarketScope` inventory valuation always prices at
    // `HighestBuy` (liquidation value), unlike the old explicit-source
    // path's configurable `pricing_policy` -- see `inventory_scope_preview`.
    assert_eq!(body[0]["currentPrice"], "3.8100");
    assert_eq!(body[0]["currentValue"], "381.0000");
    assert_eq!(body[0]["marketRegionId"], 10_000_009);
    assert_eq!(body[0]["marketLocationId"], 1_049_588_174_021_i64);
}

/// The route-level half of `verify_structure_market_access` that needs no
/// fake ESI transport (the resolver's own behavior with a fake transport,
/// built via the `#[doc(hidden)]` `test-support` constructor
/// `EsiApplicationService::new_for_tests`, is covered by
/// `crates/iskworks-app/src/esi_service/tests.rs` and the route unit tests
/// in `apps/iskworks-api/src/routes/esi.rs`). Proves
/// the route classifies before ever touching ESI: an NPC station is
/// rejected outright (no character access needed for public data), and an
/// unresolved location is rejected with a clear "resolve first" message --
/// neither reaches the "EVE integration not configured" branch even though
/// this fixture has no ESI service wired up at all.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn verify_structure_market_access_classifies_before_touching_esi(pool: PgPool) {
    let state = fixture(&pool).await;
    let import_id: Uuid = sqlx::query_scalar("SELECT id FROM sde_imports WHERE active=true")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sde_regions (import_id,region_id,name_en) VALUES ($1,10000002,'The Forge')",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_solar_systems (import_id,solar_system_id,name_en,region_id) VALUES ($1,30000142,'Jita',10000002)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_npc_stations (import_id,station_id,name_en,solar_system_id,owner_corporation_id,station_type_id) VALUES ($1,60003760,'Jita IV - Moon 4 - Caldari Navy Assembly Plant',30000142,1000035,1529)",
    )
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let npc_station = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/market-locations/60003760/verify-access")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        !npc_station.status().is_success(),
        "an NPC station needs no character access and must be rejected before any ESI call"
    );

    let unresolved = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/market-locations/1050487654321/verify-access")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        !unresolved.status().is_success(),
        "an unresolved location must be rejected with a clear message, not silently treated as a station"
    );
}

/// Global public market data: a workspace that never requested an item
/// and has no price source for the scope still sees its app-wide book --
/// the new-workspace "Tritanium has no price" case.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_workspace_without_its_own_source_sees_the_app_wide_book(pool: PgPool) {
    let state = fixture(&pool).await;
    seed_request_geography(&pool).await;
    let workspace_id: Uuid =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE display_name='Market API Test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let market = PgMarketRepository::new(pool.clone());
    let now = chrono::Utc::now();
    market
        .register_public_market_demand(
            10_000_002,
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
            true,
            now,
        )
        .await
        .unwrap();
    let claim = market
        .begin_public_market_refresh(10_000_002, 34, now, now + chrono::Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    let order = |order_id: i64, price: &str, location_id: i64| {
        iskworks_core::EsiMarketOrder::new(
            order_id,
            iskworks_core::MarketOrderSide::Sell,
            price,
            100_000,
            100_000,
            1,
            "station".to_string(),
            now,
            90,
            location_id,
            30_000_142,
            None,
        )
        .unwrap()
    };
    assert!(market
        .complete_public_market_refresh(
            claim,
            now + chrono::Duration::minutes(15),
            iskworks_core::PublicMarketObservationBatch {
                id: iskworks_core::MarketObservationBatchId::new(),
                type_id: 34,
                type_name: "Tritanium".to_string(),
                region_id: 10_000_002,
                observed_at: now,
                etag: None,
                expires_at: None,
                orders: vec![
                    order(1, "4.2500", 60_003_760),
                    order(2, "4.1000", 60_003_761)
                ],
            },
        )
        .await
        .unwrap());
    let app = build_router(state);
    let get = |uri: &'static str| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            serde_json::from_slice::<Value>(
                &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            )
            .unwrap()
        }
    };

    let jita = get("/api/market/items/34/orders?regionId=10000002&locationId=60003760").await;
    let sells = jita["sellOrders"].as_array().unwrap();
    assert_eq!(sells.len(), 1, "a station sees only its own orders");
    assert_eq!(sells[0]["price"], "4.2500");

    let forge = get("/api/market/items/34/orders?regionId=10000002").await;
    assert_eq!(forge["sellOrders"].as_array().unwrap().len(), 2);
    assert_eq!(forge["summary"]["bestSell"], "4.1000");

    let freshness =
        get("/api/market/items/freshness?regionId=10000002&locationId=60003760&typeIds=34").await;
    assert!(!freshness["mostRecentUpdatedAt"].is_null());

    let sources: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM market_price_source_configs WHERE workspace_id=$1",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sources, 0,
        "reading prices needs no price source of the workspace's own"
    );
}
