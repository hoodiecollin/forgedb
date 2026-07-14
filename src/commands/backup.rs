use crate::commands::compact::format_bytes;
use crate::error::CliError;
use std::path::PathBuf;

pub struct CreateOptions {
    pub data_dir: PathBuf,
    pub output: PathBuf,
    pub incremental: bool,
    pub base: Option<PathBuf>,
}

pub struct RestoreOptions {
    /// A single full archive, or a base followed by its incrementals in order.
    pub archives: Vec<PathBuf>,
    pub output: PathBuf,
    pub overwrite: bool,
}

pub struct ListOptions {
    pub archive: PathBuf,
    pub json: bool,
}

fn map_err(e: forgedb_backup::BackupError) -> CliError {
    CliError::Backup(e.to_string())
}

/// Create a snapshot archive of a database directory — a full lock-free
/// snapshot, or an incremental byte-tail delta against a base (`--incremental`).
pub fn create(opts: CreateOptions) -> Result<(), CliError> {
    let (summary, kind) = if opts.incremental {
        let base = opts.base.ok_or_else(|| {
            CliError::Backup(
                "--incremental requires --base <archive> (the base or prior delta to build on)"
                    .to_string(),
            )
        })?;
        (
            forgedb_backup::create_incremental(&opts.data_dir, &base, &opts.output)
                .map_err(map_err)?,
            "Incremental backup",
        )
    } else {
        (
            forgedb_backup::create(&opts.data_dir, &opts.output).map_err(map_err)?,
            "Backup",
        )
    };

    println!("\n✓ {} created: {}\n", kind, summary.archive_path.display());
    println!("{:<24} {:>12} {:>10}", "Model", "Rows", "Epoch");
    println!("{}", "=".repeat(48));
    for m in &summary.models {
        println!("{:<24} {:>12} {:>10}", m.dir, m.row_count, m.compaction_epoch);
    }
    println!("{}", "=".repeat(48));
    println!(
        "{} model(s), {} of {}",
        summary.models.len(),
        format_bytes(summary.total_bytes),
        if opts.incremental {
            "appended-tail data"
        } else {
            "committed data"
        }
    );
    Ok(())
}

/// Restore a snapshot archive (or a base + incremental chain) into a data
/// directory (atomic temp + rename). Pass the base first, then each delta.
pub fn restore(opts: RestoreOptions) -> Result<(), CliError> {
    let summary =
        forgedb_backup::restore_chain(&opts.archives, &opts.output, opts.overwrite)
            .map_err(map_err)?;

    println!("\n✓ Restored to: {}\n", summary.out_dir.display());
    if opts.archives.len() > 1 {
        println!(
            "Applied {} archive(s) (1 base + {} incremental).",
            opts.archives.len(),
            opts.archives.len() - 1
        );
    }
    println!("{:<24} {:>12}", "Model", "Rows");
    println!("{}", "=".repeat(38));
    for m in &summary.models {
        println!("{:<24} {:>12}", m.dir, m.row_count);
    }
    println!("{}", "=".repeat(38));
    println!(
        "{} model(s), {} restored",
        summary.models.len(),
        format_bytes(summary.total_bytes)
    );
    Ok(())
}

/// Inspect an archive's header without materializing it.
pub fn list(opts: ListOptions) -> Result<(), CliError> {
    let header = forgedb_backup::read_header(&opts.archive).map_err(map_err)?;

    if opts.json {
        let json = serde_json::to_string_pretty(&header)
            .map_err(|e| CliError::Backup(e.to_string()))?;
        println!("{}", json);
        return Ok(());
    }

    println!("\nArchive: {}", opts.archive.display());
    println!("Container version: {}", header.container_version);
    if header.is_incremental() {
        print!("Kind: incremental (chain sequence {}", header.sequence);
        if let Some(base) = &header.base {
            print!(", base sequence {}", base.sequence);
        }
        println!(")");
    } else {
        println!("Kind: full base (chain sequence 0)");
    }
    if !header.chain_id.is_empty() {
        println!("Chain id: {}", header.chain_id);
    }
    println!();
    println!(
        "{:<24} {:>10} {:>10} {:>8}",
        "Model", "Rows", "Epoch", "SchemaV"
    );
    println!("{}", "=".repeat(56));
    for m in &header.models {
        println!(
            "{:<24} {:>10} {:>10} {:>8}",
            m.dir, m.row_count, m.compaction_epoch, m.schema_version
        );
    }
    Ok(())
}
