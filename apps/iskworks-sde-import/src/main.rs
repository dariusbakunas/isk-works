use std::env;
use std::path::PathBuf;

use clap::Parser;
use iskworks_sde::{ImportOutcome, ProgressEvent, ProgressReporter, SdeImporter};
use iskworks_storage::PgSdeRepository;
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Parser)]
#[command(
    name = "iskworks-sde-import",
    about = "Import a local CCP JSONL SDE zip into ISK Works"
)]
struct Args {
    /// Path to a locally downloaded CCP JSONL SDE zip archive.
    #[arg(long)]
    path: PathBuf,

    /// Re-import and activate a fresh dataset even if this archive's checksum
    /// already matches the active import (e.g. an older import predates a
    /// data category this importer knows how to parse).
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Clone, Copy)]
struct ConsoleProgress;

impl ProgressReporter for ConsoleProgress {
    fn report(&self, event: ProgressEvent) {
        match (event.completed, event.total) {
            (Some(completed), Some(total)) => {
                println!("{} ({completed}/{total})", event.message);
            }
            _ => println!("{}", event.message),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required; use `make sde-import SDE_PATH=/path/to/sde.zip` for local development")?;
    let repository = PgSdeRepository::connect(&database_url).await?;
    MIGRATOR.run(repository.pool()).await?;

    let outcome = SdeImporter::new(&repository, &ConsoleProgress)
        .import_zip(&args.path, args.force)
        .await?;
    // An already-active import skips the write path, so make sure it has its
    // analytics categories too.
    if let Some(rows) = repository.ensure_active_type_categories().await? {
        println!("Built analytics categories for {rows} types");
    }

    match outcome {
        ImportOutcome::Imported {
            import_id,
            source_version,
            counts,
        } => println!(
            "Activated SDE {source_version} as {import_id}: {} types, {} blueprints, {} material lines, {} product lines, {} skipped legacy/incomplete blueprints, {} reaction formulas, {} reaction material lines, {} reaction product lines, {} skipped reaction formulas",
            counts.types,
            counts.blueprints,
            counts.material_lines,
            counts.product_lines,
            counts.skipped_blueprints,
            counts.reaction_formulas,
            counts.reaction_material_lines,
            counts.reaction_product_lines,
            counts.skipped_reaction_formulas
        ),
        ImportOutcome::AlreadyActive {
            import_id,
            source_version,
            counts,
        } => println!(
            "SDE {source_version} is already active as {import_id}: {} types, {} blueprints, {} reaction formulas",
            counts.types, counts.blueprints, counts.reaction_formulas
        ),
    }

    Ok(())
}
