//! The Type Info window's route, over fakes: the view's shape, its price
//! at the workspace market, pilot parsing and unknown types.

use std::collections::BTreeMap;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    MarketError, MarketOrderBook, MarketOrderSide, MarketOrderView, MarketPriceSource,
    MarketRepository, MarketScope, Money, PriceSourceId, WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, RequiredSkill, SdeError,
    SdeReadRepository, SkillAttribute, SkillDefinition, TypeBonusLine, TypeBonusSource,
    TypeInfoBase, TypeListEntry, TypeSearchResult, TypeUsage, UsedWithKind,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

mod support;
use support::workspace::configured_workspace;

const HARPY: i64 = 11_381;
const CALDARI_FRIGATE: i64 = 3_330;
const SCRIPT: i64 = 40_334;
const TRITANIUM: i64 = 34;

struct FakeSde;

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
    }
    async fn search_manufacturing_blueprints(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!()
    }
    async fn search_types(&self, _: &str, _: u32) -> Result<Vec<TypeSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn type_info_base(&self, type_id: i64) -> Result<Option<TypeInfoBase>, SdeError> {
        Ok((type_id == HARPY).then(|| TypeInfoBase {
            type_id: HARPY,
            name: "Harpy".into(),
            published: true,
            group_id: Some(324),
            group_name: Some("Assault Frigate".into()),
            category_id: Some(6),
            category_name: Some("Ship".into()),
            meta_group_id: Some(2),
            meta_group_name: Some("Tech II".into()),
            description: Some("A railgun platform.<br>Formidable.".into()),
            volume_m3: Some(Decimal::new(28_600, 0)),
            ..TypeInfoBase::default()
        }))
    }
    async fn type_bonuses(&self, type_id: i64) -> Result<Vec<TypeBonusLine>, SdeError> {
        Ok(if type_id == HARPY {
            vec![TypeBonusLine {
                source: TypeBonusSource::Skill(CALDARI_FRIGATE),
                bonus: Some(Decimal::from(4)),
                unit_id: Some(105),
                unit_display_name: Some("%".into()),
                text: "bonus to all shield resistances".into(),
            }]
        } else {
            Vec::new()
        })
    }
    async fn type_names(&self, type_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        Ok(type_ids
            .iter()
            .filter(|id| **id == CALDARI_FRIGATE)
            .map(|id| (*id, "Caldari Frigate".to_string()))
            .collect())
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        Ok(type_ids
            .iter()
            .filter(|id| **id == HARPY)
            .map(|id| {
                (
                    *id,
                    vec![RequiredSkill {
                        skill_type_id: CALDARI_FRIGATE,
                        level: 5,
                    }],
                )
            })
            .collect())
    }
    async fn type_variations(&self, type_id: i64) -> Result<Vec<TypeListEntry>, SdeError> {
        Ok(if type_id == HARPY {
            vec![
                TypeListEntry {
                    type_id: 603,
                    name: "Merlin".into(),
                    meta_group_id: 1,
                    meta_group_name: "Tech I".into(),
                    meta_level: Some(0),
                },
                TypeListEntry {
                    type_id: HARPY,
                    name: "Harpy".into(),
                    meta_group_id: 2,
                    meta_group_name: "Tech II".into(),
                    meta_level: Some(5),
                },
            ]
        } else {
            Vec::new()
        })
    }
    async fn types_used_with(
        &self,
        type_id: i64,
    ) -> Result<Option<(UsedWithKind, Vec<TypeListEntry>)>, SdeError> {
        Ok((type_id == SCRIPT).then(|| {
            (
                UsedWithKind::Modules,
                vec![TypeListEntry {
                    type_id: 37_543,
                    name: "Guidance Disruptor I".into(),
                    meta_group_id: 1,
                    meta_group_name: "Tech I".into(),
                    meta_level: Some(0),
                }],
            )
        }))
    }
    async fn type_usages(
        &self,
        type_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TypeUsage>, i64), SdeError> {
        let all: Vec<TypeUsage> = if type_id == TRITANIUM {
            (0..120)
                .map(|index| TypeUsage {
                    kind: iskworks_sde::CandidateRecipeKind::Manufacturing,
                    recipe_type_id: 1_000 + index,
                    recipe_name: format!("Thing {index:03} Blueprint"),
                    product_type_id: 2_000 + index,
                    product_name: format!("Thing {index:03}"),
                    quantity: 314,
                })
                .collect()
        } else {
            Vec::new()
        };
        let total = all.len() as i64;
        Ok((
            all.into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect(),
            total,
        ))
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        Ok(vec![SkillDefinition {
            type_id: CALDARI_FRIGATE,
            name: "Caldari Frigate".into(),
            group_id: 257,
            group_name: "Spaceship Command".into(),
            rank: 2,
            primary_attribute: SkillAttribute::Perception,
            secondary_attribute: SkillAttribute::Willpower,
            omega_only: false,
            prerequisites: Vec::new(),
        }])
    }
}

/// Sells the Harpy at 36.8M (and 40M).
struct FakeMarket;

#[async_trait]
impl MarketRepository for FakeMarket {
    async fn scoped_order_books(
        &self,
        _: WorkspaceId,
        _: MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<MarketOrderView>>, MarketError> {
        let order = |price: i64| MarketOrderView {
            observation_id: None,
            import_batch_id: None,
            imported_file_id: None,
            order_id: price,
            type_id: HARPY,
            type_name: String::new(),
            side: MarketOrderSide::Sell,
            price: Money(Decimal::from(price)),
            remaining_volume: 3,
            entered_volume: 3,
            minimum_volume: 1,
            order_range: 0,
            issued_at: Utc::now(),
            duration_days: 90,
            observed_at: Utc::now(),
            revalidated_at: None,
            location_id: 60_003_760,
            solar_system_id: 30_000_142,
            region_id: 10_000_002,
            jumps: 0,
        };
        Ok(type_ids
            .iter()
            .filter(|id| **id == HARPY)
            .map(|id| (*id, vec![order(40_000_000), order(36_800_000)]))
            .collect())
    }
    async fn resolve_type_name(&self, _: i64) -> Result<Option<String>, MarketError> {
        unimplemented!()
    }
    async fn location_names(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<BTreeMap<i64, String>, MarketError> {
        unimplemented!()
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketLocation>,
    ) -> Result<(), MarketError> {
        unimplemented!()
    }
    async fn imported_file_checksums(
        &self,
        _: WorkspaceId,
        _: &[String],
    ) -> Result<std::collections::BTreeSet<String>, MarketError> {
        unimplemented!()
    }
    async fn commit_import(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn list_imports(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::MarketImportBatch>, MarketError> {
        unimplemented!()
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: iskworks_core::MarketImportBatchId,
    ) -> Result<iskworks_core::MarketImportBatch, MarketError> {
        unimplemented!()
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<MarketOrderBook, MarketError> {
        unimplemented!()
    }
    async fn get_source_order_books(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
        _: &[i64],
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<BTreeMap<i64, MarketOrderBook>, MarketError> {
        unimplemented!()
    }
    async fn get_market_price_source(
        &self,
        _: WorkspaceId,
        _: PriceSourceId,
    ) -> Result<MarketPriceSource, MarketError> {
        unimplemented!()
    }
}

fn app(market: bool) -> axum::Router {
    let mut state = AppState::new(Arc::new(configured_workspace("Type Info Test")))
        .with_sde_repository(Arc::new(FakeSde));
    if market {
        state = state.with_market_repository(Arc::new(FakeMarket));
    }
    build_router(state)
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn type_info_has_the_windows_shape() {
    let (status, body) = get(&app(true), &format!("/api/types/{HARPY}/info")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["typeId"], HARPY);
    assert_eq!(body["name"], "Harpy");
    assert_eq!(
        body["group"],
        json!({ "id": 324, "name": "Assault Frigate" })
    );
    assert_eq!(body["metaGroup"]["name"], "Tech II");
    assert_eq!(
        body["description"],
        json!([
            { "kind": "text", "text": "A railgun platform.", "bold": false },
            { "kind": "break" },
            { "kind": "text", "text": "Formidable.", "bold": false },
        ])
    );
    assert_eq!(body["basics"]["volumeM3"], "28600");
    assert_eq!(body["basics"]["massKg"], Value::Null);
    assert_eq!(
        body["traits"],
        json!([{
            "heading": "Caldari Frigate bonuses (per skill level)",
            "skillTypeId": CALDARI_FRIGATE,
            "bonuses": [{
                "amount": "4%",
                "text": [{ "kind": "text", "text": "bonus to all shield resistances", "bold": false }],
            }],
        }])
    );
    assert_eq!(
        body["requirements"],
        json!([{
            "skillTypeId": CALDARI_FRIGATE,
            "name": "Caldari Frigate",
            "level": 5,
            "trainedLevel": 5,
            "children": [],
        }])
    );
    assert_eq!(body["missing"], Value::Null);
    assert_eq!(body["attributes"], json!([]));
    assert_eq!(
        body["industry"],
        json!({ "producedBy": [], "reprocessesInto": [], "usedInCount": 0 })
    );
    assert_eq!(body["variationCount"], 0);
    assert_eq!(body["usedWith"], Value::Null);
    // The lowest sell at the workspace's market (Jita 4-4 by default).
    assert_eq!(body["price"]["bestSell"], "36800000");
    assert_eq!(body["price"]["scope"]["locationId"], 60_003_760);
    assert!(body["price"]["observedAt"].is_string());
}

#[tokio::test]
async fn without_market_data_the_price_is_left_out() {
    let (status, body) = get(&app(false), &format!("/api/types/{HARPY}/info")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["price"], Value::Null);
}

#[tokio::test]
async fn all_zero_checks_requirements_untrained() {
    let (status, body) = get(
        &app(false),
        &format!("/api/types/{HARPY}/info?pilot=allZero"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["requirements"][0]["trainedLevel"], 0);
}

#[tokio::test]
async fn an_unknown_pilot_is_a_bad_request() {
    let (status, body) = get(
        &app(false),
        &format!("/api/types/{HARPY}/info?pilot=nobody"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn an_unknown_type_is_not_found() {
    let (status, body) = get(&app(false), "/api/types/77777/info").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "type_not_found");
}

#[tokio::test]
async fn variations_come_grouped_by_meta_group() {
    let (status, body) = get(&app(false), &format!("/api/types/{HARPY}/variations")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!([
            { "metaGroupId": 1, "metaGroupName": "Tech I",
              "items": [{ "typeId": 603, "name": "Merlin", "metaLevel": 0 }] },
            { "metaGroupId": 2, "metaGroupName": "Tech II",
              "items": [{ "typeId": HARPY, "name": "Harpy", "metaLevel": 5 }] },
        ])
    );
}

#[tokio::test]
async fn used_with_says_which_way_it_points() {
    let (status, body) = get(&app(false), &format!("/api/types/{SCRIPT}/used-with")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["kind"], "modules");
    assert_eq!(
        body["groups"][0]["items"][0]["name"],
        "Guidance Disruptor I"
    );

    let (_, none) = get(&app(false), &format!("/api/types/{HARPY}/used-with")).await;
    assert_eq!(none, json!({ "kind": null, "groups": [] }));
}

#[tokio::test]
async fn used_in_pages_fifty_at_a_time_by_default() {
    let (status, body) = get(&app(false), &format!("/api/types/{TRITANIUM}/used-in")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 120);
    assert_eq!(body["limit"], 50);
    assert_eq!(body["items"].as_array().unwrap().len(), 50);
    assert_eq!(body["nextOffset"], 50);
    assert_eq!(
        body["items"][0],
        json!({
            "kind": "manufacturing",
            "recipeTypeId": 1_000,
            "recipeName": "Thing 000 Blueprint",
            "productTypeId": 2_000,
            "productName": "Thing 000",
            "quantity": 314,
        })
    );

    let (_, last) = get(
        &app(false),
        &format!("/api/types/{TRITANIUM}/used-in?limit=500&offset=100"),
    )
    .await;
    assert_eq!(last["limit"], 100, "capped");
    assert_eq!(last["items"].as_array().unwrap().len(), 20);
    assert_eq!(last["nextOffset"], Value::Null);
}

#[tokio::test]
async fn a_bad_page_query_is_a_bad_request() {
    let (status, _) = get(
        &app(false),
        &format!("/api/types/{TRITANIUM}/used-in?limit=-1"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
