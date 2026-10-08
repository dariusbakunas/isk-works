//! SQL-backed tests for the Build Graph market-evidence pin
//! (`PgMarketRepository::resolve_scope_evidence` +
//! `PgIndustryRepository::derive_market_price_items(evidence)`).
//!
//! Every test runs against a real Postgres via `#[sqlx::test]`, gated
//! `#[ignore]` like the rest of the storage suite.

use super::common::*;

use iskworks_core::{MarketScope, MarketScopeEvidence};

/// Register coverage, begin a refresh, and complete one ESI observation
/// batch for `type_id` at `observed_at` with a single sell order at
/// `sell_price` (best ask == that price). Returns the batch id.
async fn complete_esi_batch(
    repository: &PgMarketRepository,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    type_id: i64,
    type_name: &str,
    observed_at: DateTime<Utc>,
    sell_price: &str,
) -> MarketObservationBatchId {
    repository
        .register_market_coverage(
            workspace_id,
            source_id,
            vec![MarketCoverageRegistration {
                type_id,
                type_name: type_name.to_string(),
            }],
        )
        .await
        .unwrap();
    let claim = repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            type_id,
            observed_at,
            observed_at + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    let batch_id = MarketObservationBatchId::new();
    repository
        .complete_esi_market_refresh(
            workspace_id,
            claim,
            observed_at + chrono::Duration::minutes(1),
            EsiMarketObservationBatch {
                id: batch_id,
                source_id,
                type_id,
                type_name: type_name.to_string(),
                region_id: 10_000_002,
                solar_system_id: 30_000_142,
                location_id: 60_003_760,
                observed_at,
                etag: None,
                expires_at: Some(observed_at + chrono::Duration::minutes(5)),
                orders: vec![EsiMarketOrder::new(
                    type_id * 1000 + 1,
                    MarketOrderSide::Sell,
                    sell_price,
                    1_000_000,
                    1_000_000,
                    1,
                    "station".to_string(),
                    observed_at,
                    90,
                    60_003_760,
                    30_000_142,
                    Some(60_003_760),
                )
                .unwrap()],
            },
        )
        .await
        .unwrap();
    batch_id
}

/// A second workspace/owner in the same test DB -- `fixture()` can't be
/// called twice (it inserts the one allowed active `sde_imports` row).
async fn extra_workspace(pool: &PgPool) -> WorkspaceId {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = crate::db_now();
    // The workspace <-> owner FKs are mutually referential; insert both in
    // one transaction so the deferred check passes at commit (same as
    // `fixture`).
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Market Test 2',$2,$3,$3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Market Test 2',true,$3,$3)",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    WorkspaceId(workspace_id)
}

fn jita_scope() -> MarketScope {
    MarketScope {
        region_id: 10_000_002,
        location_id: Some(60_003_760),
    }
}

fn sell_request(type_id: i64) -> MarketPriceRequest {
    MarketPriceRequest {
        type_id,
        type_name: "Tritanium".to_string(),
        requested_quantity: 1,
        pricing_policy: MarketPricingPolicy::LowestSell,
    }
}

fn price(items: &[iskworks_core::PriceSourceItem], type_id: i64) -> Money {
    items.iter().find(|i| i.type_id == type_id).unwrap().price
}

// 1. `resolve_scope_evidence` returns the completed ESI + import evidence.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_scope_evidence_returns_completed_observation_and_import_evidence(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let observed_at = crate::db_now() - chrono::Duration::minutes(10);
    let batch = complete_esi_batch(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        observed_at,
        "4.0000",
    )
    .await;

    // An import in a *different* region (the fixture file is Insmother /
    // region 10_000_009) -- it must NOT show up as this Jita scope's
    // evidence.
    let mut import = upload();
    import.user_observed_at = Some(crate::db_now() - chrono::Duration::minutes(5));
    MarketService::new(repository.clone())
        .import(workspace_id, vec![import])
        .await
        .unwrap();

    let before = crate::db_now();
    let evidence = PgIndustryRepository::new(pool.clone())
        .resolve_market_evidence(workspace_id, jita_scope())
        .await
        .unwrap();
    let after = Utc::now();

    assert_eq!(evidence.scope, jita_scope());
    assert_eq!(evidence.observation_batch_ids, vec![batch]);
    assert_eq!(
        evidence.import_batch_id, None,
        "out-of-region import excluded"
    );
    assert_eq!(evidence.observed_at, Some(observed_at));
    assert!(evidence.as_of >= before && evidence.as_of <= after);
}

// 1b. An import within the scope's region is picked up as import evidence.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_scope_evidence_picks_up_an_in_scope_import_batch(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let insmother_scope = MarketScope {
        region_id: 10_000_009,
        location_id: Some(1_049_588_174_021),
    };
    let mut import = upload();
    import.filename = "insmother.csv".to_string(); // no parseable timestamp -> user_observed_at wins
    let observed = crate::db_now() - chrono::Duration::minutes(20);
    import.user_observed_at = Some(observed);
    let import_batch = MarketService::new(repository.clone())
        .import(workspace_id, vec![import])
        .await
        .unwrap()
        .batch
        .unwrap()
        .id;

    let evidence = PgIndustryRepository::new(pool.clone())
        .resolve_market_evidence(workspace_id, insmother_scope)
        .await
        .unwrap();

    assert_eq!(evidence.import_batch_id, Some(import_batch));
    assert!(evidence.observation_batch_ids.is_empty()); // no ESI source here
    assert_eq!(evidence.observed_at, Some(observed));
}

// 2. In-progress / non-completed batches are never selected.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_scope_evidence_ignores_an_in_progress_refresh(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let completed = complete_esi_batch(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        crate::db_now() - chrono::Duration::minutes(30),
        "4.0000",
    )
    .await;

    // Start another refresh for the same type but never complete it.
    let now = crate::db_now();
    repository
        .begin_market_refresh(
            workspace_id,
            source_id,
            34,
            now,
            now + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();

    let evidence = PgIndustryRepository::new(pool.clone())
        .resolve_market_evidence(workspace_id, jita_scope())
        .await
        .unwrap();
    // Still only the completed batch -- `last_completed_batch_id` never
    // points at the in-flight one.
    assert_eq!(evidence.observation_batch_ids, vec![completed]);
}

// 3. Evidence is isolated by workspace and by scope region/location.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_scope_evidence_is_isolated_by_workspace_and_scope(pool: PgPool) {
    let (workspace_a, repo_a) = fixture(&pool).await;
    let source_a = esi_source(&pool, workspace_a).await;
    let batch_a = complete_esi_batch(
        &repo_a,
        workspace_a,
        source_a,
        34,
        "Tritanium",
        crate::db_now() - chrono::Duration::minutes(10),
        "4.0000",
    )
    .await;

    let workspace_b = extra_workspace(&pool).await;
    let repo_b = repo_a.clone();
    let source_b = esi_source(&pool, workspace_b).await;
    let batch_b = complete_esi_batch(
        &repo_b,
        workspace_b,
        source_b,
        34,
        "Tritanium",
        crate::db_now() - chrono::Duration::minutes(10),
        "9.0000",
    )
    .await;

    let industry = PgIndustryRepository::new(pool.clone());
    let ev_a = industry
        .resolve_market_evidence(workspace_a, jita_scope())
        .await
        .unwrap();
    let ev_b = industry
        .resolve_market_evidence(workspace_b, jita_scope())
        .await
        .unwrap();
    assert_eq!(ev_a.observation_batch_ids, vec![batch_a]);
    assert_eq!(ev_b.observation_batch_ids, vec![batch_b]);

    // A different region for the same workspace sees no evidence.
    let other_region = MarketScope {
        region_id: 10_000_043,
        location_id: Some(60_008_494),
    };
    let ev_other = industry
        .resolve_market_evidence(workspace_a, other_region)
        .await
        .unwrap();
    assert!(ev_other.observation_batch_ids.is_empty());
    assert_eq!(ev_other.import_batch_id, None);
}

// 4. A batch observed at/before `evidence.as_of` is visible to a pinned read.
// 5. A newer batch observed after `evidence.as_of` is *excluded* from a
//    pinned read (its fresher price never appears); an unpinned read sees it.
//
// Note: because a superseding ESI refresh garbage-collects the batch it
// replaces (`complete_esi_market_refresh`), a pinned read that outlives the
// prune of its as_of-era batch returns *no price* for that type rather than
// the old one -- a coherent degradation (no node ever prices fresher than
// another), not a silent inconsistency. Within one synchronous graph fold
// nothing prunes, which is the case this pin exists for.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_pinned_read_sees_the_batch_current_at_as_of_and_not_a_later_one(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let industry = PgIndustryRepository::new(pool.clone());

    // Batch T0 at price 4.0.
    complete_esi_batch(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        crate::db_now() - chrono::Duration::hours(2),
        "4.0000",
    )
    .await;

    // Evidence resolved *now* -- as_of freezes here, T0 batch is <= as_of.
    let evidence = industry
        .resolve_market_evidence(workspace_id, jita_scope())
        .await
        .unwrap();

    // (4) the T0 batch is visible to a read pinned to this evidence, and the
    // pinned read is stable across repeated calls.
    for _ in 0..2 {
        let pinned = industry
            .derive_market_price_items(
                workspace_id,
                jita_scope(),
                vec![sell_request(34)],
                Some(&evidence),
            )
            .await
            .unwrap();
        assert_eq!(price(&pinned, 34), Money::parse("4.0000").unwrap());
    }

    // A refresh completes *after* as_of, at price 400.0 (this supersedes and
    // prunes T0).
    complete_esi_batch(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        evidence.as_of + chrono::Duration::minutes(30),
        "400.0000",
    )
    .await;

    // (5) the pinned read never surfaces the newer batch's 400.0 -- the
    // `observed_at <= as_of` cutoff excludes it. (T0 is pruned, so the type
    // is simply absent here.)
    let pinned_after = industry
        .derive_market_price_items(
            workspace_id,
            jita_scope(),
            vec![sell_request(34)],
            Some(&evidence),
        )
        .await
        .unwrap();
    assert!(
        !pinned_after
            .iter()
            .any(|item| item.type_id == 34 && item.price == Money::parse("400.0000").unwrap()),
        "a batch newer than as_of must not reach a pinned read: {pinned_after:?}"
    );

    // (7) an unpinned/live read picks up the newer batch, as before.
    let live = industry
        .derive_market_price_items(workspace_id, jita_scope(), vec![sell_request(34)], None)
        .await
        .unwrap();
    assert_eq!(price(&live, 34), Money::parse("400.0000").unwrap());
}

// 6. Import pricing honors `evidence.import_batch_id`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_pinned_read_honors_the_import_batch_id(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let insmother_scope = MarketScope {
        region_id: 10_000_009,
        location_id: Some(1_049_588_174_021),
    };
    let service = MarketService::new(repository.clone());
    let industry = PgIndustryRepository::new(pool.clone());

    // Import A: best ask 3.97 (unmodified fixture), observed earlier.
    let file_a_observed = crate::db_now() - chrono::Duration::hours(2);
    let mut file_a = upload();
    file_a.filename = "Tritanium-a.txt".to_string();
    file_a.user_observed_at = Some(file_a_observed);
    let batch_a = service
        .import(workspace_id, vec![file_a.clone()])
        .await
        .unwrap()
        .batch
        .unwrap()
        .id;

    // Import B: same rows, best ask dropped to 2.50 (still the lowest),
    // observed later.
    let mut file_b = file_a;
    file_b.filename = "Tritanium-b.txt".to_string();
    file_b.user_observed_at = Some(crate::db_now() - chrono::Duration::minutes(5));
    file_b.content = String::from_utf8(file_b.content)
        .unwrap()
        .replacen("3.97,", "2.50,", 1)
        .into_bytes();
    service.import(workspace_id, vec![file_b]).await.unwrap();

    // Live read -> latest import (2.50).
    let live = industry
        .derive_market_price_items(workspace_id, insmother_scope, vec![sell_request(34)], None)
        .await
        .unwrap();
    assert_eq!(price(&live, 34), Money::parse("2.50").unwrap());

    // Pinned to import batch A -> 3.97, never the newer file.
    let evidence = MarketScopeEvidence {
        scope: insmother_scope,
        observation_batch_ids: vec![],
        import_batch_id: Some(batch_a),
        observed_at: Some(file_a_observed),
        as_of: crate::db_now(),
    };
    let pinned = industry
        .derive_market_price_items(
            workspace_id,
            insmother_scope,
            vec![sell_request(34)],
            Some(&evidence),
        )
        .await
        .unwrap();
    assert_eq!(price(&pinned, 34), Money::parse("3.97").unwrap());
}

// 8. Freshness/staleness classification uses `evidence.as_of`, not `now`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn staleness_is_classified_against_evidence_as_of_not_a_later_now(pool: PgPool) {
    let (workspace_id, repository) = fixture(&pool).await;
    let source_id = esi_source(&pool, workspace_id).await;
    let industry = PgIndustryRepository::new(pool.clone());

    // A batch observed 30 minutes ago -- FRESH relative to a real `now`
    // (fresh window is 1h; stale is 24h).
    let observed_at = crate::db_now() - chrono::Duration::minutes(30);
    complete_esi_batch(
        &repository,
        workspace_id,
        source_id,
        34,
        "Tritanium",
        observed_at,
        "4.0000",
    )
    .await;

    // Live read: classified against `now` -> not stale.
    let live = industry
        .derive_market_price_items(workspace_id, jita_scope(), vec![sell_request(34)], None)
        .await
        .unwrap();
    assert!(!live[0]
        .note
        .contains(iskworks_core::STALE_MARKET_PROVENANCE));

    // Pinned read whose `as_of` is 30h *after* the batch -> stale window
    // (24h) exceeded, so the same batch is classified stale.
    let evidence = MarketScopeEvidence {
        scope: jita_scope(),
        observation_batch_ids: vec![],
        import_batch_id: None,
        observed_at: Some(observed_at),
        as_of: observed_at + chrono::Duration::hours(30),
    };
    let pinned = industry
        .derive_market_price_items(
            workspace_id,
            jita_scope(),
            vec![sell_request(34)],
            Some(&evidence),
        )
        .await
        .unwrap();
    assert!(
        pinned[0]
            .note
            .contains(iskworks_core::STALE_MARKET_PROVENANCE),
        "staleness must be measured from evidence.as_of, got note: {:?}",
        pinned[0].note
    );
}
