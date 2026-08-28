use forgedb_storage_native::{ColumnMetadata, ColumnType, DirLock, FixedColumn, Manifest};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
fn manifest_save_and_reload_compiles() -> Result<(), std::io::Error> {
    let manifest = Manifest {
        schema_version: 1,
        engine_version: 1,
        row_count: 42,
        columns: vec![
            ColumnMetadata {
                name: "id".to_string(),
                column_type: ColumnType::U64,
                column_index: 0,
                ..Default::default()
            },
            ColumnMetadata {
                name: "email".to_string(),
                column_type: ColumnType::String,
                column_index: 1,
                ..Default::default()
            },
        ],
        wal_enabled: false,
        last_checkpoint: 0,
        compaction_epoch: 0,
        row_anchor: None,
        auto_sequences: Default::default(),
    };
    manifest.save_to(&PathBuf::from("./mydb/manifest.json"))?;
    let reopened = Manifest::load_from(&PathBuf::from("./mydb/manifest.json"))?;
    assert_eq!(reopened.row_count, 42);
    Ok(())
}

#[allow(dead_code)]
fn fixed_column_append_and_read_compiles() -> Result<(), std::io::Error> {
    let mut id_column = FixedColumn::new(PathBuf::from("./data/fixed/u64_0.bin"), 8)?;
    id_column.append_u64(1001)?;
    let id = id_column.read_u64(0)?;
    assert_eq!(id, 1001);
    id_column.flush()?;
    Ok(())
}

#[allow(dead_code)]
fn dir_lock_is_released_on_drop_compiles() -> Result<(), std::io::Error> {
    let lock = DirLock::acquire(Path::new("./data"))?;
    drop(lock);
    Ok(())
}

#[allow(dead_code)]
fn dir_lock_contention_is_would_block_compiles() -> Result<(), std::io::Error> {
    match DirLock::acquire(Path::new("./data")) {
        Ok(_lock) => {}
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            eprintln!("error: another ForgeDB writer already has this data directory open");
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
