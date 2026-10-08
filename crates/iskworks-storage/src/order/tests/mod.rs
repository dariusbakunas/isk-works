use super::*;
use crate::{PgIndustryRepository, PgInventoryRepository};
use iskworks_core::order::RecordingState;
use iskworks_core::{
    IndustryRepository, InventoryItemKey, InventoryRepository, PriceSnapshot, ProductionRepository,
};

async fn fixture_workspace(pool: &PgPool) -> (WorkspaceId, OwnerId, Uuid) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let import_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1, 'Order Test', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Order Test', false, $3, $3)",
    )
    .bind(owner_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1, 'test', 'fixture', $2, 'active', true, $3, $3)",
    )
    .bind(import_id)
    .bind(format!("order-test-{}", Uuid::new_v4()))
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (workspace_id, owner_id, import_id)
}

/// A second, independent workspace/owner pair sharing the first one's SDE
/// import -- `sde_imports_one_active_idx` allows only one *active* import
/// globally, so a test needing two workspaces can't call `fixture_workspace`
/// twice.
async fn fixture_second_workspace(pool: &PgPool) -> (WorkspaceId, OwnerId) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1, 'Order Test Second Workspace', $2, $3, $3)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Order Test Second Workspace', false, $3, $3)",
    )
    .bind(owner_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (workspace_id, owner_id)
}

async fn fixture_second_owner(pool: &PgPool, workspace_id: WorkspaceId) -> OwnerId {
    let owner_id = OwnerId(Uuid::new_v4());
    let now = crate::db_now();
    sqlx::query(
        "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Order Test Second Owner', false, $3, $3)",
    )
    .bind(owner_id.0)
    .bind(workspace_id.0)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    owner_id
}

/// Real Purchase/Consumption postings (`verify_active_type`, inside
/// `PgInventoryRepository::post_in_transaction`) reject any `type_id` not
/// present in the active SDE import -- only the ticket/order lifecycle
/// tests that actually complete something need this; plain
/// creation/allocation tests never post a ledger event.
async fn seed_sde_type(pool: &PgPool, import_id: Uuid, type_id: i64, name_en: &str) {
    sqlx::query(
        "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1, $2, $3, true)",
    )
    .bind(import_id)
    .bind(type_id)
    .bind(name_en)
    .execute(pool)
    .await
    .unwrap();
}

/// Minimal valid `builds` row -- just enough to satisfy `orders`/`tickets`'
/// FK, since nothing in the order repository reads a build's actual
/// recipe/materials.
async fn fixture_build(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    import_id: Uuid,
) -> BuildId {
    let build_id = BuildId::new();
    let now = crate::db_now();
    sqlx::query(
        r#"
        INSERT INTO builds (
          id, workspace_id, owner_id, display_name, blueprint_type_id, blueprint_name,
          product_type_id, product_name, product_quantity_per_run, source_sde_dataset_id,
          source_sde_version, recipe_fingerprint, runs, notes, revision, created_at, updated_at,
          recipe_kind, plan_root_build_id
        ) VALUES ($1, $2, $3, 'Test Build', 1000, 'Test Blueprint', 2000, 'Test Product', 1, $4,
                  'test', 'fp', 1, '', 1, $5, $5, 'manufacturing', $1)
        "#,
    )
    .bind(build_id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(import_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    build_id
}

/// `create_order` persists the price snapshot itself (in the same
/// transaction as the order/requirements), so tests just build the
/// in-memory value to pass into `NewOrder` -- no separate DB insert.
fn empty_price_snapshot() -> PriceSnapshot {
    PriceSnapshot {
        id: PriceSnapshotId(Uuid::new_v4()),
        price_source_id: None,
        source_name: "Jita 4-4".to_string(),
        source_revision: 1,
        created_at: crate::db_now(),
        items: Vec::new(),
    }
}

async fn seed_inventory_balance(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
    quantity: i64,
) {
    sqlx::query(
        r#"
        INSERT INTO inventory_balances (
          workspace_id, owner_id, type_id, captured_name, quantity,
          total_historical_cost, revision, last_activity_at
        ) VALUES ($1, $2, $3, 'Tritanium', $4, 0, 1, $5)
        "#,
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(quantity)
    .bind(crate::db_now())
    .execute(pool)
    .await
    .unwrap();
}

async fn fixture_price_source(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    name: &str,
) -> PriceSourceId {
    let id = PriceSourceId(Uuid::new_v4());
    let now = crate::db_now();
    sqlx::query(
        "INSERT INTO price_sources (id, workspace_id, display_name, source_kind, created_at, updated_at) \
         VALUES ($1, $2, $3, 'manual', $4, $4)",
    )
    .bind(id.0)
    .bind(workspace_id.0)
    .bind(name)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A standalone Acquisition ticket, created directly (not through an
/// Order/requirement) -- the shape Acquisition Run batching tests need:
/// `kind: Acquisition`, no prerequisites, a real batching-compatibility
/// key. `market_scope` and `price_source_id` are independently settable
/// (rather than derived from one another) so tests can exercise every
/// `BatchKey` combination -- scope-only, manual-only, both, or neither.
fn new_acquisition_ticket(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
    captured_name: &str,
    quantity: u64,
    market_scope: Option<MarketScope>,
    price_source_id: Option<PriceSourceId>,
) -> NewTicket {
    NewTicket {
        order_id: None,
        notes: String::new(),
        assignee_character_id: None,
        id: TicketId::new(),
        workspace_id,
        owner_id,
        kind: TicketKind::Acquisition,
        type_id: Some(type_id),
        captured_name: captured_name.to_string(),
        quantity: Some(quantity),
        source_build_id: None,
        estimated_unit_cost: Some(Money::parse("10").unwrap()),
        estimated_line_total: None,
        market_region_id: market_scope.map(|scope| scope.region_id),
        market_location_id: market_scope.and_then(|scope| scope.location_id),
        price_source_id,
        execution_snapshot: None,
        prerequisites: Vec::new(),
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    }
}

fn draft_order(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    source_build_id: BuildId,
    price_snapshot_id: PriceSnapshotId,
) -> Order {
    let now = crate::db_now();
    Order {
        id: OrderId::new(),
        workspace_id,
        owner_id,
        source_build_id: Some(source_build_id),
        source_build_revision: 1,
        display_name: "Manufacture Ishtar".to_string(),
        runs: 1,
        recipe_fingerprint: "fp".to_string(),
        price_snapshot_id,
        estimated_material_cost: Money::zero(),
        expected_revenue: None,
        estimated_margin: None,
        missing_price_count: 0,
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        planning_snapshot_version: 1,
    }
}

/// A `Full`-scoped (inventory-unaware) requirement -- the neutral shape for
/// tests that only care about persistence / lifecycle / no-side-effect.
fn requirement(
    type_id: i64,
    kind: RequirementKind,
    required_quantity: u64,
) -> iskworks_core::order::NewOrderRequirement {
    requirement_reusing(type_id, kind, required_quantity, 0)
}

/// A `Missing`-scoped requirement carrying a frozen Model-B inventory-reuse
/// intent (`reused_quantity` of `required_quantity` -> `InventorySatisfied`).
fn requirement_reusing(
    type_id: i64,
    kind: RequirementKind,
    required_quantity: u64,
    reused_quantity: u64,
) -> iskworks_core::order::NewOrderRequirement {
    iskworks_core::order::NewOrderRequirement {
        id: OrderRequirementId::new(),
        type_id,
        captured_name: "Tritanium".to_string(),
        kind,
        source_build_id: None,
        required_quantity,
        fulfillment_scope: if reused_quantity == 0 {
            FulfillmentScope::Full
        } else {
            FulfillmentScope::Missing
        },
        reused_quantity,
        estimated_unit_cost: None,
        estimated_line_total: None,
        reused_line_total: None,
        operation_occurrence_key: None,
        child_occurrence_key: None,
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: None,
        price_evidence: None,
    }
}

mod acquisition_costs;
mod acquisition_runs;
mod deletion;
mod epic_membership;
mod generic_tickets;
mod membership_migration;
mod order_lifecycle;
mod orders;
mod record_acquisition;
mod record_production;
mod recording_reversal;
mod requirement_ticket_creation;
mod reserved_quantity;
mod set_ticket_status;
mod status_todo_migration;
mod ticket_inventory_neutrality;
mod ticket_lifecycle;

// ---------------------------------------------------------------------------
// Order and requirement helpers
// ---------------------------------------------------------------------------

/// A `Full`-scoped (inventory-unaware) prerequisite -- the neutral shape,
/// matching how every child ticket's prerequisites are frozen today.
fn ticket_prerequisite(
    type_id: i64,
    required_quantity: u64,
) -> iskworks_core::order::NewTicketPrerequisite {
    ticket_prerequisite_reusing(type_id, required_quantity, 0)
}

/// A `Missing`-scoped prerequisite carrying a frozen inventory-reuse intent
/// (as the root Epic ticket's prerequisites do).
fn ticket_prerequisite_reusing(
    type_id: i64,
    required_quantity: u64,
    reused_quantity: u64,
) -> iskworks_core::order::NewTicketPrerequisite {
    iskworks_core::order::NewTicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        type_id,
        captured_name: "Isogen".to_string(),
        kind: RequirementKind::Buy,
        source_build_id: None,
        required_quantity,
        fulfillment_scope: if reused_quantity == 0 {
            FulfillmentScope::Full
        } else {
            FulfillmentScope::Missing
        },
        reused_quantity,
        estimated_unit_cost: None,
        estimated_line_total: None,
        reused_line_total: None,
        operation_occurrence_key: None,
        child_occurrence_key: None,
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: None,
        price_evidence: None,
    }
}

// ---------------------------------------------------------------------------
// Inventory snapshot helpers
// ---------------------------------------------------------------------------

async fn allocation_snapshot(pool: &PgPool) -> Vec<AllocationSnapshotRow> {
    sqlx::query_as(
        "SELECT id, quantity, released_at, consumed_at \
         FROM inventory_allocations ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// `(quantity, total_historical_cost, revision)` for one balance row, for
/// byte-identical before/after comparison. `None` when no row exists.
async fn inventory_balance(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
) -> Option<(i64, Decimal, i64)> {
    sqlx::query_as(
        "SELECT quantity, total_historical_cost, revision FROM inventory_balances \
         WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn inventory_event_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Snapshot of every `inventory_allocations` row's identity, quantity, and
/// lifecycle timestamps, ordered stably, for byte-identical before/after
/// comparison. A released or consumed row would change `released_at` /
/// `consumed_at`; a new reservation would change the row count.
type AllocationSnapshotRow = (Uuid, i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>);

// ---------------------------------------------------------------------------
// Acquisition recording helpers
// ---------------------------------------------------------------------------

fn acquisition_input(quantity: u64, unit_cost: Option<&str>) -> RecordAcquisitionInput {
    RecordAcquisitionInput {
        idempotency_key: Uuid::new_v4(),
        quantity,
        unit_cost: unit_cost.map(|value| Money::parse(value).unwrap()),
        location_note: String::new(),
        note: String::new(),
        effective_at: crate::db_now(),
    }
}

/// A standalone Acquisition ticket with **no** `estimated_unit_cost` -- for
/// the cost-hierarchy steps below step 1.
fn new_acquisition_ticket_no_estimate(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
    captured_name: &str,
    quantity: u64,
) -> NewTicket {
    NewTicket {
        order_id: None,
        notes: String::new(),
        assignee_character_id: None,
        id: TicketId::new(),
        workspace_id,
        owner_id,
        kind: TicketKind::Acquisition,
        type_id: Some(type_id),
        captured_name: captured_name.to_string(),
        quantity: Some(quantity),
        source_build_id: None,
        estimated_unit_cost: None,
        estimated_line_total: None,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        execution_snapshot: None,
        prerequisites: Vec::new(),
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    }
}

async fn purchase_events(pool: &PgPool) -> Vec<PurchaseEventRow> {
    sqlx::query_as(
        "SELECT quantity_delta, total_cost_delta, cost_quality, \
         ticket_inventory_recording_id, source_reference, resulting_average_cost \
         FROM inventory_events WHERE event_kind = 'purchase' \
         ORDER BY workspace_id, owner_id, type_id, sequence",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn recording_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM ticket_inventory_recordings")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every `purchase` event, oldest first:
/// `(quantity_delta, total_cost_delta, cost_quality, ticket_inventory_recording_id, source_reference, resulting_average_cost)`.
type PurchaseEventRow = (i64, Decimal, String, Option<Uuid>, String, Option<Decimal>);

// ---------------------------------------------------------------------------
// Production recording helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn mfg_ticket(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    kind: TicketKind,
    product_type_id: i64,
    product_name: &str,
    build_id: BuildId,
    prerequisites: Vec<iskworks_core::order::NewTicketPrerequisite>,
    planned_runs: Option<u64>,
) -> NewTicket {
    NewTicket {
        order_id: None,
        notes: String::new(),
        assignee_character_id: None,
        id: TicketId::new(),
        workspace_id,
        owner_id,
        kind,
        type_id: Some(product_type_id),
        captured_name: product_name.to_string(),
        quantity: Some(10),
        source_build_id: Some(build_id),
        estimated_unit_cost: None,
        estimated_line_total: None,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        execution_snapshot: planned_runs.map(|runs| iskworks_core::TaskExecutionSnapshot {
            runs,
            blueprint: None,
            facility: None,
            duration_seconds: None,
            installation_cost: None,
            material_value: None,
        }),
        prerequisites,
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    }
}

fn prod_prereq(type_id: i64, captured_name: &str) -> iskworks_core::order::NewTicketPrerequisite {
    iskworks_core::order::NewTicketPrerequisite {
        id: TicketPrerequisiteId::new(),
        type_id,
        captured_name: captured_name.to_string(),
        kind: RequirementKind::Buy,
        source_build_id: None,
        required_quantity: 1_000_000,
        fulfillment_scope: FulfillmentScope::Full,
        reused_quantity: 0,
        estimated_unit_cost: None,
        estimated_line_total: None,
        reused_line_total: None,
        operation_occurrence_key: None,
        child_occurrence_key: None,
        inventory_unit_basis: None,
        child_produced_quantity: None,
        child_consumed_quantity: None,
        child_surplus_quantity: None,
        child_surplus_retained_basis: None,
        child_consumed_cost: None,
        dependency_id: None,
        price_evidence: None,
    }
}

fn production_input(
    runs_completed: u64,
    output_type_id: i64,
    output_quantity: u64,
    inputs: &[(i64, u64)],
    installation_cost: &str,
) -> RecordProductionInput {
    RecordProductionInput {
        idempotency_key: Uuid::new_v4(),
        runs_completed,
        output_type_id,
        output_quantity,
        inputs: inputs
            .iter()
            .map(
                |(type_id, quantity)| iskworks_core::order::RecordProductionInputLine {
                    type_id: *type_id,
                    quantity: *quantity,
                },
            )
            .collect(),
        installation_cost: Money::parse(installation_cost).unwrap(),
        location_note: String::new(),
        note: String::new(),
        effective_at: crate::db_now(),
    }
}

async fn seed_inventory_balance_with_cost(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    type_id: i64,
    captured_name: &str,
    quantity: i64,
    total_historical_cost: &str,
) {
    sqlx::query(
        "INSERT INTO inventory_balances \
           (workspace_id, owner_id, type_id, captured_name, quantity, \
            total_historical_cost, revision, last_activity_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 1, $7)",
    )
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(type_id)
    .bind(captured_name)
    .bind(quantity)
    .bind(total_historical_cost.parse::<Decimal>().unwrap())
    .bind(crate::db_now())
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Generic ticket helpers
// ---------------------------------------------------------------------------

fn generic_ticket(workspace_id: WorkspaceId, owner_id: OwnerId, captured_name: &str) -> NewTicket {
    NewTicket {
        id: TicketId::new(),
        workspace_id,
        owner_id,
        order_id: None,
        kind: TicketKind::Generic,
        type_id: None,
        captured_name: captured_name.to_string(),
        quantity: None,
        source_build_id: None,
        estimated_unit_cost: None,
        estimated_line_total: None,
        market_region_id: None,
        market_location_id: None,
        price_source_id: None,
        notes: String::new(),
        assignee_character_id: None,
        execution_snapshot: None,
        prerequisites: Vec::new(),
        occurrence_key: None,
        parent_ticket_id: None,
        produced_quantity: None,
        material_component_cost: None,
        own_installation_cost: None,
        total_production_cost: None,
        plan_evidence: None,
    }
}
