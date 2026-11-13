//! `OrderStore` for Postgres: Orders (Epics), their frozen plan and
//! requirements, reservations, fulfillments and lifecycle.

use super::*;

use super::inserts::*;

#[async_trait]
impl OrderStore for PgOrderRepository {
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

    async fn get_orders(
        &self,
        workspace_id: WorkspaceId,
        order_ids: &[OrderId],
    ) -> Result<Vec<Order>, OrderError> {
        let ids: Vec<uuid::Uuid> = order_ids.iter().map(|id| id.0).collect();
        sqlx::query_as::<_, OrderRow>(
            "SELECT id, workspace_id, owner_id, source_build_id, source_build_revision, \
             display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, \
             expected_revenue, estimated_margin, missing_price_count, created_at, updated_at, \
             started_at, completed_at, canceled_at, archived_at, planning_snapshot_version \
             FROM orders WHERE workspace_id = $1 AND id = ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(OrderRow::into_order)
        .collect()
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
        self.list_order_requirements_for_orders(&[order_id]).await
    }

    async fn list_order_requirements_for_orders(
        &self,
        order_ids: &[OrderId],
    ) -> Result<Vec<OrderRequirement>, OrderError> {
        sqlx::query_as::<_, OrderRequirementRow>(
            "SELECT id, order_id, type_id, captured_name, kind, source_build_id, \
             required_quantity, fulfillment_scope, reused_quantity, fresh_quantity, \
             estimated_unit_cost, estimated_line_total, reused_line_total, \
             operation_occurrence_key, child_occurrence_key, inventory_unit_basis, \
             child_produced_quantity, child_consumed_quantity, child_surplus_quantity, \
             child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id \
             FROM order_requirements WHERE order_id = ANY($1) ORDER BY order_id, type_id",
        )
        .bind(order_ids.iter().map(|id| id.0).collect::<Vec<_>>())
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
        self.list_order_plan_operations_for_orders(&[order_id])
            .await
    }

    async fn list_order_plan_operations_for_orders(
        &self,
        order_ids: &[OrderId],
    ) -> Result<Vec<PlanOperation>, OrderError> {
        sqlx::query_as::<_, PlanOperationRow>(
            "SELECT id, order_id, occurrence_key, parent_occurrence_key, build_id, activity, \
             runs, persisted_runs, product_type_id, product_name, output_per_run, \
             produced_quantity, blueprint_or_formula_type_id, material_component_cost, \
             own_installation_cost, total_production_cost, complete, evidence, created_at, \
             consumed_quantity, surplus_quantity, surplus_retained_basis \
             FROM order_plan_operations WHERE order_id = ANY($1) \
             ORDER BY order_id, created_at, occurrence_key",
        )
        .bind(order_ids.iter().map(|id| id.0).collect::<Vec<_>>())
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

            // Each operation's ticket holds that operation's claim, so a
            // later "create ticket for this step" sees it.
            if let Some(occurrence_key) = new_ticket.occurrence_key.as_deref() {
                super::reservations::claim_operation(
                    &mut tx,
                    order.id,
                    occurrence_key,
                    new_ticket.id,
                    now,
                )
                .await?;
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

            let operations = super::reservations::own_operation(
                new_ticket.order_id,
                new_ticket.occurrence_key.as_deref(),
            );
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
                completed_at: None,
                archived_at: None,
                occurrence_key: new_ticket.occurrence_key,
                parent_ticket_id: new_ticket.parent_ticket_id,
                produced_quantity: new_ticket.produced_quantity,
                material_component_cost: new_ticket.material_component_cost,
                own_installation_cost: new_ticket.own_installation_cost,
                total_production_cost: new_ticket.total_production_cost,
                plan_evidence: new_ticket.plan_evidence,
                merged_into_ticket_id: None,
                operations,
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

    async fn reserve_order_inventory(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<CappedReservationPlan, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let plan = super::reservations::top_up_order_reservations(
            &mut tx,
            workspace_id,
            order_id,
            crate::db_now(),
        )
        .await?;
        tx.commit().await.map_err(map_error)?;
        Ok(plan)
    }

    async fn requirement_reservation_totals(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Vec<RequirementReservationTotals>, OrderError> {
        self.requirement_reservation_totals_for_orders(workspace_id, &[order_id])
            .await
    }

    async fn requirement_reservation_totals_for_orders(
        &self,
        workspace_id: WorkspaceId,
        order_ids: &[OrderId],
    ) -> Result<Vec<RequirementReservationTotals>, OrderError> {
        let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
            "SELECT r.id, \
               COALESCE(SUM(a.quantity) FILTER \
                 (WHERE a.released_at IS NULL AND a.consumed_at IS NULL), 0)::bigint, \
               COALESCE(SUM(a.quantity) FILTER \
                 (WHERE a.released_at IS NULL AND a.consumed_at IS NOT NULL), 0)::bigint \
             FROM order_requirements r \
             JOIN orders o ON o.id = r.order_id \
             LEFT JOIN inventory_allocations a ON a.order_requirement_id = r.id \
             WHERE r.order_id = ANY($1) AND o.workspace_id = $2 \
             GROUP BY r.id",
        )
        .bind(order_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        rows.into_iter()
            .map(|(id, reserved, consumed)| {
                Ok(RequirementReservationTotals {
                    requirement_id: OrderRequirementId(id),
                    reserved: u64_from_i64(reserved)?,
                    consumed: u64_from_i64(consumed)?,
                })
            })
            .collect()
    }

    async fn list_order_requirement_fulfillments(
        &self,
        order_requirement_id: OrderRequirementId,
    ) -> Result<Vec<OrderRequirementFulfillment>, OrderError> {
        self.list_order_requirement_fulfillments_for_requirements(&[order_requirement_id])
            .await
    }

    async fn list_order_requirement_fulfillments_for_requirements(
        &self,
        order_requirement_ids: &[OrderRequirementId],
    ) -> Result<Vec<OrderRequirementFulfillment>, OrderError> {
        sqlx::query_as::<_, OrderRequirementFulfillmentRow>(
            "SELECT id, order_requirement_id, ticket_id, allocated_quantity, linked_at \
             FROM order_requirement_fulfillments WHERE order_requirement_id = ANY($1) \
             ORDER BY order_requirement_id, linked_at",
        )
        .bind(
            order_requirement_ids
                .iter()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(OrderRequirementFulfillmentRow::into_fulfillment)
        .collect()
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
        // Stamp `completed_at` and release whatever the Epic still holds
        // (actuals that differed from the plan, skipped or unrecorded
        // work): a completed Epic has nothing left to use it for. No
        // inventory event, no balance/cost-basis change, no ticket
        // mutation, no recording-completeness check. Final production
        // output is posted by the root ticket's `record_ticket_production`.
        let now = crate::db_now();
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let result = sqlx::query(
            "UPDATE orders SET completed_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND started_at IS NOT NULL AND completed_at IS NULL AND canceled_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(map_error)?;
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotCompletable)
                .await?);
        }
        super::reservations::release_order_allocations(&mut tx, order_id, now).await?;
        tx.commit().await.map_err(map_error)?;
        self.get_order(workspace_id, order_id).await
    }

    async fn cancel_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        // Stamp `canceled_at` and release the Epic's reservations back to
        // free stock in one transaction (no ledger event: nothing was
        // used). It never cascades onto linked tickets -- a shared ticket
        // may still be needed by other work.
        let now = crate::db_now();
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let result = sqlx::query(
            "UPDATE orders SET canceled_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 \
             AND completed_at IS NULL AND canceled_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(map_error)?;
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotCancelable)
                .await?);
        }
        super::reservations::release_order_allocations(&mut tx, order_id, now).await?;
        tx.commit().await.map_err(map_error)?;
        self.get_order(workspace_id, order_id).await
    }

    async fn archive_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Order, OrderError> {
        // Archiving releases the Epic's reservations too; restoring does
        // not re-reserve (see `reserve_order_inventory`).
        let now = crate::db_now();
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let result = sqlx::query(
            "UPDATE orders SET archived_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND archived_at IS NULL",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(order_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(map_error)?;
            return Err(self
                .order_precondition_error(workspace_id, order_id, OrderError::OrderNotArchivable)
                .await?);
        }
        super::reservations::release_order_allocations(&mut tx, order_id, now).await?;
        tx.commit().await.map_err(map_error)?;
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
}
