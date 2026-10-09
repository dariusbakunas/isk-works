use super::*;

// ─────────────────────────────────────────────────────────────────────────
// Inventory-aware snapshotting applies RECURSIVELY: a Manufacturing/Reaction
// ticket generated for a Build/React Epic requirement freezes its OWN
// prerequisites with the same inventory-aware Missing/Full semantics,
// against the linked Build's own live coverage -- otherwise inventory reuse
// would be ignored one level deeper.
// ─────────────────────────────────────────────────────────────────────────

/// Create the child production ticket for the Epic's Build/React requirement
/// of `component_type_id`; return its JSON body.
async fn create_child_production_ticket(
    fx: &Fixture,
    order_body: &Value,
    component_type_id: i64,
) -> Value {
    let order_id = order_body["id"].as_str().unwrap();
    let req = requirement_of(order_body, component_type_id);
    assert!(
        req["kind"] == "build" || req["kind"] == "react",
        "component {component_type_id} should be a Build/React requirement, got {}",
        req["kind"]
    );
    let req_id = req["id"].as_str().unwrap();
    let (status, ticket) = post_json(
        &fx.app,
        &format!("/api/orders/{order_id}/requirements/{req_id}/tickets"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "child ticket: {ticket}");
    let ticket_id = ticket["id"].as_str().unwrap().to_string();
    // The POST returns a plain `Ticket`; re-read the enriched list view so
    // `prerequisites` / `blockedBy` are present.
    list_tickets(&fx.app)
        .await
        .into_iter()
        .find(|t| t["id"].as_str() == Some(ticket_id.as_str()))
        .unwrap_or_else(|| panic!("child ticket {ticket_id} not in /api/tickets"))
}

fn prereq_of(ticket: &Value, type_id: i64) -> &Value {
    ticket["prerequisites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["typeId"] == type_id)
        .unwrap_or_else(|| panic!("no prerequisite for type {type_id} on {ticket}"))
}

/// Manufacturing child, raw prerequisite fully covered by inventory:
/// frozen `Missing` / `reused == required` / `fresh == 0`, and the child
/// ticket has no unmet blocker for it.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn child_manufacturing_ticket_prerequisite_fully_covered_is_frozen_as_inventory_reuse(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;

    // Parent needs 5 Fabricated Component -> child ticket at 5 runs ->
    // 5*60 = 300 Pyerite, 5*100 = 500 Tritanium. Cover both generously;
    // Pyerite is consumed ONLY by the child, so no root contention there.
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 15_000).await; // avg 3
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "body: {order}");

    let child = create_child_production_ticket(&fx, &order, 90100).await;
    assert_eq!(child["kind"], "manufacturing");
    // An eagerly-generated child ticket's own frozen output is
    // `producedQuantity` (full job output), not an `executionSnapshot`
    // (only the root ticket carries one).
    assert_eq!(child["producedQuantity"], 5, "ceil(5 / 1 per run)");

    let pyerite = prereq_of(&child, 35);
    assert_eq!(pyerite["fulfillmentScope"], "missing");
    assert_eq!(pyerite["reusedQuantity"], 300);
    assert_eq!(pyerite["freshQuantity"], 0);
    money_eq(&pyerite["reusedLineTotal"], "900.0000"); // 300 @ avg 3

    // No unmet blocker for a fully inventory-covered prerequisite.
    let blocked_types: Vec<i64> = child["blockedBy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["typeId"].as_i64().unwrap())
        .collect();
    assert!(
        !blocked_types.contains(&35),
        "a fresh==0 prerequisite is not a blocker (would otherwise drive false downstream work)"
    );
}

/// Manufacturing child, raw prerequisite partially covered:
/// `reused == available`, `fresh == shortage`; downstream work is the
/// shortage only.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn child_manufacturing_ticket_prerequisite_partially_covered_freezes_the_shortage(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;

    // 300 Pyerite needed, only 200 on hand.
    seed_balance(&pool, &fx, 35, "Pyerite", 200, 1_000).await; // avg 5
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let child = create_child_production_ticket(&fx, &order, 90100).await;

    let pyerite = prereq_of(&child, 35);
    assert_eq!(pyerite["fulfillmentScope"], "missing");
    assert_eq!(pyerite["reusedQuantity"], 200);
    assert_eq!(pyerite["freshQuantity"], 100);
    money_eq(&pyerite["reusedLineTotal"], "1000.0000"); // 200 @ avg 5

    let blocked = child["blockedBy"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["typeId"] == 35)
        .expect("the 100-unit shortage is still an unmet blocker");
    assert_eq!(
        blocked["outstandingQuantity"], 100,
        "downstream work is the shortage only, not the full 300"
    );
}

/// A raw prerequisite explicitly `Full`-scoped on the child Build:
/// inventory is ignored, `reused == 0`, `fresh == required`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn child_manufacturing_ticket_prerequisite_full_scope_ignores_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    // Pyerite marked Full on the *child* Build's own planning input.
    let parent = assembly_with_built_component(&fx, 1, 90100, &[35]).await;

    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 25_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let child = create_child_production_ticket(&fx, &order, 90100).await;

    let pyerite = prereq_of(&child, 35);
    assert_eq!(pyerite["fulfillmentScope"], "full");
    assert_eq!(pyerite["reusedQuantity"], 0);
    assert_eq!(pyerite["freshQuantity"], 300);
    assert_eq!(pyerite["reusedLineTotal"], Value::Null);
    // Tritanium on the child has no override -> still default Missing.
    assert_eq!(prereq_of(&child, 34)["fulfillmentScope"], "missing");
}

// --- Live-Build FacilityProfile revision: the Create-Epic freeze boundary ---
//
// A Build references a FacilityProfile by id and always calculates against
// the CURRENT profile. Editing the profile (bumping its `revision`) must not
// block Create Epic and must not make the Build re-save. Create Epic is the
// immutable boundary: the resulting root ticket's `execution_snapshot`
// freezes the exact profile it resolved, and later profile edits never
// touch it.

/// Simulate a FacilityProfile edit: bump `revision`, change a
/// calculation-relevant setting.
async fn edit_facility_profile(
    pool: &PgPool,
    id: Uuid,
    revision: i64,
    material_reduction_percent: &str,
) {
    sqlx::query(
        "UPDATE industry_facility_profiles \
         SET revision = $2, material_reduction_percent = $3::numeric, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(revision)
    .bind(material_reduction_percent)
    .execute(pool)
    .await
    .unwrap();
}

/// Point one Build's shared manufacturing slot at `facility_id`. A live
/// Build references its facility by id only.
async fn stamp_manufacturing_facility(fx: &Fixture, build_id: BuildId, facility_id: Uuid) {
    let build = fx
        .industry
        .get_build(fx.workspace_id, build_id)
        .await
        .unwrap();
    let mut snapshot = build
        .draft_planning
        .clone()
        .expect("build has draft planning");
    snapshot.input.manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: iskworks_core::FacilityProfileId(facility_id),
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });
    fx.industry
        .update_draft(
            fx.workspace_id,
            build_id,
            DraftUpdate {
                expected_revision: build.revision,
                name: build.name.clone(),
                runs: build.runs,
                notes: build.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(snapshot),
            },
        )
        .await
        .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_order_resolves_and_freezes_the_current_facility_after_an_edit(pool: PgPool) {
    let fx = fixture(&pool).await;
    let build = rifter(&fx).await;

    // Facility live at revision 7, 1% material reduction; the Build's slot
    // references it by id.
    let facility_id = Uuid::new_v4();
    insert_facility_profile(&pool, fx.workspace_id, facility_id, 7, "1").await;
    stamp_manufacturing_facility(&fx, build.id, facility_id).await;

    // Edit -> revision 8, 5% reduction. The Build is NOT re-saved.
    edit_facility_profile(&pool, facility_id, 8, "5").await;

    // Create Epic succeeds and freezes the CURRENT (revision 8) profile.
    // The root's facility identity is sourced from the
    // request's own command overlay (the same overlay the whole-tree
    // freeze uses), not re-derived from the persisted Build server-side --
    // so, matching what a real client does, re-fetch the Build (now
    // carrying the stamped facility in its draft) before building the
    // request body.
    let build = fx
        .industry
        .get_build(fx.workspace_id, build.id)
        .await
        .unwrap();
    let (status, _order) = create_order(&fx.app, &build).await;
    assert_eq!(status, StatusCode::CREATED);

    let tickets = list_tickets(&fx.app).await;
    let snap = &root_ticket_for(&tickets, build.id)["executionSnapshot"];
    assert_eq!(
        snap["facility"]["revision"], 8,
        "froze the current revision"
    );
    assert_eq!(
        snap["facility"]["materialReductionPercent"], "5.000000",
        "froze the rev-8 settings"
    );
    assert!(
        snap["installationCost"].is_object(),
        "installation-cost evidence frozen: {snap:?}"
    );

    // A further edit -> revision 9 must NOT mutate the frozen Epic snapshot.
    edit_facility_profile(&pool, facility_id, 9, "9").await;
    let tickets_after = list_tickets(&fx.app).await;
    let snap_after = &root_ticket_for(&tickets_after, build.id)["executionSnapshot"];
    assert_eq!(
        snap_after["facility"]["revision"], 8,
        "Epic stays frozen at revision 8"
    );
    assert_eq!(
        snap_after["facility"]["materialReductionPercent"], "5.000000",
        "frozen settings unchanged by the rev-9 edit"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn linked_production_ticket_freezes_the_current_facility_revision(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    let child_id = BuildId(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT producer_build_id FROM production_dependencies \
             WHERE consumer_build_id = $1 AND component_type_id = 90100",
        )
        .bind(parent.id.0)
        .fetch_one(&pool)
        .await
        .expect("assembly fixture creates the component's producer"),
    );

    // Both Builds reference the facility (revision 4); it is then edited to
    // revision 5 with neither Build re-saved.
    let facility_id = Uuid::new_v4();
    insert_facility_profile(&pool, fx.workspace_id, facility_id, 4, "1").await;
    stamp_manufacturing_facility(&fx, parent.id, facility_id).await;
    stamp_manufacturing_facility(&fx, child_id, facility_id).await;
    edit_facility_profile(&pool, facility_id, 5, "7").await;

    seed_balance(&pool, &fx, 35, "Pyerite", 50_000, 150_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 500_000, 2_500_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    // The linked Manufacturing ticket is eagerly generated by whole-tree
    // Create Epic -- its frozen facility identity lives in
    // `planEvidence.installation`, not a per-ticket
    // `executionSnapshot` (which only the root ticket carries; see
    // `create_order`'s own doc).
    let child = create_child_production_ticket(&fx, &order, 90100).await;
    assert_eq!(child["kind"], "manufacturing");
    assert_eq!(
        child["planEvidence"]["installation"]["facilityProfileRevision"],
        5
    );
}

/// Reaction child: same recursive Missing netting on its formula
/// inputs; the generated ticket is `kind: reaction`.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn child_reaction_ticket_prerequisite_is_inventory_netted(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90200, &[]).await;

    // 5 Reacted Part -> child reaction ticket at 5 runs -> 5*40 = 200 Pyerite.
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 20_000).await; // avg 4
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let child = create_child_production_ticket(&fx, &order, 90200).await;

    assert_eq!(child["kind"], "reaction");
    assert_eq!(child["producedQuantity"], 5);
    let pyerite = prereq_of(&child, 35);
    assert_eq!(pyerite["fulfillmentScope"], "missing");
    assert_eq!(pyerite["reusedQuantity"], 200);
    assert_eq!(pyerite["freshQuantity"], 0);
    money_eq(&pyerite["reusedLineTotal"], "800.0000"); // 200 @ avg 4
}

/// Generating the Epic AND the child production ticket touches no
/// inventory: zero events, zero allocations, balances/revisions unchanged.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recursive_netting_creates_no_inventory_side_effects(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 15_000).await;
    seed_balance(&pool, &fx, 34, "Tritanium", 50_000, 250_000).await;

    let before = inventory_fingerprint(&pool).await;
    let allocations_before = allocation_count(&pool).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let _child = create_child_production_ticket(&fx, &order, 90100).await;

    assert_eq!(
        inventory_fingerprint(&pool).await,
        before,
        "recursive netting posted an event or moved a balance"
    );
    // Only reservations of the frozen reuse -- never a ledger change.
    assert!(allocation_count(&pool).await > allocations_before);
}

/// **Whole-plan shared-inventory regression** -- superseding the earlier
/// `within_one_epic_shared_raw_material_may_be_frozen_twice_without_reserving`
/// (which documented the OLD per-Build independent-netting limitation:
/// root froze 2000 reused, the child froze its own 500 reused
/// *independently*, planning 2500 from only 2200 on hand). The whole-tree
/// freeze draws every boundary from **one** shared `PlanningInventory`
/// pool, so the physical 2200 can never be double-counted: root claims
/// 2000 first (DFS pre-order: a root's own boundaries are allocated before
/// recursing into a child), leaving 200 for the child's own 500-unit need
/// -- 200 reused + 300 fresh, never 500 reused. Asserted by reading the
/// **persisted** `order_requirements` rows back from Postgres, not the
/// live HTTP response alone. `list_order_requirements`/list_order_plan_operations
/// reuse the same query path the running server (`PgOrderRepository`) uses.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn within_one_epic_shared_raw_material_is_allocated_once_across_root_and_child(pool: PgPool) {
    let fx = fixture(&pool).await;
    let parent = assembly_with_built_component(&fx, 1, 90100, &[]).await;

    // Root needs 2000 Tritanium directly; the child (5 runs @ 100/run) needs
    // 500 more. Only 2200 on hand -- less than the 2500 combined.
    seed_balance(&pool, &fx, 34, "Tritanium", 2_200, 6_600).await; // avg 3
    seed_balance(&pool, &fx, 35, "Pyerite", 5_000, 15_000).await;

    let before = inventory_fingerprint(&pool).await;

    let (status, order) = create_order(&fx.app, &parent).await;
    assert_eq!(status, StatusCode::CREATED);
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();

    let order_repository = PgOrderRepository::new(pool.clone());
    let requirements = order_repository
        .list_order_requirements(OrderId(order_id))
        .await
        .unwrap();
    let operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();

    let root_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.is_none())
        .expect("root operation");
    let child_op = operations
        .iter()
        .find(|op| op.parent_occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str()))
        .expect("Fabricated Component operation");

    let root_trit = requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(root_op.occurrence_key.as_str())
        })
        .expect("root Tritanium requirement");
    let child_trit = requirements
        .iter()
        .find(|r| {
            r.type_id == 34
                && r.operation_occurrence_key.as_deref() == Some(child_op.occurrence_key.as_str())
        })
        .expect("child Tritanium requirement");

    // Root claims its full 2000 first (DFS pre-order).
    assert_eq!(root_trit.reused_quantity, 2000);
    assert_eq!(root_trit.fresh_quantity, 0);

    // The child's own 500-unit need draws only what's left (200), never a
    // second independent claim on the same 2200 -- the exact fix for the
    // old per-Build double-counting bug.
    assert_eq!(
        child_trit.reused_quantity, 200,
        "shared stock must not be double-counted: root already claimed 2000 of the 2200 on hand"
    );
    assert_eq!(child_trit.fresh_quantity, 300);
    assert_eq!(root_trit.reused_quantity + child_trit.reused_quantity, 2200);

    // The eagerly-generated child ticket (one ticket per active
    // operation) carries the identical frozen split -- proving
    // `create_ticket_for_requirement`'s duplicate-prevention guard returns
    // *this* ticket rather than re-netting a second one against live
    // inventory (see `create_ticket_for_requirement`'s occurrence-key
    // check in `apps/iskworks-api/src/routes/orders.rs`).
    let child_ticket_row: (i64, i64) = sqlx::query_as(
        "SELECT reused_quantity, fresh_quantity FROM ticket_prerequisites tp \
         JOIN tickets t ON t.id = tp.ticket_id \
         WHERE t.occurrence_key = $1 AND tp.type_id = 34",
    )
    .bind(&child_op.occurrence_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(child_ticket_row, (200, 300));

    // Re-requesting a ticket for the child requirement returns the SAME
    // (already frozen) ticket -- no duplicate, no re-netting against the
    // live 2200 balance.
    let dup = create_child_production_ticket(&fx, &order, 90100).await;
    let dup_prereq = prereq_of(&dup, 34);
    assert_eq!(dup_prereq["reusedQuantity"], 200);
    assert_eq!(dup_prereq["freshQuantity"], 300);
    let ticket_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM tickets WHERE order_id = $1 AND type_id = 90100",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ticket_count, 1,
        "must not create a second production ticket for the same frozen operation"
    );

    // Inventory is byte-identical; the Epic only reserved its frozen
    // reuse -- shared Tritanium reserved once across root and child, never
    // more than is on hand.
    assert_eq!(inventory_fingerprint(&pool).await, before);
    let reserved_tritanium: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
         WHERE type_id = 34 AND released_at IS NULL AND consumed_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let on_hand: i64 =
        sqlx::query_scalar("SELECT quantity FROM inventory_balances WHERE type_id = 34")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        reserved_tritanium <= on_hand,
        "{reserved_tritanium} > {on_hand}"
    );
}
