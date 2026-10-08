use super::*;

// ─── Migration 202609050002_ticket_status_todo.sql ──────────────────────
// `#[sqlx::test]` applies every migration up front against an empty DB, so
// the migration's data `UPDATE` has nothing to act on and the CHECK
// constraint it installs is already the 4-state one by the time a test
// body runs. These tests seed rows shaped like they predate the migration
// (restoring the previous CHECK constraint so `blocked`/`ready` are
// insertable again), then replay the migration file verbatim and assert
// the organizational remap.

const TICKET_STATUS_TODO_MIGRATION: &str =
    include_str!("../../../../../migrations/202609050002_ticket_status_todo.sql");

/// Widens `tickets_status_check` to accept the legacy `blocked`/`ready`
/// values again (alongside the current four) so rows shaped like they
/// predate the migration can be seeded. Touches no data.
async fn allow_legacy_ticket_status_values(pool: &PgPool) {
    sqlx::raw_sql(
        "ALTER TABLE tickets DROP CONSTRAINT tickets_status_check; \
         ALTER TABLE tickets ADD CONSTRAINT tickets_status_check \
           CHECK (status IN ('todo', 'blocked', 'ready', 'in_progress', 'complete', 'canceled'));",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn ticket_status_todo_migration_remaps_blocked_and_ready_only(pool: PgPool) {
    let (workspace_id, owner_id, _import_id) = fixture_workspace(&pool).await;
    let repository = PgOrderRepository::new(pool.clone());

    // Five tickets, one per legacy status. They are created `todo`; we
    // rewrite the column directly below to simulate pre-migration state.
    let mut ids = Vec::new();
    for name in [
        "A blocked",
        "B ready",
        "C in_progress",
        "D complete",
        "E canceled",
    ] {
        let (ticket, _) = repository
            .create_ticket(generic_ticket(workspace_id, owner_id, name))
            .await
            .unwrap();
        ids.push(ticket.id.0);
    }

    allow_legacy_ticket_status_values(&pool).await;

    // A fixed timestamp well in the past so we can tell which rows the
    // migration's `updated_at = now()` touched.
    let baseline: DateTime<Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
    let workspace_updated_before: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM workspaces WHERE id = $1")
            .bind(workspace_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();

    for (id, legacy_status) in
        ids.iter()
            .zip(["blocked", "ready", "in_progress", "complete", "canceled"])
    {
        sqlx::query("UPDATE tickets SET status = $1, updated_at = $2 WHERE id = $3")
            .bind(legacy_status)
            .bind(baseline)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }

    sqlx::raw_sql(TICKET_STATUS_TODO_MIGRATION)
        .execute(&pool)
        .await
        .unwrap();

    let rows: Vec<(Uuid, String, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT id, status, updated_at, captured_name FROM tickets WHERE workspace_id = $1 ORDER BY captured_name",
    )
    .bind(workspace_id.0)
    .fetch_all(&pool)
    .await
    .unwrap();

    let by_name = |needle: &str| rows.iter().find(|r| r.3 == needle).unwrap();

    // blocked / ready both collapse to `todo`, and the migration stamps
    // their `updated_at`.
    let blocked = by_name("A blocked");
    assert_eq!(blocked.1, "todo");
    assert!(
        blocked.2 > baseline,
        "the migration bumped the remapped row's updated_at"
    );
    let ready = by_name("B ready");
    assert_eq!(ready.1, "todo");
    assert!(ready.2 > baseline);

    // Every other status is left exactly as it was, `updated_at` included.
    for (name, expected) in [
        ("C in_progress", "in_progress"),
        ("D complete", "complete"),
        ("E canceled", "canceled"),
    ] {
        let row = by_name(name);
        assert_eq!(row.1, expected, "{name} must be untouched");
        assert_eq!(row.2, baseline, "{name}'s updated_at must be untouched");
    }

    // Names (a stand-in for "no unrelated column changed") are intact.
    for name in [
        "A blocked",
        "B ready",
        "C in_progress",
        "D complete",
        "E canceled",
    ] {
        assert!(rows.iter().any(|r| r.3 == name));
    }

    // No `blocked` / `ready` rows survive anywhere.
    let leftover: i64 =
        sqlx::query_scalar("SELECT count(*) FROM tickets WHERE status IN ('blocked', 'ready')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(leftover, 0);

    // The migrated CHECK constraint is in force -- a legacy value is rejected.
    let (ticket, _) = repository
        .create_ticket(generic_ticket(workspace_id, owner_id, "F reject"))
        .await
        .unwrap();
    let rejected = sqlx::query("UPDATE tickets SET status = 'blocked' WHERE id = $1")
        .bind(ticket.id.0)
        .execute(&pool)
        .await;
    assert!(
        rejected.is_err(),
        "post-migration, 'blocked' must violate tickets_status_check"
    );

    // An unrelated table was not touched.
    let workspace_updated_after: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM workspaces WHERE id = $1")
            .bind(workspace_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(workspace_updated_before, workspace_updated_after);
}
