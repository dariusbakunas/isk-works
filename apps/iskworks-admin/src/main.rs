//! Minimal operator tooling for the invite-only alpha. No web UI, no
//! server — just enough to mint, list, and disable invite codes against the
//! same database the API uses. See `docs/security/invite-only-alpha.md`.
//!
//! `DATABASE_URL` must point at the ISK Works database. Migrations are run
//! on startup (idempotent) so this works even before the API's first boot.

use std::env;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use iskworks_core::{generate_invite_code, hash_invite_code_from_raw, validate_new_invite};
use iskworks_storage::{NewInvite, PgInviteRepository};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Parser)]
#[command(
    name = "iskworks-admin",
    about = "ISK Works operator tooling (invite codes)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Invite code operations.
    #[command(subcommand)]
    Invite(InviteCommand),
}

#[derive(Debug, Subcommand)]
enum InviteCommand {
    /// Mint a new invite code. The plaintext is printed exactly once here
    /// and then only its hash exists in the database — it cannot be
    /// recovered later.
    Create {
        /// How many times the code may be redeemed (default 1).
        #[arg(long, default_value_t = 1)]
        max_uses: i32,
        /// Optional expiry, RFC 3339 (e.g. 2026-10-01T00:00:00Z).
        #[arg(long)]
        expires_at: Option<DateTime<Utc>>,
        /// Optional free-text note for your own bookkeeping.
        #[arg(long)]
        note: Option<String>,
    },
    /// List every invite (id, timestamps, usage, note) — never the code.
    List,
    /// Disable an invite so it can no longer be redeemed. The row is kept
    /// for audit; already-disabled invites are left as they are.
    Disable {
        /// The invite id from `invite list`.
        id: Uuid,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required (point it at the ISK Works database)")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    MIGRATOR.run(&pool).await?;
    let invites = PgInviteRepository::new(pool);

    match cli.command {
        Command::Invite(InviteCommand::Create {
            max_uses,
            expires_at,
            note,
        }) => {
            let note = validate_new_invite(max_uses, expires_at, note, Utc::now())
                .map_err(|error| error.message)?;
            let code = generate_invite_code();
            let id = invites
                .create_invite(NewInvite {
                    code_hash: hash_invite_code_from_raw(&code),
                    max_uses,
                    expires_at,
                    note,
                    // The CLI has no encryption key; CLI invites are not revealable.
                    code_ciphertext: None,
                })
                .await?;
            let id = id.0;
            println!("Invite created.");
            println!("  id:        {id}");
            println!(
                "  code:      {code}   <- copy now; it is not stored and cannot be shown again"
            );
            println!("  max_uses:  {max_uses}");
            match expires_at {
                Some(at) => println!("  expires:   {at}"),
                None => println!("  expires:   never"),
            }
        }
        Command::Invite(InviteCommand::List) => {
            let rows = invites.list_invites().await?;
            if rows.is_empty() {
                println!("No invites.");
                return Ok(());
            }
            println!(
                "{:<38}  {:<20}  {:<20}  {:>9}  {:<9}  note",
                "id", "created", "expires", "uses", "status"
            );
            for row in rows {
                let status = row.status(Utc::now()).as_str();
                let id = row.id.0;
                let created = row.created_at.format("%Y-%m-%d %H:%M:%SZ");
                let expires = row
                    .expires_at
                    .map(|at| at.format("%Y-%m-%d %H:%M:%SZ").to_string())
                    .unwrap_or_else(|| "never".to_string());
                let uses = format!("{}/{}", row.use_count, row.max_uses);
                let note = row.note.unwrap_or_default();
                println!("{id:<38}  {created:<20}  {expires:<20}  {uses:>9}  {status:<9}  {note}");
            }
        }
        Command::Invite(InviteCommand::Disable { id }) => {
            let existed = invites.disable_invite(iskworks_core::InviteId(id)).await?;
            if existed {
                println!("Invite {id} disabled.");
            } else {
                return Err(format!("no invite with id {id}").into());
            }
        }
    }

    Ok(())
}
