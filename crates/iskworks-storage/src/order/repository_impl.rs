use super::*;

use super::inserts::*;
use super::inventory_posting::*;
use super::recording_queries::*;

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn verify_ticket_references(
        &self,
        workspace_id: WorkspaceId,
        order_id: Option<OrderId>,
        assignee_character_id: Option<ConnectedCharacterId>,
        price_source_id: Option<PriceSourceId>,
    ) -> Result<(), OrderError> {
        let all_owned: bool = sqlx::query_scalar(
            r#"
            SELECT ($2::uuid IS NULL
                    OR EXISTS (SELECT 1 FROM orders WHERE id = $2 AND workspace_id = $1))
               AND ($3::uuid IS NULL
                    OR EXISTS (SELECT 1 FROM eve_connections WHERE id = $3 AND workspace_id = $1))
               AND ($4::uuid IS NULL
                    OR EXISTS (SELECT 1 FROM price_sources WHERE id = $4 AND workspace_id = $1))
            "#,
        )
        .bind(workspace_id.0)
        .bind(order_id.map(|id| id.0))
        .bind(assignee_character_id.map(|id| id.0))
        .bind(price_source_id.map(|id| id.0))
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        if all_owned {
            Ok(())
        } else {
            Err(OrderError::TicketReferenceNotFound)
        }
    }

    async fn create_order(
        &self,
        new_order: NewOrder,
    ) -> Result<(Order, Vec<OrderRequirement>), OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let order = new_order.order;
        let snapshot = new_order.price_snapshot;
        let source_build_id = order.source_build_id.ok_or_else(|| {
            OrderError::Persistence("new Order requires a source Build".to_string())
        })?;

        sqlx::query(
            r#"
            INSERT INTO price_snapshots (
              id, workspace_id, build_id, price_source_id, captured_source_name,
              captured_source_revision, purpose, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, 'plan_generation', $7)
            "#,
        )
        .bind(snapshot.id.0)
        .bind(order.workspace_id.0)
        .bind(source_build_id.0)
        .bind(snapshot.price_source_id.map(|id| id.0))
        .bind(&snapshot.source_name)
        .bind(i64_from_u64(snapshot.source_revision)?)
        .bind(snapshot.created_at)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        for item in &snapshot.items {
            sqlx::query(
                r#"
                INSERT INTO price_snapshot_items (
                  price_snapshot_id, type_id, captured_name, item_role, selection_kind,
                  manual_unit_price, price, pricing_policy, missing, source_note, sort_order,
                  market_region_id, market_location_id
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(snapshot.id.0)
            .bind(item.type_id)
            .bind(&item.type_name)
            .bind(planner_item_role_str(item.item_role))
            .bind(pricing_selection_kind_str(item.selection_kind))
            .bind(item.manual_unit_price.map(|money| money.0))
            .bind(item.price.map(|money| money.0))
            .bind(item.pricing_policy.map(pricing_policy_str))
            .bind(item.missing)
            .bind(&item.source_note)
            .bind(
                i32::try_from(item.sort_order)
                    .map_err(|_| OrderError::Persistence("invalid sort order".to_string()))?,
            )
            .bind(item.market_region_id)
            .bind(item.market_location_id)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        sqlx::query(
            r#"
            INSERT INTO orders (
              id, workspace_id, owner_id, source_build_id, source_build_revision,
              display_name, runs, recipe_fingerprint, price_snapshot_id,
              estimated_material_cost, expected_revenue, estimated_margin,
              missing_price_count, created_at, updated_at, started_at, completed_at,
              canceled_at, archived_at, planning_snapshot_version
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            "#,
        )
        .bind(order.id.0)
        .bind(order.workspace_id.0)
        .bind(order.owner_id.0)
        .bind(source_build_id.0)
        .bind(i64_from_u64(order.source_build_revision)?)
        .bind(&order.display_name)
        .bind(i64_from_u64(order.runs)?)
        .bind(&order.recipe_fingerprint)
        .bind(order.price_snapshot_id.0)
        .bind(order.estimated_material_cost.0)
        .bind(order.expected_revenue.map(|money| money.0))
        .bind(order.estimated_margin.map(|money| money.0))
        .bind(
            i32::try_from(order.missing_price_count)
                .map_err(|_| OrderError::Persistence("invalid missing count".to_string()))?,
        )
        .bind(order.created_at)
        .bind(order.updated_at)
        .bind(order.started_at)
        .bind(order.completed_at)
        .bind(order.canceled_at)
        .bind(order.archived_at)
        .bind(i16::from(order.planning_snapshot_version))
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        let mut requirements = Vec::with_capacity(new_order.requirements.len());
        for requirement in new_order.requirements {
            // Persist the caller's frozen inventory-reuse snapshot
            // (`fulfillment_scope` / `reused_quantity` / `reused_line_total`)
            // as-is. `fresh_quantity` is derived, and the
            // `reused + fresh = required` CHECK guards consistency. This is
            // inventory-neutral -- no `inventory_allocations` /
            // `inventory_events` / balance write anywhere in this method.
            requirements.push(insert_order_requirement(&mut tx, order.id, requirement).await?);
        }

        tx.commit().await.map_err(map_error)?;
        Ok((order, requirements))
    }

    async fn get_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        sqlx::query_as::<_, OrderRow>(
            "SELECT id, workspace_id, owner_id, source_build_id, source_build_revision, \
             display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, \
             expected_revenue, estimated_margin, missing_price_count, created_at, updated_at, \
             started_at, completed_at, canceled_at, archived_at, planning_snapshot_version \
             FROM orders WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(order_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(OrderError::OrderNotFound)?
        .into_order()
    }

    async fn list_orders(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<Order>, OrderError> {
        sqlx::query_as::<_, OrderRow>(
            "SELECT id, workspace_id, owner_id, source_build_id, source_build_revision, \
             display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, \
             expected_revenue, estimated_margin, missing_price_count, created_at, updated_at, \
             started_at, completed_at, canceled_at, archived_at, planning_snapshot_version \
             FROM orders WHERE workspace_id = $1 AND owner_id = $2 ORDER BY updated_at DESC",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(OrderRow::into_order)
        .collect()
    }

    async fn list_order_requirements(
        &self,
        order_id: OrderId,
    ) -> Result<Vec<OrderRequirement>, OrderError> {
        sqlx::query_as::<_, OrderRequirementRow>(
            "SELECT id, order_id, type_id, captured_name, kind, source_build_id, \
             required_quantity, fulfillment_scope, reused_quantity, fresh_quantity, \
             estimated_unit_cost, estimated_line_total, reused_line_total, \
             operation_occurrence_key, child_occurrence_key, inventory_unit_basis, \
             child_produced_quantity, child_consumed_quantity, child_surplus_quantity, \
             child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id \
             FROM order_requirements WHERE order_id = $1 ORDER BY type_id",
        )
        .bind(order_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(OrderRequirementRow::into_requirement)
        .collect()
    }

    async fn list_order_plan_operations(
        &self,
        order_id: OrderId,
    ) -> Result<Vec<PlanOperation>, OrderError> {
        sqlx::query_as::<_, PlanOperationRow>(
            "SELECT id, order_id, occurrence_key, parent_occurrence_key, build_id, activity, \
             runs, persisted_runs, product_type_id, product_name, output_per_run, \
             produced_quantity, blueprint_or_formula_type_id, material_component_cost, \
             own_installation_cost, total_production_cost, complete, evidence, created_at, \
             consumed_quantity, surplus_quantity, surplus_retained_basis \
             FROM order_plan_operations WHERE order_id = $1 ORDER BY created_at, occurrence_key",
        )
        .bind(order_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(PlanOperationRow::into_operation)
        .collect()
    }

    async fn get_price_source_for_snapshot(
        &self,
        price_snapshot_id: PriceSnapshotId,
    ) -> Result<Option<PriceSourceId>, OrderError> {
        let price_source_id: Option<Uuid> =
            sqlx::query_scalar("SELECT price_source_id FROM price_snapshots WHERE id = $1")
                .bind(price_snapshot_id.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_error)?
                .flatten();
        Ok(price_source_id.map(PriceSourceId))
    }

    async fn get_material_scope_for_snapshot(
        &self,
        price_snapshot_id: PriceSnapshotId,
    ) -> Result<Option<MarketScope>, OrderError> {
        let row: Option<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT market_region_id, market_location_id FROM price_snapshot_items \
             WHERE price_snapshot_id = $1 AND item_role = 'material' \
               AND market_region_id IS NOT NULL \
             LIMIT 1",
        )
        .bind(price_snapshot_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(row.map(|(region_id, location_id)| MarketScope {
            region_id,
            location_id,
        }))
    }

    async fn create_ticket(
        &self,
        new_ticket: NewTicket,
    ) -> Result<(Ticket, Vec<TicketPrerequisite>), OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let created =
            insert_ticket_with_prerequisites(&mut tx, new_ticket, crate::db_now()).await?;
        tx.commit().await.map_err(map_error)?;
        Ok(created)
    }

    async fn create_order_plan(
        &self,
        new_plan: NewOrderPlan,
    ) -> Result<OrderPlanResult, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let order = new_plan.order;
        let snapshot = new_plan.price_snapshot;
        let now = crate::db_now();
        let source_build_id = order.source_build_id.ok_or_else(|| {
            OrderError::Persistence("new Order plan requires a source Build".to_string())
        })?;

        // Reserve before writing anything: lock the reused types' balances,
        // measure free stock under the lock, and fail the whole plan on a
        // shortfall. The allocations themselves are inserted once their
        // requirements exist.
        let reservations = match &new_plan.reservation {
            Some(reservation) => {
                let free = super::reservations::lock_free_stock(
                    &mut tx,
                    order.workspace_id,
                    order.owner_id,
                    reuse_by_type(&new_plan.requirements).into_keys(),
                )
                .await?;
                plan_epic_reservations(&new_plan.requirements, &reservation.stages, &free)
                    .map_err(OrderError::ReservationShortfall)?
            }
            None => Vec::new(),
        };

        sqlx::query(
            r#"
            INSERT INTO price_snapshots (
              id, workspace_id, build_id, price_source_id, captured_source_name,
              captured_source_revision, purpose, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, 'plan_generation', $7)
            "#,
        )
        .bind(snapshot.id.0)
        .bind(order.workspace_id.0)
        .bind(source_build_id.0)
        .bind(snapshot.price_source_id.map(|id| id.0))
        .bind(&snapshot.source_name)
        .bind(i64_from_u64(snapshot.source_revision)?)
        .bind(snapshot.created_at)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        for item in &snapshot.items {
            sqlx::query(
                r#"
                INSERT INTO price_snapshot_items (
                  price_snapshot_id, type_id, captured_name, item_role, selection_kind,
                  manual_unit_price, price, pricing_policy, missing, source_note, sort_order,
                  market_region_id, market_location_id
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(snapshot.id.0)
            .bind(item.type_id)
            .bind(&item.type_name)
            .bind(planner_item_role_str(item.item_role))
            .bind(pricing_selection_kind_str(item.selection_kind))
            .bind(item.manual_unit_price.map(|money| money.0))
            .bind(item.price.map(|money| money.0))
            .bind(item.pricing_policy.map(pricing_policy_str))
            .bind(item.missing)
            .bind(&item.source_note)
            .bind(
                i32::try_from(item.sort_order)
                    .map_err(|_| OrderError::Persistence("invalid sort order".to_string()))?,
            )
            .bind(item.market_region_id)
            .bind(item.market_location_id)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        sqlx::query(
            r#"
            INSERT INTO orders (
              id, workspace_id, owner_id, source_build_id, source_build_revision,
              display_name, runs, recipe_fingerprint, price_snapshot_id,
              estimated_material_cost, expected_revenue, estimated_margin,
              missing_price_count, created_at, updated_at, started_at, completed_at,
              canceled_at, archived_at, planning_snapshot_version
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            "#,
        )
        .bind(order.id.0)
        .bind(order.workspace_id.0)
        .bind(order.owner_id.0)
        .bind(source_build_id.0)
        .bind(i64_from_u64(order.source_build_revision)?)
        .bind(&order.display_name)
        .bind(i64_from_u64(order.runs)?)
        .bind(&order.recipe_fingerprint)
        .bind(order.price_snapshot_id.0)
        .bind(order.estimated_material_cost.0)
        .bind(order.expected_revenue.map(|money| money.0))
        .bind(order.estimated_margin.map(|money| money.0))
        .bind(
            i32::try_from(order.missing_price_count)
                .map_err(|_| OrderError::Persistence("invalid missing count".to_string()))?,
        )
        .bind(order.created_at)
        .bind(order.updated_at)
        .bind(order.started_at)
        .bind(order.completed_at)
        .bind(order.canceled_at)
        .bind(order.archived_at)
        .bind(i16::from(order.planning_snapshot_version))
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        // Whole-tree freeze: operations first (every requirement below
        // references one by `operation_occurrence_key`), then requirements
        // at every depth, then tickets (root + every active child
        // operation), resolving each ticket's `parent_ticket_id` from its
        // `parent_occurrence_key` against tickets already inserted earlier
        // in this same loop -- the caller is required to order
        // `new_plan.tickets` parent-before-child (the root first) so every
        // parent lookup below always hits.
        let mut operations = Vec::with_capacity(new_plan.operations.len());
        for operation in new_plan.operations {
            operations.push(insert_plan_operation(&mut tx, order.id, operation, now).await?);
        }

        let mut requirements = Vec::with_capacity(new_plan.requirements.len());
        for requirement in new_plan.requirements {
            requirements.push(insert_order_requirement(&mut tx, order.id, requirement).await?);
        }
        super::reservations::insert_requirement_allocations(
            &mut tx,
            order.workspace_id,
            order.owner_id,
            AllocationReason::EpicCreate,
            &reservations,
            now,
        )
        .await?;

        let mut tickets = Vec::with_capacity(new_plan.tickets.len());
        let mut ticket_id_by_occurrence: HashMap<String, TicketId> = HashMap::new();
        for planned in new_plan.tickets {
            let mut new_ticket = planned.ticket;
            if let Some(parent_occurrence_key) = planned.parent_occurrence_key {
                let parent_ticket_id = ticket_id_by_occurrence
                    .get(&parent_occurrence_key)
                    .copied()
                    .ok_or_else(|| {
                        OrderError::Persistence(format!(
                            "plan ticket references unresolved parent occurrence {parent_occurrence_key}"
                        ))
                    })?;
                new_ticket.parent_ticket_id = Some(parent_ticket_id);
            }
            if let Some(occurrence_key) = new_ticket.occurrence_key.clone() {
                ticket_id_by_occurrence.insert(occurrence_key, new_ticket.id);
            }

            let (display_id, _execution_snapshot, _plan_evidence) =
                insert_ticket_row(&mut tx, &new_ticket, now).await?;

            // `OrderPlanResult` returns only the ticket shells (matching
            // `OrderPlanResult::tickets: Vec<Ticket>`); a caller that needs
            // each ticket's own prerequisites reads them back via
            // `list_ticket_prerequisites`, same as every other ticket path.
            for prerequisite in std::mem::take(&mut new_ticket.prerequisites) {
                insert_ticket_prerequisite(&mut tx, new_ticket.id, prerequisite).await?;
            }

            tickets.push(Ticket {
                id: new_ticket.id,
                workspace_id: new_ticket.workspace_id,
                owner_id: new_ticket.owner_id,
                display_id,
                kind: new_ticket.kind,
                type_id: new_ticket.type_id,
                captured_name: new_ticket.captured_name,
                quantity: new_ticket.quantity,
                order_id: new_ticket.order_id,
                source_build_id: new_ticket.source_build_id,
                notes: new_ticket.notes,
                assignee_character_id: new_ticket.assignee_character_id,
                status: TicketStatus::Todo,
                estimated_unit_cost: new_ticket.estimated_unit_cost,
                estimated_line_total: new_ticket.estimated_line_total,
                actual_unit_cost: None,
                actual_line_total: None,
                market_region_id: new_ticket.market_region_id,
                market_location_id: new_ticket.market_location_id,
                price_source_id: new_ticket.price_source_id,
                acquisition_run_id: None,
                acquired_quantity: None,
                execution_snapshot: new_ticket.execution_snapshot,
                created_at: now,
                updated_at: now,
                archived_at: None,
                occurrence_key: new_ticket.occurrence_key,
                parent_ticket_id: new_ticket.parent_ticket_id,
                produced_quantity: new_ticket.produced_quantity,
                material_component_cost: new_ticket.material_component_cost,
                own_installation_cost: new_ticket.own_installation_cost,
                total_production_cost: new_ticket.total_production_cost,
                plan_evidence: new_ticket.plan_evidence,
            });
        }

        tx.commit().await.map_err(map_error)?;

        Ok(OrderPlanResult {
            order,
            requirements,
            operations,
            tickets,
            reservations,
        })
    }

    async fn get_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence \
             FROM tickets WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(OrderError::TicketNotFound)?
        .into_ticket()
    }

    async fn list_tickets(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<Ticket>, OrderError> {
        sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence \
             FROM tickets WHERE workspace_id = $1 AND owner_id = $2 ORDER BY display_id",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect()
    }

    async fn list_tickets_for_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Vec<Ticket>, OrderError> {
        sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence \
             FROM tickets WHERE workspace_id = $1 AND order_id = $2 ORDER BY display_id",
        )
        .bind(workspace_id.0)
        .bind(order_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect()
    }

    async fn list_ticket_prerequisites(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketPrerequisite>, OrderError> {
        sqlx::query_as::<_, TicketPrerequisiteRow>(
            "SELECT id, ticket_id, type_id, captured_name, kind, source_build_id, \
             required_quantity, fulfillment_scope, reused_quantity, fresh_quantity, \
             estimated_unit_cost, estimated_line_total, reused_line_total, \
             operation_occurrence_key, child_occurrence_key, inventory_unit_basis, \
             child_produced_quantity, child_consumed_quantity, child_surplus_quantity, \
             child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id \
             FROM ticket_prerequisites WHERE ticket_id = $1 ORDER BY type_id",
        )
        .bind(ticket_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketPrerequisiteRow::into_prerequisite)
        .collect()
    }

    async fn link_order_requirement_to_ticket(
        &self,
        order_requirement_id: OrderRequirementId,
        ticket_id: TicketId,
        allocated_quantity: u64,
    ) -> Result<OrderRequirementFulfillment, OrderError> {
        if allocated_quantity == 0 {
            return Err(OrderError::InvalidQuantity);
        }
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        let existing = sqlx::query_as::<_, OrderRequirementFulfillmentRow>(
            r#"
            SELECT f.id, f.order_requirement_id, f.ticket_id, f.allocated_quantity, f.linked_at
            FROM order_requirement_fulfillments f
            JOIN tickets t ON t.id = f.ticket_id
            WHERE f.order_requirement_id = $1 AND f.ticket_id = $2 AND t.status <> 'canceled'
            "#,
        )
        .bind(order_requirement_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        if let Some(row) = existing {
            return row.into_fulfillment();
        }

        let requirement_type_id: Option<i64> =
            sqlx::query_scalar("SELECT type_id FROM order_requirements WHERE id = $1")
                .bind(order_requirement_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let requirement_type_id =
            requirement_type_id.ok_or(OrderError::OrderRequirementNotFound)?;
        let ticket_type_id: Option<i64> =
            sqlx::query_scalar("SELECT type_id FROM tickets WHERE id = $1")
                .bind(ticket_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let ticket_type_id = ticket_type_id.ok_or(OrderError::TicketNotFound)?;
        if requirement_type_id != ticket_type_id {
            return Err(OrderError::TicketTypeMismatch);
        }

        let id = Uuid::new_v4();
        let linked_at = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO order_requirement_fulfillments (
              id, order_requirement_id, ticket_id, allocated_quantity, linked_at
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(order_requirement_id.0)
        .bind(ticket_id.0)
        .bind(i64_from_u64(allocated_quantity)?)
        .bind(linked_at)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        tx.commit().await.map_err(map_error)?;
        Ok(OrderRequirementFulfillment {
            id: OrderRequirementFulfillmentId(id),
            order_requirement_id,
            ticket_id,
            allocated_quantity,
            linked_at,
        })
    }

    async fn create_ticket_for_order_requirement(
        &self,
        order_requirement_id: OrderRequirementId,
        new_ticket: NewTicket,
        allocated_quantity: u64,
    ) -> Result<RequirementTicketCreation, OrderError> {
        if allocated_quantity == 0 {
            return Err(OrderError::InvalidQuantity);
        }
        let allocated_quantity_db = i64_from_u64(allocated_quantity)?;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = crate::db_now();

        let requirement_type_id: Option<i64> =
            sqlx::query_scalar("SELECT type_id FROM order_requirements WHERE id = $1")
                .bind(order_requirement_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let requirement_type_id =
            requirement_type_id.ok_or(OrderError::OrderRequirementNotFound)?;
        if new_ticket.type_id != Some(requirement_type_id) {
            return Err(OrderError::TicketTypeMismatch);
        }

        // Claim the requirement first. The primary key is the uniqueness
        // guarantee: a concurrent claimer blocks here until this
        // transaction ends, then sees the committed claim (or none, if we
        // rolled back). `ticket_id`'s FK is deferred to commit, so the
        // ticket row is only inserted once the claim is ours.
        let claimed = sqlx::query(
            r#"
            INSERT INTO order_requirement_ticket_claims (order_requirement_id, ticket_id, claimed_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (order_requirement_id) DO NOTHING
            "#,
        )
        .bind(order_requirement_id.0)
        .bind(new_ticket.id.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
        .rows_affected()
            == 1;
        if !claimed {
            // Already claimed. Lock the claim (serializing with anyone else
            // re-pointing it) and look at its ticket: an active one wins;
            // a canceled one frees the requirement for this new ticket.
            let (holder, holder_status): (Uuid, String) = sqlx::query_as(
                r#"
                SELECT c.ticket_id, t.status
                FROM order_requirement_ticket_claims c
                JOIN tickets t ON t.id = c.ticket_id
                WHERE c.order_requirement_id = $1
                FOR UPDATE OF c
                "#,
            )
            .bind(order_requirement_id.0)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
            if ticket_status_from_str(&holder_status)? != TicketStatus::Canceled {
                return Ok(RequirementTicketCreation::AlreadyLinked(TicketId(holder)));
            }
            sqlx::query(
                "UPDATE order_requirement_ticket_claims SET ticket_id = $2, claimed_at = $3 \
                 WHERE order_requirement_id = $1",
            )
            .bind(order_requirement_id.0)
            .bind(new_ticket.id.0)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        let (ticket, _) = insert_ticket_with_prerequisites(&mut tx, new_ticket, now).await?;
        sqlx::query(
            r#"
            INSERT INTO order_requirement_fulfillments (
              id, order_requirement_id, ticket_id, allocated_quantity, linked_at
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(order_requirement_id.0)
        .bind(ticket.id.0)
        .bind(allocated_quantity_db)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        tx.commit().await.map_err(map_error)?;
        Ok(RequirementTicketCreation::Created(Box::new(ticket)))
    }

    async fn list_order_requirement_fulfillments(
        &self,
        order_requirement_id: OrderRequirementId,
    ) -> Result<Vec<OrderRequirementFulfillment>, OrderError> {
        sqlx::query_as::<_, OrderRequirementFulfillmentRow>(
            "SELECT id, order_requirement_id, ticket_id, allocated_quantity, linked_at \
             FROM order_requirement_fulfillments WHERE order_requirement_id = $1 ORDER BY linked_at",
        )
        .bind(order_requirement_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(OrderRequirementFulfillmentRow::into_fulfillment)
        .collect()
    }

    async fn link_ticket_prerequisite_to_ticket(
        &self,
        ticket_prerequisite_id: TicketPrerequisiteId,
        fulfilling_ticket_id: TicketId,
        allocated_quantity: u64,
    ) -> Result<TicketPrerequisiteFulfillment, OrderError> {
        if allocated_quantity == 0 {
            return Err(OrderError::InvalidQuantity);
        }
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        let existing = sqlx::query_as::<_, TicketPrerequisiteFulfillmentRow>(
            r#"
            SELECT f.id, f.ticket_prerequisite_id, f.fulfilling_ticket_id, f.allocated_quantity, f.linked_at
            FROM ticket_prerequisite_fulfillments f
            JOIN tickets t ON t.id = f.fulfilling_ticket_id
            WHERE f.ticket_prerequisite_id = $1 AND f.fulfilling_ticket_id = $2 AND t.status <> 'canceled'
            "#,
        )
        .bind(ticket_prerequisite_id.0)
        .bind(fulfilling_ticket_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        if let Some(row) = existing {
            return row.into_fulfillment();
        }

        let prerequisite_type_id: Option<i64> =
            sqlx::query_scalar("SELECT type_id FROM ticket_prerequisites WHERE id = $1")
                .bind(ticket_prerequisite_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let prerequisite_type_id =
            prerequisite_type_id.ok_or(OrderError::TicketPrerequisiteNotFound)?;
        let ticket_type_id: Option<i64> =
            sqlx::query_scalar("SELECT type_id FROM tickets WHERE id = $1")
                .bind(fulfilling_ticket_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_error)?;
        let ticket_type_id = ticket_type_id.ok_or(OrderError::TicketNotFound)?;
        if prerequisite_type_id != ticket_type_id {
            return Err(OrderError::TicketTypeMismatch);
        }

        let id = Uuid::new_v4();
        let linked_at = crate::db_now();
        sqlx::query(
            r#"
            INSERT INTO ticket_prerequisite_fulfillments (
              id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(ticket_prerequisite_id.0)
        .bind(fulfilling_ticket_id.0)
        .bind(i64_from_u64(allocated_quantity)?)
        .bind(linked_at)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        tx.commit().await.map_err(map_error)?;
        Ok(TicketPrerequisiteFulfillment {
            id: TicketPrerequisiteFulfillmentId(id),
            ticket_prerequisite_id,
            fulfilling_ticket_id,
            allocated_quantity,
            linked_at,
        })
    }

    async fn list_ticket_prerequisite_fulfillments(
        &self,
        ticket_prerequisite_id: TicketPrerequisiteId,
    ) -> Result<Vec<TicketPrerequisiteFulfillment>, OrderError> {
        sqlx::query_as::<_, TicketPrerequisiteFulfillmentRow>(
            "SELECT id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at \
             FROM ticket_prerequisite_fulfillments WHERE ticket_prerequisite_id = $1 ORDER BY linked_at",
        )
        .bind(ticket_prerequisite_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketPrerequisiteFulfillmentRow::into_fulfillment)
        .collect()
    }

    async fn available_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, OrderError> {
        let balance: Option<i64> = sqlx::query_scalar(
            "SELECT quantity FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?;
        let balance = u64_from_i64(balance.unwrap_or(0))?;

        let allocated: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 \
             AND released_at IS NULL AND consumed_at IS NULL",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        let allocated = u64_from_i64(allocated)?;

        Ok(balance.saturating_sub(allocated))
    }

    async fn start_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE orders SET started_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND started_at IS NULL AND completed_at IS NULL AND canceled_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotStartable)
                .await?);
        }
        self.get_order(workspace_id, order_id).await
    }

    async fn complete_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        // Organizational only: stamp `completed_at`, nothing else. Final
        // production output is posted by the root Manufacturing ticket's
        // explicit `record_ticket_production`, never here. No inventory
        // event, no allocation mutation, no balance/cost-basis change, no
        // ticket mutation, no recording-completeness check.
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE orders SET completed_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND started_at IS NOT NULL AND completed_at IS NULL AND canceled_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotCompletable)
                .await?);
        }
        self.get_order(workspace_id, order_id).await
    }

    async fn start_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET status = 'in_progress', updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND status = 'todo'",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .ticket_precondition_error(workspace_id, ticket_id, OrderError::TicketNotStartable)
                .await?);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    async fn complete_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET status = 'complete', updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND status = 'in_progress'",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .ticket_precondition_error(
                    workspace_id,
                    ticket_id,
                    OrderError::TicketNotCompletable,
                )
                .await?);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    async fn record_ticket_acquisition(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordAcquisitionInput,
    ) -> Result<RecordAcquisitionOutcome, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        // Lock the ticket row -- serializes concurrent recordings for this
        // same ticket so the idempotency pre-check below is race-free. The
        // `UNIQUE (ticket_id, idempotency_key)` constraint is the
        // DB-authoritative backstop regardless (handled after the INSERT).
        let ticket_row: Option<(
            String,
            Uuid,
            i64,
            String,
            i64,
            String,
            Option<Uuid>,
            Option<Decimal>,
        )> = sqlx::query_as(
            "SELECT kind, owner_id, type_id, captured_name, quantity, display_id, \
                 acquisition_run_id, estimated_unit_cost FROM tickets \
                 WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let (
            kind_str,
            owner_id,
            type_id,
            captured_name,
            requested_quantity,
            display_id,
            acquisition_run_id,
            estimated_unit_cost,
        ) = ticket_row.ok_or(OrderError::TicketNotFound)?;

        if ticket_kind_from_str(&kind_str)? != TicketKind::Acquisition {
            return Err(OrderError::RecordingRequiresAcquisitionTicket);
        }
        if acquisition_run_id.is_some() {
            return Err(OrderError::RecordingNotAllowedForBatchedTicket);
        }
        let owner_id = OwnerId(owner_id);
        let requested_quantity = u64_from_i64(requested_quantity)?;
        let estimated_unit_cost = estimated_unit_cost.map(Money);

        // Idempotent replay: this key was already recorded -- post nothing.
        if let Some(existing) =
            find_ticket_recording_by_key(&mut tx, ticket_id, input.idempotency_key).await?
        {
            let recorded = recorded_quantity_sum(&mut tx, ticket_id).await?;
            tx.commit().await.map_err(map_error)?;
            return Ok(RecordAcquisitionOutcome {
                recording: existing,
                created: false,
                summary: derive_recording_summary(requested_quantity, recorded),
            });
        }

        let recording_id = TicketInventoryRecordingId::new();
        let recorded_at = crate::db_now();
        let insert = sqlx::query(
            "INSERT INTO ticket_inventory_recordings \
               (id, ticket_id, kind, idempotency_key, recorded_quantity, \
                location_note, note, recorded_at) \
             VALUES ($1, $2, 'acquisition', $3, $4, $5, $6, $7)",
        )
        .bind(recording_id.0)
        .bind(ticket_id.0)
        .bind(input.idempotency_key)
        .bind(i64_from_u64(input.quantity)?)
        .bind(&input.location_note)
        .bind(&input.note)
        .bind(recorded_at)
        .execute(&mut *tx)
        .await;
        if let Err(error) = insert {
            if is_unique_violation(&error) {
                // A concurrent duplicate won the race (not reachable under
                // the ticket `FOR UPDATE`, but the UNIQUE is authoritative).
                tx.rollback().await.map_err(map_error)?;
                let (recording, recorded) = self
                    .replay_ticket_recording(ticket_id, input.idempotency_key)
                    .await?;
                return Ok(RecordAcquisitionOutcome {
                    recording,
                    created: false,
                    summary: derive_recording_summary(requested_quantity, recorded),
                });
            }
            return Err(map_error(error));
        }

        // One `Purchase` for exactly this recording's quantity, cost via the
        // same hierarchy `complete_ticket`'s acquisition path uses. Keeps
        // `display_id` as `source_reference` (existing convention); the
        // recording link below is the stronger provenance.
        let event_id = post_purchase_in_transaction(
            &mut tx,
            InventoryPostingLine {
                key: InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id,
                },
                type_name: &captured_name,
                quantity: input.quantity,
                source_reference: &display_id,
            },
            input.unit_cost,
            estimated_unit_cost,
            input.effective_at,
            recorded_at,
        )
        .await?;

        sqlx::query("UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = $2")
            .bind(recording_id.0)
            .bind(event_id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;

        let recorded = recorded_quantity_sum(&mut tx, ticket_id).await?;
        tx.commit().await.map_err(map_error)?;

        Ok(RecordAcquisitionOutcome {
            recording: TicketInventoryRecording {
                id: recording_id,
                ticket_id,
                kind: TicketInventoryRecordingKind::Acquisition,
                recorded_quantity: Some(input.quantity),
                runs_completed: None,
                installation_cost: None,
                output_type_id: None,
                output_quantity: None,
                location_note: input.location_note,
                note: input.note,
                recorded_at,
                reverted_at: None,
                status: TicketInventoryRecordingStatus::Recorded,
                effects: Vec::new(),
            },
            created: true,
            summary: derive_recording_summary(requested_quantity, recorded),
        })
    }

    async fn record_ticket_production(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordProductionInput,
    ) -> Result<RecordProductionOutcome, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        // Lock the ticket -- serializes concurrent recordings for it, so
        // the idempotency pre-check is race-free (the UNIQUE is the
        // authoritative backstop after the INSERT).
        let ticket_row: Option<(String, Uuid, i64, String, String, Option<serde_json::Value>)> =
            sqlx::query_as(
                "SELECT kind, owner_id, type_id, captured_name, display_id, execution_snapshot \
                 FROM tickets WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            )
            .bind(workspace_id.0)
            .bind(ticket_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        let (kind_str, owner_id, ticket_type_id, ticket_captured_name, display_id, snapshot_json) =
            ticket_row.ok_or(OrderError::TicketNotFound)?;

        let kind = ticket_kind_from_str(&kind_str)?;
        if !matches!(kind, TicketKind::Manufacturing | TicketKind::Reaction) {
            return Err(OrderError::RecordingRequiresProductionTicket);
        }
        let owner_id = OwnerId(owner_id);
        let requested_runs = execution_snapshot_runs(&snapshot_json)?;

        // The output must be the ticket's product.
        if input.output_type_id != ticket_type_id {
            return Err(OrderError::RecordingOutputTypeMismatch);
        }

        // Every input must be one of the ticket's frozen prerequisites (its
        // material list). Quantities are actuals -- not checked against the
        // plan. NOTE (known limitation): a legitimate unplanned consumable
        // not in the frozen BOM is rejected here, rather than warned about.
        let prereq_names: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(
            "SELECT type_id, captured_name FROM ticket_prerequisites WHERE ticket_id = $1",
        )
        .bind(ticket_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?
        .into_iter()
        .collect();
        for line in &input.inputs {
            if line.quantity == 0 {
                return Err(OrderError::InvalidQuantity);
            }
            if !prereq_names.contains_key(&line.type_id) {
                return Err(OrderError::RecordingInputNotAPrerequisite);
            }
        }

        // Idempotent replay: this key was already recorded -- post nothing.
        if let Some(existing) =
            find_ticket_recording_by_key(&mut tx, ticket_id, input.idempotency_key).await?
        {
            let recorded = recorded_runs_sum(&mut tx, ticket_id).await?;
            tx.commit().await.map_err(map_error)?;
            return Ok(RecordProductionOutcome {
                recording: existing,
                created: false,
                summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
            });
        }

        let recording_id = TicketInventoryRecordingId::new();
        let recorded_at = crate::db_now();
        let insert = sqlx::query(
            "INSERT INTO ticket_inventory_recordings \
               (id, ticket_id, kind, idempotency_key, runs_completed, installation_cost, \
                output_type_id, output_quantity, location_note, note, recorded_at) \
             VALUES ($1, $2, 'production', $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(recording_id.0)
        .bind(ticket_id.0)
        .bind(input.idempotency_key)
        .bind(i64_from_u64(input.runs_completed)?)
        .bind(input.installation_cost.0)
        .bind(input.output_type_id)
        .bind(i64_from_u64(input.output_quantity)?)
        .bind(&input.location_note)
        .bind(&input.note)
        .bind(recorded_at)
        .execute(&mut *tx)
        .await;
        if let Err(error) = insert {
            if is_unique_violation(&error) {
                tx.rollback().await.map_err(map_error)?;
                let (recording, recorded) = self
                    .replay_ticket_recording(ticket_id, input.idempotency_key)
                    .await?;
                return Ok(RecordProductionOutcome {
                    recording,
                    created: false,
                    summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
                });
            }
            return Err(map_error(error));
        }

        // Every multi-balance inventory writer takes the complete lock set
        // in the same deterministic order as recording reversal. The
        // posting helpers below re-read already-held rows for their exact
        // revision and basis, but cannot deadlock by acquiring identities
        // in caller-provided input order.
        let mut balance_type_ids: Vec<i64> = input
            .inputs
            .iter()
            .map(|line| line.type_id)
            .chain((input.output_quantity > 0).then_some(input.output_type_id))
            .collect();
        balance_type_ids.sort_unstable();
        balance_type_ids.dedup();
        for type_id in balance_type_ids {
            sqlx::query(
                "SELECT revision FROM inventory_balances \
                 WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
            )
            .bind(workspace_id.0)
            .bind(owner_id.0)
            .bind(type_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        // Consume each actual input at the balance's current weighted
        // average; sum the real ledger basis removed.
        let mut consumed_basis = Decimal::ZERO;
        let mut event_ids: Vec<Uuid> = Vec::with_capacity(input.inputs.len() + 1);
        for line in &input.inputs {
            let type_name = prereq_names
                .get(&line.type_id)
                .expect("validated to be a prerequisite above");
            let (event_id, consumed) = post_consumption_in_transaction(
                &mut tx,
                InventoryPostingLine {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id: line.type_id,
                    },
                    type_name,
                    quantity: line.quantity,
                    source_reference: &display_id,
                },
                recorded_at,
            )
            .await?;
            consumed_basis += consumed;
            event_ids.push(event_id.0);
        }

        // Production batch basis = actual consumed material basis + actual
        // installation cost. Assigned to the output in full -- every
        // produced unit (planned and surplus alike) shares it per-unit.
        let mut production_basis = consumed_basis + input.installation_cost.0;
        production_basis.rescale(4);

        if input.output_quantity > 0 {
            // Display unit cost only; `total_cost_delta` stays authoritative
            // (`apply_inventory_event` re-derives the average from it).
            let unit_cost = Money(production_basis)
                .checked_div_quantity(input.output_quantity)
                .map_err(|error| OrderError::Persistence(error.to_string()))?;
            let output_event = post_production_output_in_transaction(
                &mut tx,
                InventoryPostingLine {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id: input.output_type_id,
                    },
                    type_name: &ticket_captured_name,
                    quantity: input.output_quantity,
                    source_reference: &display_id,
                },
                MoneyDelta(production_basis),
                Some(unit_cost),
                // Known: consumed basis is from the actual ledger and
                // installation cost is explicit -- nothing estimated.
                CostInputQuality::Known,
                input.effective_at,
                recorded_at,
            )
            .await?;
            event_ids.push(output_event.0);
        }

        sqlx::query(
            "UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = ANY($2)",
        )
        .bind(recording_id.0)
        .bind(&event_ids)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        let recorded = recorded_runs_sum(&mut tx, ticket_id).await?;
        tx.commit().await.map_err(map_error)?;

        Ok(RecordProductionOutcome {
            recording: TicketInventoryRecording {
                id: recording_id,
                ticket_id,
                kind: TicketInventoryRecordingKind::Production,
                recorded_quantity: None,
                runs_completed: Some(input.runs_completed),
                installation_cost: Some(input.installation_cost),
                output_type_id: Some(input.output_type_id),
                output_quantity: Some(input.output_quantity),
                location_note: input.location_note,
                note: input.note,
                recorded_at,
                reverted_at: None,
                status: TicketInventoryRecordingStatus::Recorded,
                effects: Vec::new(),
            },
            created: true,
            summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
        })
    }

    async fn revert_ticket_inventory_recording(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        recording_id: TicketInventoryRecordingId,
    ) -> Result<RevertTicketInventoryRecordingOutcome, OrderError> {
        #[derive(sqlx::FromRow)]
        struct LinkedEventRow {
            id: Uuid,
            workspace_id: Uuid,
            owner_id: Uuid,
            type_id: i64,
            captured_name: String,
            event_kind: String,
            quantity_delta: i64,
            total_cost_delta: Decimal,
            unit_cost: Option<Decimal>,
            cost_quality: String,
            source_reference: String,
            reversed_by_event_id: Option<Uuid>,
        }

        fn event_kind(value: &str) -> Result<InventoryEventKind, OrderError> {
            match value {
                "purchase" => Ok(InventoryEventKind::Purchase),
                "consumption" => Ok(InventoryEventKind::Consumption),
                "production_output" => Ok(InventoryEventKind::ProductionOutput),
                _ => Err(OrderError::RecordingEvidenceInvalid),
            }
        }

        fn cost_quality(value: &str) -> Result<CostInputQuality, OrderError> {
            match value {
                "known" => Ok(CostInputQuality::Known),
                "estimated" => Ok(CostInputQuality::Estimated),
                "zero_cost" => Ok(CostInputQuality::ZeroCost),
                _ => Err(OrderError::RecordingEvidenceInvalid),
            }
        }

        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let ticket: Option<(String, Uuid, Option<i64>, Option<serde_json::Value>)> =
            sqlx::query_as(
                "SELECT kind, owner_id, quantity, execution_snapshot FROM tickets \
                 WHERE workspace_id = $1 AND id = $2",
            )
            .bind(workspace_id.0)
            .bind(ticket_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        let (ticket_kind, ticket_owner_id, requested_quantity, execution_snapshot) =
            ticket.ok_or(OrderError::TicketNotFound)?;

        let recording = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, installation_cost, \
             output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE id = $1 FOR UPDATE",
        )
        .bind(recording_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?
        .ok_or(OrderError::RecordingNotFound)?
        .into_recording()?;
        if recording.ticket_id != ticket_id {
            return Err(OrderError::RecordingNotFound);
        }
        if recording.reverted_at.is_some() {
            return Err(OrderError::RecordingAlreadyReversed);
        }

        let mut events = sqlx::query_as::<_, LinkedEventRow>(
            "SELECT e.id, e.workspace_id, e.owner_id, e.type_id, e.captured_name, \
                    e.event_kind, e.quantity_delta, e.total_cost_delta, e.unit_cost, \
                    e.cost_quality, e.source_reference, reversed.id AS reversed_by_event_id \
             FROM inventory_events e \
             LEFT JOIN inventory_events reversed ON reversed.reverses_event_id = e.id \
             WHERE e.ticket_inventory_recording_id = $1 AND e.event_kind <> 'reversal'",
        )
        .bind(recording_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;

        if events.iter().any(|event| {
            event.workspace_id != workspace_id.0
                || event.owner_id != ticket_owner_id
                || event.reversed_by_event_id.is_some()
        }) {
            return Err(OrderError::RecordingEvidenceInvalid);
        }
        match recording.kind {
            TicketInventoryRecordingKind::Acquisition => {
                let expected_quantity = recording
                    .recorded_quantity
                    .and_then(|quantity| i64::try_from(quantity).ok());
                if events.len() != 1
                    || events[0].event_kind != "purchase"
                    || Some(events[0].quantity_delta) != expected_quantity
                    || events[0].quantity_delta <= 0
                    || events[0].total_cost_delta.is_sign_negative()
                {
                    return Err(OrderError::RecordingEvidenceInvalid);
                }
            }
            TicketInventoryRecordingKind::Production => {
                let expected_output_quantity = recording.output_quantity.unwrap_or(0);
                let outputs: Vec<&LinkedEventRow> = events
                    .iter()
                    .filter(|event| event.event_kind == "production_output")
                    .collect();
                let consumptions: Vec<&LinkedEventRow> = events
                    .iter()
                    .filter(|event| event.event_kind == "consumption")
                    .collect();
                let expected_output_count = usize::from(expected_output_quantity > 0);
                let invalid_shape = outputs.len() != expected_output_count
                    || events.len() != outputs.len() + consumptions.len()
                    || consumptions.iter().any(|event| {
                        event.quantity_delta >= 0 || event.total_cost_delta > Decimal::ZERO
                    });
                let invalid_output = outputs.first().is_some_and(|output| {
                    output.type_id != recording.output_type_id.unwrap_or_default()
                        || output.quantity_delta
                            != i64::try_from(expected_output_quantity).unwrap_or(i64::MIN)
                        || output.quantity_delta <= 0
                        || output.total_cost_delta.is_sign_negative()
                });
                let consumed_basis = consumptions
                    .iter()
                    .fold(Decimal::ZERO, |total, event| total - event.total_cost_delta);
                let mut expected_basis = consumed_basis
                    + recording
                        .installation_cost
                        .map(|money| money.0)
                        .unwrap_or(Decimal::ZERO);
                expected_basis.rescale(4);
                let invalid_basis = match outputs.first() {
                    Some(output) => output.total_cost_delta != expected_basis,
                    None => !expected_output_quantity.eq(&0),
                };
                if invalid_shape || invalid_output || invalid_basis {
                    return Err(OrderError::RecordingEvidenceInvalid);
                }
            }
        }

        let mut keys: Vec<(Uuid, Uuid, i64)> = events
            .iter()
            .map(|event| (event.workspace_id, event.owner_id, event.type_id))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let mut revisions = HashMap::with_capacity(keys.len());
        for (event_workspace_id, event_owner_id, type_id) in keys {
            let revision: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM inventory_balances \
                 WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
            )
            .bind(event_workspace_id)
            .bind(event_owner_id)
            .bind(type_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
            revisions.insert(
                (event_workspace_id, event_owner_id, type_id),
                u64_from_i64(revision.ok_or(OrderError::RecordingEvidenceInvalid)?)?,
            );
        }

        // Balance locks serialize every writer affecting these identities.
        // Re-read reversal state only now so a generic reversal that raced
        // before the locks cannot be missed by the earlier evidence load.
        let reversed_while_waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inventory_events reversal \
             JOIN inventory_events original ON original.id = reversal.reverses_event_id \
             WHERE original.ticket_inventory_recording_id = $1)",
        )
        .bind(recording_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        if reversed_while_waiting {
            return Err(OrderError::RecordingAlreadyReversed);
        }

        events.sort_by_key(|event| {
            let compensation = event.quantity_delta.saturating_neg();
            (
                compensation >= 0,
                event.workspace_id,
                event.owner_id,
                event.type_id,
                event.id,
            )
        });
        let now = crate::db_now();
        for event in events {
            let key_tuple = (event.workspace_id, event.owner_id, event.type_id);
            let expected_revision = *revisions
                .get(&key_tuple)
                .ok_or(OrderError::RecordingEvidenceInvalid)?;
            let posting = InventoryPosting {
                id: InventoryEventId::new(),
                key: InventoryItemKey {
                    workspace_id: WorkspaceId(event.workspace_id),
                    owner_id: OwnerId(event.owner_id),
                    type_id: event.type_id,
                },
                type_name: event.captured_name,
                kind: InventoryEventKind::Reversal,
                quantity_delta: event
                    .quantity_delta
                    .checked_neg()
                    .ok_or(OrderError::RecordingEvidenceInvalid)?,
                total_cost_delta: MoneyDelta(if event.total_cost_delta.is_zero() {
                    Decimal::ZERO
                } else {
                    -event.total_cost_delta
                }),
                unit_cost: event.unit_cost.map(Money),
                cost_quality: cost_quality(&event.cost_quality)?,
                source_reference: event.source_reference,
                note: "Ticket recording reverted".to_string(),
                effective_at: now,
                recorded_at: now,
                expected_revision,
                reverses_event_id: Some(InventoryEventId(event.id)),
            };
            event_kind(&event.event_kind)?;
            let reversal_id = posting.id;
            let resulting =
                PgInventoryRepository::post_historical_reversal_in_transaction(&mut tx, &posting)
                    .await
                    .map_err(|error| match error {
                        iskworks_core::InventoryError::AlreadyReversed => {
                            OrderError::RecordingAlreadyReversed
                        }
                        _ => OrderError::RecordingReversalInvalid,
                    })?;
            sqlx::query(
                "UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = $2",
            )
            .bind(recording_id.0)
            .bind(reversal_id.0)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    OrderError::RecordingAlreadyReversed
                } else {
                    map_error(error)
                }
            })?;
            revisions.insert(key_tuple, resulting.revision);
        }

        let reverted_at = crate::db_now();
        let updated = sqlx::query(
            "UPDATE ticket_inventory_recordings SET reverted_at = $1 \
             WHERE id = $2 AND reverted_at IS NULL",
        )
        .bind(reverted_at)
        .bind(recording_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if updated.rows_affected() != 1 {
            return Err(OrderError::RecordingAlreadyReversed);
        }

        let recorded = match recording.kind {
            TicketInventoryRecordingKind::Acquisition => {
                recorded_quantity_sum(&mut tx, ticket_id).await?
            }
            TicketInventoryRecordingKind::Production => {
                recorded_runs_sum(&mut tx, ticket_id).await?
            }
        };
        let requested = match ticket_kind_from_str(&ticket_kind)? {
            TicketKind::Acquisition => requested_quantity
                .map(u64_from_i64)
                .transpose()?
                .unwrap_or_default(),
            TicketKind::Manufacturing | TicketKind::Reaction => {
                execution_snapshot_runs(&execution_snapshot)?.unwrap_or(recorded)
            }
            TicketKind::Generic => recorded,
        };
        tx.commit().await.map_err(map_error)?;

        Ok(RevertTicketInventoryRecordingOutcome {
            recording: TicketInventoryRecording {
                reverted_at: Some(reverted_at),
                status: TicketInventoryRecordingStatus::Reversed,
                ..recording
            },
            summary: derive_recording_summary(requested, recorded),
        })
    }

    async fn list_ticket_inventory_recordings(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketInventoryRecording>, OrderError> {
        let mut recordings = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, \
             installation_cost, output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE ticket_id = $1 \
             ORDER BY recorded_at ASC, id ASC",
        )
        .bind(ticket_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketInventoryRecordingRow::into_recording)
        .collect::<Result<Vec<_>, _>>()?;
        for recording in &mut recordings {
            let rows: Vec<(Uuid, String, i64, String, i64, Decimal)> = sqlx::query_as(
                "SELECT id, event_kind, type_id, captured_name, quantity_delta, total_cost_delta \
                 FROM inventory_events WHERE ticket_inventory_recording_id = $1 \
                 AND event_kind <> 'reversal' ORDER BY sequence, id",
            )
            .bind(recording.id.0)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;
            recording.effects = rows
                .into_iter()
                .map(
                    |(id, kind, type_id, captured_name, quantity_delta, total_cost_delta)| {
                        let kind = match kind.as_str() {
                            "purchase" => InventoryEventKind::Purchase,
                            "consumption" => InventoryEventKind::Consumption,
                            "production_output" => InventoryEventKind::ProductionOutput,
                            _ => return Err(OrderError::RecordingEvidenceInvalid),
                        };
                        Ok(TicketInventoryEffect {
                            event_id: InventoryEventId(id),
                            kind,
                            type_id,
                            captured_name,
                            quantity_delta,
                            total_cost_delta: MoneyDelta(total_cost_delta),
                        })
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(recordings)
    }

    async fn cancel_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        // Organizational only: stamp `canceled_at`. An Order owns no
        // inventory reservations, so there is nothing to release, and it
        // never cascades onto its linked tickets -- a shared ticket may
        // still be needed by other work.
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE orders SET canceled_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND completed_at IS NULL AND canceled_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotCancelable)
                .await?);
        }
        self.get_order(workspace_id, order_id).await
    }

    async fn archive_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE orders SET archived_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND archived_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotArchivable)
                .await?);
        }
        self.get_order(workspace_id, order_id).await
    }

    async fn restore_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE orders SET archived_at = NULL, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND archived_at IS NOT NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotRestorable)
                .await?);
        }
        self.get_order(workspace_id, order_id).await
    }

    async fn delete_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<(), OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let price_snapshot_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT price_snapshot_id FROM orders WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(order_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let price_snapshot_id = price_snapshot_id.ok_or(OrderError::OrderNotFound)?;
        sqlx::query("DELETE FROM orders WHERE workspace_id = $1 AND id = $2")
            .bind(workspace_id.0)
            .bind(order_id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        // The frozen snapshot is Epic-owned once captured. Delete it only
        // after the Epic row is gone, and only if no surviving Epic can
        // reference it (the FK is UNIQUE today; the guard is future-safe).
        sqlx::query(
            "DELETE FROM price_snapshots ps WHERE ps.id = $1 \
             AND NOT EXISTS (SELECT 1 FROM orders o WHERE o.price_snapshot_id = ps.id)",
        )
        .bind(price_snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }

    /// Workflow-only, like `set_ticket_status`/`complete_ticket`: sets
    /// `status = 'canceled'`/`updated_at` and nothing else. Tickets own no
    /// reservations, so there is nothing to release, and no *other* ticket's
    /// status is ever recomputed. "Canceled" means "I am no longer tracking
    /// this work as active," not "undo accounting/history." Valid from
    /// `todo` or `in_progress` (a `complete`/`canceled` ticket is left
    /// alone).
    async fn cancel_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET status = 'canceled', updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND status IN ('todo', 'in_progress')",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .ticket_precondition_error(workspace_id, ticket_id, OrderError::TicketNotCancelable)
                .await?);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    async fn archive_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET archived_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND archived_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .ticket_precondition_error(workspace_id, ticket_id, OrderError::TicketNotArchivable)
                .await?);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    async fn restore_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET archived_at = NULL, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND archived_at IS NOT NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(self
                .ticket_precondition_error(workspace_id, ticket_id, OrderError::TicketNotRestorable)
                .await?);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    async fn delete_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<(), OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM tickets WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        if exists.is_none() {
            return Err(OrderError::TicketNotFound);
        }
        crate::cascade::teardown_tickets(&mut tx, &[ticket_id.0])
            .await
            .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }

    /// Bare workflow-status write -- see `OrderRepository::set_ticket_status`'s
    /// own doc. One `UPDATE`, no transaction, no `FOR UPDATE`, no cascade,
    /// no inventory: `tickets.status`/`updated_at` and nothing else. Every
    /// transition is legal (no `AND status IN (...)` guard, unlike
    /// `start_ticket`/`cancel_ticket`), so Board drags move both
    /// directions. A same-status write still matches its row, so it
    /// returns the ticket rather than a spurious `TicketNotFound`.
    async fn set_ticket_status(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        status: TicketStatus,
    ) -> Result<Ticket, OrderError> {
        let now = crate::db_now();
        let result = sqlx::query(
            "UPDATE tickets SET status = $1, updated_at = $2 WHERE workspace_id = $3 AND id = $4",
        )
        .bind(ticket_status_str(status))
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(OrderError::TicketNotFound);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    /// A bare metadata write -- see `TicketMetadataUpdate`'s own doc for
    /// the omitted/clear/set three-value semantics per field. Builds one
    /// dynamic `UPDATE` (only the fields actually present), never touches
    /// `status`/`recording`/`execution_snapshot`/`source_build_id`/
    /// requirement fulfillments/AcquisitionRun membership. An
    /// entirely-omitted update is still a valid call (a no-op that just
    /// validates the ticket exists in this workspace), matching
    /// `set_ticket_status`'s own "only failure is TicketNotFound" contract.
    async fn update_ticket_metadata(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        update: TicketMetadataUpdate,
    ) -> Result<Ticket, OrderError> {
        if update.captured_name.is_none()
            && update.notes.is_none()
            && update.order_id.is_none()
            && update.assignee_character_id.is_none()
        {
            return self.get_ticket(workspace_id, ticket_id).await;
        }

        let mut query = sqlx::QueryBuilder::<Postgres>::new("UPDATE tickets SET updated_at = ");
        query.push_bind(crate::db_now());
        if let Some(captured_name) = update.captured_name {
            query.push(", captured_name = ").push_bind(captured_name);
        }
        if let Some(notes) = update.notes {
            query.push(", notes = ").push_bind(notes);
        }
        if let Some(order_id) = update.order_id {
            query
                .push(", order_id = ")
                .push_bind(order_id.map(|id| id.0));
        }
        if let Some(assignee_character_id) = update.assignee_character_id {
            query
                .push(", assignee_character_id = ")
                .push_bind(assignee_character_id.map(|id| id.0));
        }
        query
            .push(" WHERE workspace_id = ")
            .push_bind(workspace_id.0);
        query.push(" AND id = ").push_bind(ticket_id.0);

        let result = query.build().execute(&self.pool).await.map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(OrderError::TicketNotFound);
        }
        self.get_ticket(workspace_id, ticket_id).await
    }

    // Acquisition Run batching for standalone tickets -- implementations
    // live in `acquisition.rs` (see that module's own doc comment for why
    // this trait's methods can't live in a second `impl` block there).
    async fn create_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        name: Option<String>,
        ticket_ids: Vec<TicketId>,
    ) -> Result<AcquisitionRun, OrderError> {
        acquisition::create_order_acquisition_run(
            &self.pool,
            workspace_id,
            owner_id,
            name,
            ticket_ids,
        )
        .await
    }

    async fn list_order_acquisition_runs(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<AcquisitionRun>, OrderError> {
        acquisition::list_order_acquisition_runs(&self.pool, workspace_id, owner_id).await
    }

    async fn get_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError> {
        acquisition::get_order_acquisition_run(&self.pool, workspace_id, run_id).await
    }

    async fn list_order_acquisition_run_tickets(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<Vec<Ticket>, OrderError> {
        acquisition::list_order_acquisition_run_tickets(&self.pool, workspace_id, run_id).await
    }

    async fn list_order_acquisition_run_items(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
    ) -> Result<Vec<AcquisitionRunItem>, OrderError> {
        acquisition::list_order_acquisition_run_items(&self.pool, workspace_id, run_id).await
    }

    async fn start_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError> {
        acquisition::start_order_acquisition_run(&self.pool, workspace_id, owner_id, run_id).await
    }

    async fn record_order_acquisition_progress(
        &self,
        workspace_id: WorkspaceId,
        run_id: AcquisitionRunId,
        items: Vec<AcquisitionProgressUpdate>,
    ) -> Result<AcquisitionRun, OrderError> {
        acquisition::record_order_acquisition_progress(&self.pool, workspace_id, run_id, items)
            .await
    }

    async fn complete_order_acquisition_run(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        run_id: AcquisitionRunId,
    ) -> Result<AcquisitionRun, OrderError> {
        acquisition::complete_order_acquisition_run(&self.pool, workspace_id, owner_id, run_id)
            .await
    }
}

impl PgOrderRepository {
    async fn order_precondition_error(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
        not_eligible: OrderError,
    ) -> Result<OrderError, OrderError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM orders WHERE workspace_id = $1 AND id = $2)",
        )
        .bind(workspace_id.0)
        .bind(order_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(if exists {
            not_eligible
        } else {
            OrderError::OrderNotFound
        })
    }

    async fn ticket_precondition_error(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        not_eligible: OrderError,
    ) -> Result<OrderError, OrderError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tickets WHERE workspace_id = $1 AND id = $2)",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(if exists {
            not_eligible
        } else {
            OrderError::TicketNotFound
        })
    }

    /// Belt-and-braces path for `record_ticket_acquisition` /
    /// `record_ticket_production` when the recording INSERT hit the
    /// `(ticket_id, idempotency_key)` UNIQUE (a concurrent duplicate that
    /// somehow bypassed the ticket row lock). The winning transaction has
    /// committed; read its recording back on a fresh connection and return
    /// it, plus the kind-appropriate recorded total, as an idempotent
    /// replay -- the caller wraps it in the right outcome type.
    async fn replay_ticket_recording(
        &self,
        ticket_id: TicketId,
        idempotency_key: uuid::Uuid,
    ) -> Result<(TicketInventoryRecording, u64), OrderError> {
        let recording = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, \
             installation_cost, output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE ticket_id = $1 AND idempotency_key = $2",
        )
        .bind(ticket_id.0)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or_else(|| {
            OrderError::Persistence(
                "recording disappeared after a unique-violation replay".to_string(),
            )
        })?
        .into_recording()?;
        let column = match recording.kind {
            TicketInventoryRecordingKind::Acquisition => "recorded_quantity",
            TicketInventoryRecordingKind::Production => "runs_completed",
        };
        let sum: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(SUM({column}), 0)::bigint \
             FROM ticket_inventory_recordings WHERE ticket_id = $1 AND reverted_at IS NULL"
        ))
        .bind(ticket_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        Ok((recording, u64_from_i64(sum)?))
    }
}
