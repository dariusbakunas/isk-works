//! `TicketStore` for Postgres: Ticket creation, reads, prerequisites,
//! combine/split and lifecycle.

use super::*;

use super::inserts::*;

#[async_trait]
impl TicketStore for PgOrderRepository {
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

    async fn get_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Ticket, OrderError> {
        let ticket = sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, completed_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence, merged_into_ticket_id \
             FROM tickets WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(OrderError::TicketNotFound)?
        .into_ticket()?;
        let mut tickets = super::reservations::with_operations(&self.pool, vec![ticket]).await?;
        Ok(tickets.remove(0))
    }

    async fn list_tickets(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<Ticket>, OrderError> {
        let tickets = sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, completed_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence, merged_into_ticket_id \
             FROM tickets WHERE workspace_id = $1 AND owner_id = $2 ORDER BY display_id",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect::<Result<Vec<_>, _>>()?;
        super::reservations::with_operations(&self.pool, tickets).await
    }

    async fn get_tickets(
        &self,
        workspace_id: WorkspaceId,
        ticket_ids: &[TicketId],
    ) -> Result<Vec<Ticket>, OrderError> {
        let tickets = sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, completed_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence, merged_into_ticket_id \
             FROM tickets WHERE workspace_id = $1 AND id = ANY($2) ORDER BY display_id",
        )
        .bind(workspace_id.0)
        .bind(ticket_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect::<Result<Vec<_>, _>>()?;
        super::reservations::with_operations(&self.pool, tickets).await
    }

    async fn list_tickets_for_order(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
    ) -> Result<Vec<Ticket>, OrderError> {
        self.list_tickets_for_orders(workspace_id, &[order_id])
            .await
    }

    async fn list_tickets_for_orders(
        &self,
        workspace_id: WorkspaceId,
        order_ids: &[OrderId],
    ) -> Result<Vec<Ticket>, OrderError> {
        let tickets = sqlx::query_as::<_, TicketRow>(
            "SELECT id, workspace_id, owner_id, display_id, kind, type_id, captured_name, \
             quantity, order_id, source_build_id, notes, assignee_character_id, status, estimated_unit_cost, estimated_line_total, \
             actual_unit_cost, actual_line_total, market_region_id, market_location_id, \
             price_source_id, acquisition_run_id, acquired_quantity, \
             execution_snapshot, created_at, updated_at, completed_at, archived_at, \
             occurrence_key, parent_ticket_id, produced_quantity, material_component_cost, \
             own_installation_cost, total_production_cost, plan_evidence, merged_into_ticket_id \
             FROM tickets WHERE workspace_id = $1 AND (order_id = ANY($2) OR id IN ( \
               SELECT ticket_id FROM order_operation_ticket_claims WHERE order_id = ANY($2))) \
             ORDER BY order_id NULLS LAST, display_id",
        )
        .bind(workspace_id.0)
        .bind(order_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect::<Result<Vec<_>, _>>()?;
        super::reservations::with_operations(&self.pool, tickets).await
    }

    async fn list_ticket_prerequisites(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketPrerequisite>, OrderError> {
        self.list_ticket_prerequisites_for_tickets(&[ticket_id])
            .await
    }

    async fn list_ticket_prerequisites_for_tickets(
        &self,
        ticket_ids: &[TicketId],
    ) -> Result<Vec<TicketPrerequisite>, OrderError> {
        sqlx::query_as::<_, TicketPrerequisiteRow>(
            "SELECT id, ticket_id, type_id, captured_name, kind, source_build_id, \
             required_quantity, fulfillment_scope, reused_quantity, fresh_quantity, \
             estimated_unit_cost, estimated_line_total, reused_line_total, \
             operation_occurrence_key, child_occurrence_key, inventory_unit_basis, \
             child_produced_quantity, child_consumed_quantity, child_surplus_quantity, \
             child_surplus_retained_basis, price_evidence, child_consumed_cost, dependency_id, \
             order_id \
             FROM ticket_prerequisites WHERE ticket_id = ANY($1) ORDER BY ticket_id, type_id",
        )
        .bind(ticket_ids.iter().map(|id| id.0).collect::<Vec<_>>())
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

    async fn create_ticket_for_order_requirements(
        &self,
        new_ticket: NewTicket,
        links: &[(OrderRequirementId, u64)],
    ) -> Result<RequirementTicketCreation, OrderError> {
        if links.is_empty() || links.iter().any(|(_, quantity)| *quantity == 0) {
            return Err(OrderError::InvalidQuantity);
        }
        // Claim in id order so two overlapping creates can't deadlock.
        let mut links = links.to_vec();
        links.sort_by_key(|(requirement_id, _)| requirement_id.0);
        links.dedup_by_key(|(requirement_id, _)| requirement_id.0);
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = crate::db_now();

        let requirement_ids: Vec<Uuid> = links.iter().map(|(id, _)| id.0).collect();
        let type_ids: Vec<i64> =
            sqlx::query_scalar("SELECT type_id FROM order_requirements WHERE id = ANY($1)")
                .bind(&requirement_ids)
                .fetch_all(&mut *tx)
                .await
                .map_err(map_error)?;
        if type_ids.len() != links.len() {
            return Err(OrderError::OrderRequirementNotFound);
        }
        if type_ids
            .iter()
            .any(|type_id| new_ticket.type_id != Some(*type_id))
        {
            return Err(OrderError::TicketTypeMismatch);
        }

        for (order_requirement_id, _) in &links {
            // Claim the requirement first. The primary key is the
            // uniqueness guarantee: a concurrent claimer blocks here until
            // this transaction ends, then sees the committed claim (or none,
            // if we rolled back). `ticket_id`'s FK is deferred to commit, so
            // the ticket row is only inserted once every claim is ours.
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
            if claimed {
                continue;
            }
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
                tx.rollback().await.map_err(map_error)?;
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
        for (order_requirement_id, allocated_quantity) in &links {
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
            .bind(i64_from_u64(*allocated_quantity)?)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        tx.commit().await.map_err(map_error)?;
        Ok(RequirementTicketCreation::Created(Box::new(ticket)))
    }

    async fn create_operation_ticket(
        &self,
        workspace_id: WorkspaceId,
        order_id: OrderId,
        occurrence_key: &str,
    ) -> Result<OperationTicketCreation, OrderError> {
        let order = self.get_order(workspace_id, order_id).await?;
        if order.planning_snapshot_version < 3 {
            return Err(OrderError::FrozenPlanUnavailable);
        }
        let operations = self.list_order_plan_operations(order_id).await?;
        let operation = operations
            .iter()
            .find(|operation| operation.occurrence_key == occurrence_key)
            .ok_or(OrderError::OperationNotFound)?;
        let requirements = self.list_order_requirements(order_id).await?;
        let mut new_ticket = operation_ticket(&order, operation, &requirements);

        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = crate::db_now();
        if let Some(holder) = super::reservations::claim_operation(
            &mut tx,
            order_id,
            occurrence_key,
            new_ticket.id,
            now,
        )
        .await?
        {
            return Ok(OperationTicketCreation::AlreadyExists(holder));
        }

        // Link under the parent operation's active ticket, if it has one.
        if let Some(parent_key) = operation.parent_occurrence_key.as_deref() {
            new_ticket.parent_ticket_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT c.ticket_id FROM order_operation_ticket_claims c \
                 JOIN tickets t ON t.id = c.ticket_id \
                 WHERE c.order_id = $1 AND c.occurrence_key = $2 AND t.status <> 'canceled'",
            )
            .bind(order_id.0)
            .bind(parent_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?
            .map(TicketId);
        }
        let (ticket, _) = insert_ticket_with_prerequisites(&mut tx, new_ticket, now).await?;

        // Adopt child operations' tickets created before this one.
        sqlx::query(
            "UPDATE tickets SET parent_ticket_id = $1 \
             WHERE order_id = $2 AND parent_ticket_id IS NULL AND status <> 'canceled' \
             AND occurrence_key IN (SELECT occurrence_key FROM order_plan_operations \
                                    WHERE order_id = $2 AND parent_occurrence_key = $3)",
        )
        .bind(ticket.id.0)
        .bind(order_id.0)
        .bind(occurrence_key)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        tx.commit().await.map_err(map_error)?;
        Ok(OperationTicketCreation::Created(Box::new(ticket)))
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
        self.list_ticket_prerequisite_fulfillments_for_prerequisites(&[ticket_prerequisite_id])
            .await
    }

    async fn list_ticket_prerequisite_fulfillments_for_prerequisites(
        &self,
        ticket_prerequisite_ids: &[TicketPrerequisiteId],
    ) -> Result<Vec<TicketPrerequisiteFulfillment>, OrderError> {
        sqlx::query_as::<_, TicketPrerequisiteFulfillmentRow>(
            "SELECT id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at \
             FROM ticket_prerequisite_fulfillments WHERE ticket_prerequisite_id = ANY($1) \
             ORDER BY ticket_prerequisite_id, linked_at",
        )
        .bind(
            ticket_prerequisite_ids
                .iter()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketPrerequisiteFulfillmentRow::into_fulfillment)
        .collect()
    }

    async fn list_ticket_operations(
        &self,
        ticket_ids: &[TicketId],
    ) -> Result<HashMap<TicketId, Vec<TicketOperation>>, OrderError> {
        let ids: Vec<Uuid> = ticket_ids.iter().map(|id| id.0).collect();
        super::reservations::ticket_operations(&self.pool, &ids).await
    }

    async fn combine_tickets(
        &self,
        workspace_id: WorkspaceId,
        ticket_ids: &[TicketId],
    ) -> Result<Ticket, OrderError> {
        let combined =
            super::combine::combine_tickets(&self.pool, workspace_id, ticket_ids).await?;
        self.get_ticket(workspace_id, combined).await
    }

    async fn split_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
    ) -> Result<Vec<Ticket>, OrderError> {
        let members = super::combine::split_ticket(&self.pool, workspace_id, ticket_id).await?;
        self.get_tickets(workspace_id, &members).await
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
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let result = sqlx::query(
            "UPDATE tickets SET status = 'complete', completed_at = $1, updated_at = $1 \
             WHERE workspace_id = $2 AND id = $3 AND status = 'in_progress'",
        )
        .bind(now)
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(map_error)?;
            return Err(self
                .ticket_precondition_error(
                    workspace_id,
                    ticket_id,
                    OrderError::TicketNotCompletable,
                )
                .await?);
        }
        super::epic_completion::complete_epic_if_finished(&mut tx, workspace_id, ticket_id, now)
            .await?;
        tx.commit().await.map_err(map_error)?;
        self.get_ticket(workspace_id, ticket_id).await
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

    async fn set_ticket_status(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        status: TicketStatus,
    ) -> Result<Ticket, OrderError> {
        self.update_ticket(
            workspace_id,
            ticket_id,
            TicketMetadataUpdate::default(),
            Some(status),
        )
        .await
    }

    async fn update_ticket_metadata(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        update: TicketMetadataUpdate,
    ) -> Result<Ticket, OrderError> {
        self.update_ticket(workspace_id, ticket_id, update, None)
            .await
    }

    /// Metadata, then status, then the Epic auto-completion a status write
    /// may trigger, in one transaction. See `ticket_updates` for each write.
    async fn update_ticket(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        metadata: TicketMetadataUpdate,
        status: Option<TicketStatus>,
    ) -> Result<Ticket, OrderError> {
        if let Some(status) = status {
            super::combine::guard_merged_status(&self.pool, workspace_id, ticket_id, status)
                .await?;
        }
        let now = crate::db_now();
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        super::ticket_updates::write_ticket_metadata(
            &mut tx,
            workspace_id,
            ticket_id,
            metadata,
            now,
        )
        .await?;
        if let Some(status) = status {
            super::ticket_updates::write_ticket_status(
                &mut tx,
                workspace_id,
                ticket_id,
                status,
                now,
            )
            .await?;
            super::epic_completion::complete_epic_if_finished(
                &mut tx,
                workspace_id,
                ticket_id,
                now,
            )
            .await?;
        }
        tx.commit().await.map_err(map_error)?;
        self.get_ticket(workspace_id, ticket_id).await
    }
}

impl PgOrderRepository {
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
}
