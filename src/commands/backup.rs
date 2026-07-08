use crate::commands::compact::format_bytes;
use crate::error::CliError;
use std::path::PathBuf;

pub struct CreateOptions {
    pub data_dir: PathBuf,
    pub output: PathBuf,
}

pub struct RestoreOptions {
    pub archive: PathBuf,
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

/// Create a lock-free full-snapshot archive of a database directory.
pub fn create(opts: CreateOptions) -> Result<(), CliError> {
    let summary = forgedb_backup::create(&opts.data_dir, &opts.output).map_err(map_err)?;

    println!("\n✓ Backup created: {}\n", summary.archive_path.display());
    println!("{:<24} {:>12} {:>10}", "Model", "Rows", "Epoch");
    println!("{}", "=".repeat(48));
    for m in &summary.models {
        println!("{:<24} {:>12} {:>10}", m.dir, m.row_count, m.compaction_epoch);
    }
    println!("{}", "=".repeat(48));
    println!(
        "{} model(s), {} of committed data",
        summary.models.len(),
        format_bytes(summary.total_bytes)
    );
    Ok(())
}

/// Restore a snapshot archive into a data directory (atomic temp + rename).
pub fn restore(opts: RestoreOptions) -> Result<(), CliError> {
    let summary =
        forgedb_backup::restore(&opts.archive, &opts.output, opts.overwrite).map_err(map_err)?;

    println!("\n✓ Restored to: {}\n", summary.out_dir.display());
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
