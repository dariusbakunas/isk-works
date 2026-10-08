use super::*;

#[derive(Clone)]
pub struct PgWorkspaceRepository {
    pool: PgPool,
}

impl PgWorkspaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self::new(pool))
    }
}

#[async_trait]
impl WorkspaceRepository for PgWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        let workspace_row = sqlx::query_as::<_, WorkspaceRow>(
            r#"
            SELECT id, display_name, owner_id, default_market_region_id,
                   default_market_location_id, created_at, updated_at
            FROM workspaces
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let Some(workspace_row) = workspace_row else {
            return Ok(WorkspaceState::unconfigured());
        };

        let owner_row = sqlx::query_as::<_, OwnerRow>(
            r#"
            SELECT id, workspace_id, owner_kind, display_name, eve_owner_id, hidden, created_at, updated_at
            FROM owners
            WHERE id = $1
            "#,
        )
        .bind(workspace_row.owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(WorkspaceState::configured(
            workspace_row.into_workspace(),
            owner_row.into_owner()?,
        ))
    }

    async fn get_workspace_state_by_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        let workspace_row = sqlx::query_as::<_, WorkspaceRow>(
            r#"
            SELECT id, display_name, owner_id, default_market_region_id,
                   default_market_location_id, created_at, updated_at
            FROM workspaces
            WHERE id = $1
            "#,
        )
        .bind(workspace_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let Some(workspace_row) = workspace_row else {
            return Ok(WorkspaceState::unconfigured());
        };

        let owner_row = sqlx::query_as::<_, OwnerRow>(
            r#"
            SELECT id, workspace_id, owner_kind, display_name, eve_owner_id, hidden, created_at, updated_at
            FROM owners
            WHERE id = $1
            "#,
        )
        .bind(workspace_row.owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(WorkspaceState::configured(
            workspace_row.into_workspace(),
            owner_row.into_owner()?,
        ))
    }

    async fn create_workspace(
        &self,
        new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workspaces")
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        if existing > 0 {
            return Err(AppError::WorkspaceAlreadyConfigured);
        }

        insert_workspace(&mut tx, &new_workspace.workspace)
            .await
            .map_err(map_create_error)?;
        insert_owner(&mut tx, &new_workspace.owner)
            .await
            .map_err(map_create_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(WorkspaceState::configured(
            new_workspace.workspace,
            new_workspace.owner,
        ))
    }
}

pub(crate) async fn insert_workspace(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &Workspace,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO workspaces (
          id, display_name, owner_id, default_market_region_id,
          default_market_location_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(workspace.id.0)
    .bind(&workspace.name)
    .bind(workspace.owner_id.0)
    .bind(workspace.default_market_region_id)
    .bind(workspace.default_market_location_id)
    .bind(workspace.created_at)
    .bind(workspace.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn insert_owner(
    tx: &mut Transaction<'_, Postgres>,
    owner: &Owner,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO owners (
          id, workspace_id, owner_kind, display_name, eve_owner_id, hidden, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(owner.id.0)
    .bind(owner.workspace_id.0)
    .bind(owner_kind_to_str(owner.kind))
    .bind(&owner.display_name)
    .bind(owner.eve_owner_id)
    .bind(owner.hidden)
    .bind(owner.created_at)
    .bind(owner.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: Uuid,
    display_name: String,
    owner_id: Uuid,
    default_market_region_id: Option<i64>,
    default_market_location_id: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl WorkspaceRow {
    fn into_workspace(self) -> Workspace {
        Workspace {
            id: WorkspaceId(self.id),
            name: self.display_name,
            owner_id: OwnerId(self.owner_id),
            default_market_region_id: self.default_market_region_id,
            default_market_location_id: self.default_market_location_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct OwnerRow {
    id: Uuid,
    workspace_id: Uuid,
    owner_kind: String,
    display_name: String,
    eve_owner_id: Option<i64>,
    hidden: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl OwnerRow {
    fn into_owner(self) -> Result<Owner, AppError> {
        Ok(Owner {
            id: OwnerId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            kind: owner_kind_from_str(&self.owner_kind)?,
            display_name: self.display_name,
            eve_owner_id: self.eve_owner_id,
            hidden: self.hidden,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn owner_kind_to_str(kind: OwnerKind) -> &'static str {
    match kind {
        OwnerKind::Manual => "manual",
    }
}

fn owner_kind_from_str(value: &str) -> Result<OwnerKind, AppError> {
    match value {
        "manual" => Ok(OwnerKind::Manual),
        other => Err(AppError::Persistence(format!("unknown owner kind {other}"))),
    }
}

fn map_sqlx_error(error: sqlx::Error) -> AppError {
    AppError::Persistence(error.to_string())
}

fn map_create_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return AppError::WorkspaceAlreadyConfigured;
        }
    }

    map_sqlx_error(error)
}
