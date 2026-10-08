use super::*;

pub(super) async fn lock_source(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    expected_revision: u64,
) -> Result<(), IndustryError> {
    let revision = sqlx::query_scalar::<_, i64>(
        "SELECT revision FROM price_sources WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_error)?
    .ok_or(IndustryError::PriceSourceNotFound)?;
    if revision != i64_from_u64(expected_revision)? {
        return Err(IndustryError::RevisionConflict);
    }
    Ok(())
}

pub(super) async fn bump_source(
    tx: &mut Transaction<'_, Postgres>,
    source_id: PriceSourceId,
) -> Result<(), IndustryError> {
    sqlx::query("UPDATE price_sources SET revision = revision + 1, updated_at = $1 WHERE id = $2")
        .bind(crate::db_now())
        .bind(source_id.0)
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    Ok(())
}

pub(super) async fn classify_source_result(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    source_id: PriceSourceId,
    rows_affected: u64,
) -> Result<(), IndustryError> {
    if rows_affected > 0 {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM price_sources WHERE workspace_id = $1 AND id = $2)",
    )
    .bind(workspace_id.0)
    .bind(source_id.0)
    .fetch_one(pool)
    .await
    .map_err(map_error)?;
    Err(if exists {
        IndustryError::RevisionConflict
    } else {
        IndustryError::PriceSourceNotFound
    })
}

#[derive(sqlx::FromRow)]
pub(super) struct PriceSourceRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) source_kind: String,
    pub(super) revision: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) item_count: i64,
    pub(super) recent_build_count: i64,
}

impl PriceSourceRow {
    pub(super) fn into_source(
        self,
        items: Vec<PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError> {
        Ok(PriceSource {
            id: PriceSourceId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            name: self.display_name,
            description: self.description,
            kind: match self.source_kind.as_str() {
                "manual" => PriceSourceKind::Manual,
                "eve_client_market_export" => PriceSourceKind::EveClientMarketExport,
                "esi_market_orders" => PriceSourceKind::EsiMarketOrders,
                other => {
                    return Err(IndustryError::Persistence(format!(
                        "unknown price source kind {other}"
                    )))
                }
            },
            revision: u64_from_i64(self.revision)?,
            item_count: u64_from_i64(self.item_count)?,
            recent_build_count: u64_from_i64(self.recent_build_count)?,
            items,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct PriceSourceItemRow {
    pub(super) type_id: i64,
    pub(super) captured_name: String,
    pub(super) price: Decimal,
    pub(super) note: String,
    pub(super) updated_at: DateTime<Utc>,
}

impl PriceSourceItemRow {
    pub(super) fn into_item(self) -> PriceSourceItem {
        PriceSourceItem {
            type_id: self.type_id,
            type_name: self.captured_name,
            price: Money(self.price),
            note: self.note,
            updated_at: self.updated_at,
        }
    }
}
