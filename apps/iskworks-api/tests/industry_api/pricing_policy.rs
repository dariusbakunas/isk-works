use super::*;

// ─── Tickets / Acquisition Runs ─────────────────────────────────────────

// ─── Real dependency model regression coverage ────────────────────────────
//
// Uses `committed_plan_with_two_priced_phases_via_http`'s Rifter/Hull
// Section/Tritanium/Pyerite shape, which is exactly the reported bug's
// structure: Tritanium (type 34, ≈ Vexor) is a direct root material,
// consumed only by Final Assembly (Rifter, type 5_876, ≈ Ishtar); Hull
// Section (type 90_001, ≈ Crystalline Carbonide Armor Plate) is a
// Build-resolved intermediate consumed by Final Assembly, needing its own
// material Pyerite (type 35, ≈ Armor Plate's own materials) which Final
// Assembly never touches directly.

// ─── Material/output pricing policy wiring ────────────────────────────────
//
// `DraftPlanningInput.material_pricing_policy`/`output_pricing_policy` were
// round-tripped through `build_draft_planning.planning_input` but never
// actually reached `IndustryService::preview_plan`, which hard-coded
// `HighestBuy`/`LowestSell` regardless of what was stored or sent on the
// wire. `FixtureIndustryRepository`'s own `derive_market_price_items` (used
// by every other test in this file) ignores `MarketPriceRequest::pricing_policy`
// entirely -- it's a flat type_id -> price lookup -- so it can't tell these
// policies apart. `DepthAwareIndustryRepository` below runs the real,
// pure `calculate_market_depth` against a fixed order book instead, so a
// depth-aware policy (`AcquireQuantityFromSellOrders`,
// `LiquidateQuantityIntoBuyOrders`) genuinely prices differently than a
// best-single-order one (`LowestSell`, `HighestBuy`) walking the same book.

/// Backs the pricing-policy regression tests below. Only `get_price_source` and
/// `derive_market_price_items` do real work; every other method is an
/// unreachable stub, since `preview_plan` never calls them for a
/// `build_id: None` request with no facility/component selection (matching
/// `FixtureIndustryRepository`'s own precedent of stubbing out whatever a
/// given test's request shape never reaches).
struct DepthAwareIndustryRepository {
    price_source: PriceSource,
    order_books: std::collections::BTreeMap<i64, Vec<iskworks_core::MarketOrderView>>,
}

#[async_trait]
impl IndustryRepository for DepthAwareIndustryRepository {
    async fn list_builds(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
    ) -> Result<Vec<Build>, IndustryError> {
        Ok(Vec::new())
    }

    async fn get_build(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _build_id: BuildId,
    ) -> Result<Build, IndustryError> {
        Err(IndustryError::BuildNotFound)
    }

    async fn create_build(&self, _new_build: NewBuild) -> Result<Build, IndustryError> {
        Err(IndustryError::RevisionConflict)
    }

    async fn update_draft(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _build_id: BuildId,
        _update: DraftUpdate,
    ) -> Result<Build, IndustryError> {
        Err(IndustryError::RevisionConflict)
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
        Ok(vec![self.price_source.clone()])
    }

    async fn get_price_source(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
        _source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        Ok(self.price_source.clone())
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
        _evidence: Option<&iskworks_core::MarketScopeEvidence>,
    ) -> Result<Vec<iskworks_core::PriceSourceItem>, IndustryError> {
        // Mirrors `PgIndustryRepository::derive_market_price_items`
        // (crates/iskworks-storage/src/industry/repository_impl.rs): a
        // request whose book can't produce a price is simply omitted
        // (the caller sees `missing: true`), never an error.
        Ok(requests
            .into_iter()
            .filter_map(|request| {
                let orders = self.order_books.get(&request.type_id)?;
                let depth = iskworks_core::calculate_market_depth(
                    orders,
                    request.pricing_policy,
                    request.requested_quantity,
                )
                .expect("fixture order book is well-formed");
                let price = depth.average_unit_price?;
                Some(iskworks_core::PriceSourceItem {
                    type_id: request.type_id,
                    type_name: request.type_name,
                    price,
                    note: String::new(),
                    updated_at: chrono::Utc::now(),
                })
            })
            .collect())
    }
}

fn depth_fixture_order(
    order_id: i64,
    type_id: i64,
    side: iskworks_core::MarketOrderSide,
    price: &str,
    remaining_volume: u64,
) -> iskworks_core::MarketOrderView {
    let now = chrono::Utc::now();
    iskworks_core::MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id,
        type_id,
        type_name: String::new(),
        side,
        price: iskworks_core::Money::parse(price).unwrap(),
        remaining_volume,
        entered_volume: remaining_volume,
        minimum_volume: 1,
        order_range: 32_767,
        issued_at: now,
        duration_days: 90,
        observed_at: now,
        revalidated_at: None,
        location_id: 60_003_760,
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        jumps: 0,
    }
}

/// Blueprint 6_830 (the same "Rifter" fixture recipe `FixtureSdeRepository`
/// already defines elsewhere in this file: Tritanium 100/run, Rifter Hull
/// Section 2/run, producing 1 Rifter/run) at 6 runs, so Tritanium's
/// requested quantity is 600, Rifter's is 6 -- large enough that a
/// quantity-weighted policy has to walk past the first order-book level to
/// fully cover the request, while a best-single-order policy never does.
/// Both sides of the book are populated for both the material and the
/// output type, so whichever policy pre-fix's hard-coded default would have
/// applied (`HighestBuy` for materials, `LowestSell` for output) still
/// resolves to a *defined* (just wrong) price rather than `missing`, making
/// the policy actually used unambiguous from the response alone.
fn depth_fixture_order_books(
) -> std::collections::BTreeMap<i64, Vec<iskworks_core::MarketOrderView>> {
    use iskworks_core::MarketOrderSide::{Buy, Sell};
    std::collections::BTreeMap::from([
        (
            // Tritanium (34): the material under test.
            // lowestSell (best single order) -> 10.0000.
            // acquireQuantityFromSellOrders (600 units) -> walks 200@10 +
            // 400@25 = 12_000 / 600 = 20.0000.
            34,
            vec![
                depth_fixture_order(1, 34, Sell, "10.0000", 200),
                depth_fixture_order(2, 34, Sell, "25.0000", 2_000),
                depth_fixture_order(3, 34, Buy, "1.0000", 5_000),
            ],
        ),
        (
            // Rifter Hull Section (90_001): the recipe's other material,
            // never asserted on -- one deep sell order so it always
            // resolves under whichever sell-side material policy is under
            // test, keeping it out of the way of the Tritanium assertions.
            90_001,
            vec![depth_fixture_order(4, 90_001, Sell, "5.0000", 10_000)],
        ),
        (
            // Rifter (5_876): the output under test.
            // highestBuy (best single order) -> 50.0000.
            // liquidateQuantityIntoBuyOrders (6 units) -> walks 3@50 + 3@30
            // = 240 / 6 = 40.0000.
            5_876,
            vec![
                depth_fixture_order(5, 5_876, Buy, "50.0000", 3),
                depth_fixture_order(6, 5_876, Buy, "30.0000", 2_000),
                depth_fixture_order(7, 5_876, Sell, "99999.0000", 5_000),
            ],
        ),
    ])
}

fn depth_fixture_price_source() -> PriceSource {
    PriceSource {
        id: PriceSourceId::new(),
        workspace_id: iskworks_core::WorkspaceId::new(),
        name: "Depth-aware fixture source".to_string(),
        description: String::new(),
        // `EveClientMarketExport`, not `EsiMarketOrders`, so this stays a
        // fast fake-repository-backed test -- `EsiMarketOrders` also
        // registers live ESI coverage via `PublicMarketService`, which this
        // fixture doesn't configure (same reasoning as the existing
        // `candidate_preview_prices_a_buy_resolved_root_material_from_an_order_book_source_alongside_a_build_resolved_one`
        // test above).
        kind: PriceSourceKind::EveClientMarketExport,
        revision: 1,
        item_count: 0,
        recent_build_count: 0,
        items: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn app_with_depth_aware_price_source(price_source: PriceSource) -> axum::Router {
    let new_workspace = NewWorkspace::manual("Industry".to_string());
    let workspace_state = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
    build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: workspace_state,
        }))
        .with_industry_repository(Arc::new(DepthAwareIndustryRepository {
            price_source,
            order_books: depth_fixture_order_books(),
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository)),
    )
}

async fn preview_snapshot_line(
    app: axum::Router,
    source_id: PriceSourceId,
    runs: u64,
    material_pricing_policy: &str,
    output_pricing_policy: &str,
    pricing_selections: serde_json::Value,
) -> serde_json::Value {
    let body = serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 6_830},
        "runs": runs,
        "manualPriceListId": source_id,
        "expectedManualPriceListRevision": 1,
        "materialPricingPolicy": material_pricing_policy,
        "outputPricingPolicy": output_pricing_policy,
        "pricingSelections": pricing_selections,
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/build-plans/preview")
                .header("content-type", "application/json")
                .body(Body::from(body))
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

fn snapshot_line(json: &serde_json::Value, type_id: i64) -> &serde_json::Value {
    json["snapshot"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["typeId"] == type_id)
        .unwrap_or_else(|| panic!("no snapshot line for type {type_id} in {json:?}"))
}

/// The core regression: before this fix, `IndustryService::preview_plan`
/// hard-coded `HighestBuy` for every material line regardless of
/// `DraftPlanningInput`/`PreviewBuildPlanCommand`'s own
/// `material_pricing_policy` -- so requesting `lowestSell` vs
/// `acquireQuantityFromSellOrders` (same recipe, same order book, same
/// quantity) would have produced the *same* price both times (whatever
/// `HighestBuy` resolved to against Tritanium's buy-side book: 1.0000).
/// After the fix, the two policies price Tritanium differently, matching
/// what `calculate_market_depth` actually computes for each.
#[tokio::test]
async fn material_pricing_policy_changes_the_price_used_for_material_lines() {
    let source = depth_fixture_price_source();
    let source_id = source.id;

    let lowest_sell = preview_snapshot_line(
        app_with_depth_aware_price_source(source.clone()),
        source_id,
        6,
        "lowestSell",
        "highestBuy",
        serde_json::json!([]),
    )
    .await;
    let tritanium = snapshot_line(&lowest_sell, 34);
    assert_eq!(tritanium["pricingPolicy"], "lowestSell");
    assert_eq!(tritanium["price"], "10.0000");
    assert_eq!(tritanium["missing"], false);

    let acquire_quantity = preview_snapshot_line(
        app_with_depth_aware_price_source(source),
        source_id,
        6,
        "acquireQuantityFromSellOrders",
        "highestBuy",
        serde_json::json!([]),
    )
    .await;
    let tritanium = snapshot_line(&acquire_quantity, 34);
    assert_eq!(tritanium["pricingPolicy"], "acquireQuantityFromSellOrders");
    assert_eq!(tritanium["price"], "20.0000");
    assert_eq!(tritanium["missing"], false);
}

/// Same regression as above, for the output side: before this fix,
/// `output_pricing_policy` was hard-coded to `LowestSell` regardless of
/// what was requested, so `highestBuy` vs `liquidateQuantityIntoBuyOrders`
/// would have produced the same (sell-side) price for Rifter both times.
#[tokio::test]
async fn output_pricing_policy_changes_the_price_used_for_output_lines() {
    let source = depth_fixture_price_source();
    let source_id = source.id;

    let highest_buy = preview_snapshot_line(
        app_with_depth_aware_price_source(source.clone()),
        source_id,
        6,
        "lowestSell",
        "highestBuy",
        serde_json::json!([]),
    )
    .await;
    let rifter = snapshot_line(&highest_buy, 5_876);
    assert_eq!(rifter["itemRole"], "output");
    assert_eq!(rifter["pricingPolicy"], "highestBuy");
    assert_eq!(rifter["price"], "50.0000");
    assert_eq!(rifter["missing"], false);

    let liquidate_quantity = preview_snapshot_line(
        app_with_depth_aware_price_source(source),
        source_id,
        6,
        "lowestSell",
        "liquidateQuantityIntoBuyOrders",
        serde_json::json!([]),
    )
    .await;
    let rifter = snapshot_line(&liquidate_quantity, 5_876);
    assert_eq!(rifter["pricingPolicy"], "liquidateQuantityIntoBuyOrders");
    assert_eq!(rifter["price"], "40.0000");
    assert_eq!(rifter["missing"], false);
}

/// Proves material and output policies are independently honored within
/// one single preview call, not just one-at-a-time in isolation -- the
/// concrete "acquisition and output valuation may use different policies"
/// capability.
#[tokio::test]
async fn material_and_output_pricing_policies_can_differ_within_one_build() {
    let source = depth_fixture_price_source();
    let source_id = source.id;

    let json = preview_snapshot_line(
        app_with_depth_aware_price_source(source),
        source_id,
        6,
        "acquireQuantityFromSellOrders",
        "liquidateQuantityIntoBuyOrders",
        serde_json::json!([]),
    )
    .await;

    let tritanium = snapshot_line(&json, 34);
    assert_eq!(tritanium["pricingPolicy"], "acquireQuantityFromSellOrders");
    assert_eq!(tritanium["price"], "20.0000");

    let rifter = snapshot_line(&json, 5_876);
    assert_eq!(rifter["pricingPolicy"], "liquidateQuantityIntoBuyOrders");
    assert_eq!(rifter["price"], "40.0000");
}

/// Guards the override path: an explicit per-item
/// `pricing_selections` override must still win over the build-level
/// `material_pricing_policy` now that the latter is finally honored --
/// wiring up the stored policy must not have regressed the override path
/// that already worked. Tritanium is overridden to a manual 99.0000, well
/// away from either the `acquireQuantityFromSellOrders` price this build
/// would otherwise resolve to (20.0000) or the `lowestSell` one (10.0000).
#[tokio::test]
async fn per_item_manual_override_still_takes_precedence_over_the_stored_material_policy() {
    let source = depth_fixture_price_source();
    let source_id = source.id;

    let json = preview_snapshot_line(
        app_with_depth_aware_price_source(source),
        source_id,
        6,
        "acquireQuantityFromSellOrders",
        "highestBuy",
        serde_json::json!([{
            "typeId": 34,
            "role": "material",
            "selection": {"kind": "manual", "unit_price": "99.0000"},
        }]),
    )
    .await;

    let tritanium = snapshot_line(&json, 34);
    assert_eq!(tritanium["selectionKind"], "manual");
    assert_eq!(tritanium["price"], "99.0000");
    assert_eq!(tritanium["missing"], false);
}
