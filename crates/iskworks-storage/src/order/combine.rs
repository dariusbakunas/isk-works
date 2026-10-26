//! Combining production tickets (and, later, splitting them): the
//! transactional side of `iskworks_core::order::combine`.

use super::*;

use std::collections::BTreeSet;

use iskworks_core::order::{check_combinable, combined_ticket, CombineCandidate, CombineMember};

use super::inserts::insert_ticket_with_prerequisites;

/// Locks and loads `ticket_ids` (`FOR UPDATE`, id order), with their
/// operations. Any id missing from the workspace is `TicketNotFound`.
async fn lock_tickets(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    ticket_ids: &[Uuid],
) -> Result<HashMap<TicketId, Ticket>, OrderError> {
    let rows: Vec<TicketRow> = sqlx::query_as(&format!(
        "SELECT {TICKET_COLUMNS} FROM tickets \
         WHERE workspace_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE"
    ))
    .bind(workspace_id.0)
    .bind(ticket_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    let distinct: BTreeSet<&Uuid> = ticket_ids.iter().collect();
    if rows.len() != distinct.len() {
        return Err(OrderError::TicketNotFound);
    }
    let tickets = rows
        .into_iter()
        .map(TicketRow::into_ticket)
        .collect::<Result<Vec<_>, _>>()?;
    let tickets = super::reservations::with_operations(&mut **tx, tickets).await?;
    Ok(tickets
        .into_iter()
        .map(|ticket| (ticket.id, ticket))
        .collect())
}

/// The facts `check_combinable` needs beyond the ticket row.
struct CandidateFacts {
    legacy_epic: bool,
    root_operation: bool,
    has_live_recording: bool,
    has_linked_jobs: bool,
}

async fn candidate_facts(
    tx: &mut Transaction<'_, Postgres>,
    ticket: &Ticket,
) -> Result<CandidateFacts, OrderError> {
    let (legacy_epic, root_operation) = match ticket.operations.as_slice() {
        [operation] => {
            let row: Option<(i16, Option<bool>)> = sqlx::query_as(
                "SELECT o.planning_snapshot_version, p.parent_occurrence_key IS NULL \
                 FROM orders o LEFT JOIN order_plan_operations p \
                   ON p.order_id = o.id AND p.occurrence_key = $2 \
                 WHERE o.id = $1",
            )
            .bind(operation.order_id.0)
            .bind(&operation.occurrence_key)
            .fetch_optional(&mut **tx)
            .await
            .map_err(map_error)?;
            match row {
                Some((version, root)) => (version < 3, root.unwrap_or(true)),
                None => (false, true),
            }
        }
        _ => (false, false),
    };
    let (has_live_recording, has_linked_jobs): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM ticket_inventory_recordings \
                        WHERE ticket_id = $1 AND reverted_at IS NULL), \
                EXISTS (SELECT 1 FROM ticket_industry_jobs WHERE ticket_id = $1)",
    )
    .bind(ticket.id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(CandidateFacts {
        legacy_epic,
        root_operation,
        has_live_recording,
        has_linked_jobs,
    })
}

async fn prerequisites_of(
    tx: &mut Transaction<'_, Postgres>,
    ticket_ids: &[Uuid],
) -> Result<HashMap<TicketId, Vec<TicketPrerequisite>>, OrderError> {
    let rows: Vec<TicketPrerequisiteRow> = sqlx::query_as(&format!(
        "SELECT {TICKET_PREREQUISITE_COLUMNS} FROM ticket_prerequisites \
         WHERE ticket_id = ANY($1) ORDER BY ticket_id, type_id, id"
    ))
    .bind(ticket_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    let mut by_ticket: HashMap<TicketId, Vec<TicketPrerequisite>> = HashMap::new();
    for row in rows {
        let prerequisite = row.into_prerequisite()?;
        by_ticket
            .entry(prerequisite.ticket_id)
            .or_default()
            .push(prerequisite);
    }
    Ok(by_ticket)
}

/// See `OrderRepository::combine_tickets`.
pub(super) async fn combine_tickets(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    ticket_ids: &[TicketId],
) -> Result<TicketId, OrderError> {
    let ids: Vec<Uuid> = ticket_ids.iter().map(|id| id.0).collect();
    let mut tx = pool.begin().await.map_err(map_error)?;
    let tickets = lock_tickets(&mut tx, workspace_id, &ids).await?;
    // Claims are taken under the same lock order as `claim_operation`.
    sqlx::query(
        "SELECT 1 FROM order_operation_ticket_claims WHERE ticket_id = ANY($1) \
         ORDER BY order_id, occurrence_key FOR UPDATE",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    let mut facts = Vec::with_capacity(ticket_ids.len());
    for ticket_id in ticket_ids {
        facts.push(candidate_facts(&mut tx, &tickets[ticket_id]).await?);
    }
    let candidates: Vec<CombineCandidate<'_>> = ticket_ids
        .iter()
        .zip(&facts)
        .map(|(ticket_id, facts)| CombineCandidate {
            ticket: &tickets[ticket_id],
            legacy_epic: facts.legacy_epic,
            root_operation: facts.root_operation,
            has_live_recording: facts.has_live_recording,
            has_linked_jobs: facts.has_linked_jobs,
        })
        .collect();
    check_combinable(&candidates)?;

    let prerequisites = prerequisites_of(&mut tx, &ids).await?;
    let members: Vec<CombineMember<'_>> = ticket_ids
        .iter()
        .map(|ticket_id| CombineMember {
            ticket: &tickets[ticket_id],
            prerequisites: prerequisites
                .get(ticket_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        })
        .collect();
    let new_ticket = combined_ticket(&members)?;
    // Each copy's original, in the same order `combined_ticket` copies them.
    let copied_from: Vec<(Uuid, Uuid)> = members
        .iter()
        .flat_map(|member| member.prerequisites.iter())
        .zip(&new_ticket.prerequisites)
        .map(|(original, copy)| (original.id.0, copy.id.0))
        .collect();
    let now = crate::db_now();
    let (combined, _) = insert_ticket_with_prerequisites(&mut tx, new_ticket, now).await?;

    sqlx::query(
        "UPDATE order_operation_ticket_claims SET ticket_id = $1, claimed_at = $2 \
         WHERE ticket_id = ANY($3)",
    )
    .bind(combined.id.0)
    .bind(now)
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    // Links naming a member move to the combined ticket as new rows; the
    // members' own rows stay, ignored while the member is canceled, so a
    // split restores them as they were.
    sqlx::query(
        "INSERT INTO order_requirement_fulfillments \
           (id, order_requirement_id, ticket_id, allocated_quantity, linked_at) \
         SELECT gen_random_uuid(), order_requirement_id, $1, allocated_quantity, $2 \
         FROM order_requirement_fulfillments WHERE ticket_id = ANY($3)",
    )
    .bind(combined.id.0)
    .bind(now)
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;
    sqlx::query(
        "INSERT INTO ticket_prerequisite_fulfillments \
           (id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at) \
         SELECT gen_random_uuid(), ticket_prerequisite_id, $1, allocated_quantity, $2 \
         FROM ticket_prerequisite_fulfillments WHERE fulfilling_ticket_id = ANY($3)",
    )
    .bind(combined.id.0)
    .bind(now)
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;
    let (originals, copies): (Vec<Uuid>, Vec<Uuid>) = copied_from.into_iter().unzip();
    sqlx::query(
        "INSERT INTO ticket_prerequisite_fulfillments \
           (id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at) \
         SELECT gen_random_uuid(), m.copy_id, f.fulfilling_ticket_id, f.allocated_quantity, $3 \
         FROM ticket_prerequisite_fulfillments f \
         JOIN UNNEST($1::uuid[], $2::uuid[]) AS m(original_id, copy_id) \
           ON m.original_id = f.ticket_prerequisite_id",
    )
    .bind(&originals)
    .bind(&copies)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;

    sqlx::query(
        "UPDATE tickets SET status = 'canceled', completed_at = NULL, \
           merged_into_ticket_id = $1, updated_at = $2 \
         WHERE id = ANY($3)",
    )
    .bind(combined.id.0)
    .bind(now)
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;
    sqlx::query("UPDATE tickets SET parent_ticket_id = $1 WHERE parent_ticket_id = ANY($2)")
        .bind(combined.id.0)
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

    tx.commit().await.map_err(map_error)?;
    Ok(combined.id)
}
