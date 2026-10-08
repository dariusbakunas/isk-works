use super::connections::parse_esi_expiry;
use super::*;

impl PgEsiRepository {
    pub async fn start_sync(
        &self,
        connection: &ConnectedCharacter,
        kind: EsiSyncKind,
    ) -> Result<EsiSyncRun, InventoryError> {
        let run = EsiSyncRun {
            id: EsiSyncRunId::new(),
            connection_id: connection.id,
            requested_kind: kind,
            status: EsiSyncStatus::Running,
            phase: "Refreshing authorization".to_string(),
            started_at: crate::db_now(),
            completed_at: None,
            cache_expires_at: None,
            imported_count: 0,
            unchanged_count: 0,
            skipped_count: 0,
            error_count: 0,
            error_code: None,
            summary: String::new(),
        };
        sqlx::query(
            r#"
            INSERT INTO esi_sync_runs (
              id, workspace_id, owner_id, connection_id, requested_kind, status,
              phase, started_at
            ) VALUES ($1,$2,$3,$4,$5,'running',$6,$7)
            "#,
        )
        .bind(run.id.0)
        .bind(connection.workspace_id.0)
        .bind(connection.owner_id.0)
        .bind(connection.id.0)
        .bind(sync_kind_to_str(kind))
        .bind(&run.phase)
        .bind(run.started_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(run)
    }

    pub async fn fail_sync(
        &self,
        run_id: EsiSyncRunId,
        code: &str,
        summary: &str,
    ) -> Result<EsiSyncRun, InventoryError> {
        sqlx::query(
            "UPDATE esi_sync_runs SET status = 'failed', phase = 'Failed', completed_at = now(), error_count = error_count + 1, error_code = $2, summary = $3 WHERE id = $1",
        )
        .bind(run_id.0)
        .bind(code)
        .bind(summary)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        self.get_sync_run(run_id).await
    }

    pub async fn record_sync_metadata(
        &self,
        run_id: EsiSyncRunId,
        metadata: &iskworks_esi::EsiResponseMetadata,
    ) -> Result<(), InventoryError> {
        sqlx::query("UPDATE esi_sync_runs SET rate_limit_metadata = $2 WHERE id = $1")
            .bind(run_id.0)
            .bind(serde_json::to_value(metadata).map_err(map_json)?)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }

    pub async fn checkpoint_etag(
        &self,
        connection_id: ConnectedCharacterId,
        endpoint_kind: &str,
    ) -> Result<Option<String>, InventoryError> {
        sqlx::query_scalar(
            "SELECT etag FROM esi_sync_checkpoints WHERE connection_id = $1 AND endpoint_kind = $2",
        )
        .bind(connection_id.0)
        .bind(endpoint_kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)
        .map(Option::flatten)
    }

    pub async fn complete_unchanged_sync(
        &self,
        run: &EsiSyncRun,
        endpoint_kind: &str,
        summary: &str,
    ) -> Result<EsiSyncRun, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        sqlx::query(
            "UPDATE esi_sync_checkpoints SET last_successful_sync_at = now() WHERE connection_id = $1 AND endpoint_kind = $2",
        )
        .bind(run.connection_id.0)
        .bind(endpoint_kind)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        finalize_run(&mut tx, run.id, 0, 1, summary).await?;
        tx.commit().await.map_err(map_sqlx)?;
        self.get_sync_run(run.id).await
    }

    pub async fn get_sync_run(&self, id: EsiSyncRunId) -> Result<EsiSyncRun, InventoryError> {
        sqlx::query_as::<_, SyncRunRow>(
            "SELECT id, connection_id, requested_kind, status, phase, started_at, completed_at, rate_limit_metadata, imported_count, unchanged_count, skipped_count, error_count, error_code, summary FROM esi_sync_runs WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?
        .into_domain()
    }

    pub async fn list_sync_runs(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<EsiSyncRun>, InventoryError> {
        sqlx::query_as::<_, SyncRunRow>(
            "SELECT id, connection_id, requested_kind, status, phase, started_at, completed_at, rate_limit_metadata, imported_count, unchanged_count, skipped_count, error_count, error_code, summary FROM esi_sync_runs WHERE workspace_id = $1 ORDER BY started_at DESC LIMIT 50",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(SyncRunRow::into_domain)
        .collect()
    }
}

pub(super) async fn finalize_run(
    tx: &mut Transaction<'_, Postgres>,
    id: EsiSyncRunId,
    imported: u64,
    unchanged: u64,
    summary: &str,
) -> Result<(), InventoryError> {
    sqlx::query(
        "UPDATE esi_sync_runs SET status = 'succeeded', phase = 'Complete', completed_at = now(), imported_count = $2, unchanged_count = $3, summary = $4 WHERE id = $1",
    )
    .bind(id.0)
    .bind(i64::try_from(imported).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(i64::try_from(unchanged).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(summary)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

pub(super) fn checksum(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[derive(sqlx::FromRow)]
pub(super) struct SyncRunRow {
    id: Uuid,
    connection_id: Uuid,
    requested_kind: String,
    status: String,
    phase: String,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    rate_limit_metadata: serde_json::Value,
    imported_count: i64,
    unchanged_count: i64,
    skipped_count: i64,
    error_count: i64,
    error_code: Option<String>,
    summary: String,
}

impl SyncRunRow {
    pub(super) fn into_domain(self) -> Result<EsiSyncRun, InventoryError> {
        Ok(EsiSyncRun {
            id: EsiSyncRunId(self.id),
            connection_id: ConnectedCharacterId(self.connection_id),
            requested_kind: sync_kind_from_str(&self.requested_kind)?,
            status: sync_status_from_str(&self.status)?,
            phase: self.phase,
            started_at: self.started_at,
            completed_at: self.completed_at,
            cache_expires_at: self
                .rate_limit_metadata
                .get("expires")
                .and_then(serde_json::Value::as_str)
                .and_then(parse_esi_expiry),
            imported_count: to_u64(self.imported_count)?,
            unchanged_count: to_u64(self.unchanged_count)?,
            skipped_count: to_u64(self.skipped_count)?,
            error_count: to_u64(self.error_count)?,
            error_code: self.error_code,
            summary: self.summary,
        })
    }
}

pub(super) fn sync_kind_to_str(value: EsiSyncKind) -> &'static str {
    match value {
        EsiSyncKind::Assets => "assets",
        EsiSyncKind::WalletTransactions => "wallet_transactions",
        EsiSyncKind::AllSupported => "all_supported",
    }
}

pub(super) fn sync_kind_from_str(value: &str) -> Result<EsiSyncKind, InventoryError> {
    match value {
        "assets" => Ok(EsiSyncKind::Assets),
        "wallet_transactions" => Ok(EsiSyncKind::WalletTransactions),
        "all_supported" => Ok(EsiSyncKind::AllSupported),
        _ => Err(invalid_projection("sync kind")),
    }
}

pub(super) fn sync_status_from_str(value: &str) -> Result<EsiSyncStatus, InventoryError> {
    match value {
        "pending" => Ok(EsiSyncStatus::Pending),
        "running" => Ok(EsiSyncStatus::Running),
        "succeeded" => Ok(EsiSyncStatus::Succeeded),
        "partially_succeeded" => Ok(EsiSyncStatus::PartiallySucceeded),
        "failed" => Ok(EsiSyncStatus::Failed),
        _ => Err(invalid_projection("sync status")),
    }
}
