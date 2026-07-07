//! Round-trip + concurrency proof for snapshot backup/restore over the
//! append-only layout, using a hand-built model directory (no codegen needed):
//! the substrate crate only ever sees `manifest.json` + opaque column bytes.

use std::fs;
use std::path::Path;

use forgedb_storage::{ColumnKind, ColumnMetadata, ColumnType, Manifest, RowAnchor};

/// Write a fixed column file with `rows` values of `value_size` bytes each,
/// value `i` filled with byte `i as u8`.
fn write_fixed(path: &Path, rows: usize, value_size: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = Vec::new();
    for i in 0..rows {
        bytes.extend(std::iter::repeat(i as u8).take(value_size));
    }
    fs::write(path, bytes).unwrap();
}

/// Write a variable column (data + offsets) for the given string rows.
fn write_variable(dir: &Path, index: usize, rows: &[&str]) {
    let var = dir.join("variable");
    fs::create_dir_all(&var).unwrap();
    let mut data = Vec::new();
    let mut offsets = Vec::new();
    for s in rows {
        let offset = data.len() as u64;
        let len = s.len() as u64;
        data.extend_from_slice(s.as_bytes());
        offsets.extend_from_slice(&offset.to_le_bytes());
        offsets.extend_from_slice(&len.to_le_bytes());
    }
    fs::write(var.join(format!("string_data_{index}.bin")), data).unwrap();
    fs::write(var.join(format!("string_offsets_{index}.bin")), offsets).unwrap();
}

/// Build a `user` model dir with `committed` rows recorded by the tombstone
/// anchor, but extra *uncommitted* bytes appended past the anchor (simulating a
/// concurrent writer mid-insert). Returns the data dir root.
fn build_model_dir(root: &Path, committed: usize, uncommitted_extra: usize) {
    let dir = root.join("user");
    fs::create_dir_all(&dir).unwrap();

    let total = committed + uncommitted_extra;

    // id: fixed u64 (8 bytes) — write MORE rows than committed.
    write_fixed(&dir.join("fixed/u64_0.bin"), total, 8);
    // name: variable string — write MORE rows than committed.
    let mut names: Vec<String> = (0..total).map(|i| format!("name-{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    write_variable(&dir, 1, &name_refs);
    names.clear();

    // tombstones: the anchor — exactly `committed` bytes (appended last).
    fs::write(dir.join("tombstones.bin"), vec![0u8; committed]).unwrap();

    let manifest = Manifest {
        schema_version: 1,
        row_count: committed,
        columns: vec![
            ColumnMetadata {
                name: "id".into(),
                column_type: ColumnType::U64,
                column_index: 0,
                value_size: 8,
                kind: ColumnKind::Fixed,
                relative_path: "fixed/u64_0.bin".into(),
            },
            ColumnMetadata {
                name: "name".into(),
                column_type: ColumnType::String,
                column_index: 1,
                value_size: 0,
                kind: ColumnKind::Variable,
                relative_path: "variable/string_data_1.bin".into(),
            },
        ],
        wal_enabled: false,
        last_checkpoint: 0,
        compaction_epoch: 3,
        format_version: 1,
        row_anchor: Some(RowAnchor {
            relative_path: "tombstones.bin".into(),
            bytes_per_row: 1,
        }),
    };
    manifest.save_to(&dir.join("manifest.json")).unwrap();
}

#[test]
fn snapshot_excludes_uncommitted_and_restores_committed_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    // 5 committed rows, 3 extra uncommitted rows sitting past the anchor.
    build_model_dir(&data_dir, 5, 3);

    let archive = tmp.path().join("snap.forgebk");
    let summary = forgedb_backup::create(&data_dir, &archive).unwrap();

    assert_eq!(summary.models.len(), 1);
    let m = &summary.models[0];
    assert_eq!(m.dir, "user");
    assert_eq!(m.row_count, 5, "anchor must report exactly the committed rows");
    assert_eq!(m.compaction_epoch, 3, "epoch travels in the archive");

    // Restore into a fresh dir.
    let restored = tmp.path().join("restored");
    let r = forgedb_backup::restore(&archive, &restored, false).unwrap();
    assert_eq!(r.models[0].row_count, 5);

    // id column: exactly 5 rows * 8 bytes — the 3 uncommitted rows are gone.
    let id_bytes = fs::read(restored.join("user/fixed/u64_0.bin")).unwrap();
    assert_eq!(id_bytes.len(), 5 * 8, "restored fixed column is the committed prefix");

    // tombstones: exactly 5 bytes.
    let tomb = fs::read(restored.join("user/tombstones.bin")).unwrap();
    assert_eq!(tomb.len(), 5);

    // name column: data = concat of the 5 committed strings; offsets = 5 * 16.
    let expected_data: String = (0..5).map(|i| format!("name-{i}")).collect();
    let name_data = fs::read(restored.join("user/variable/string_data_1.bin")).unwrap();
    assert_eq!(
        name_data,
        expected_data.as_bytes(),
        "restored variable data is exactly the committed rows' bytes, no bleed"
    );
    let offsets = fs::read(restored.join("user/variable/string_offsets_1.bin")).unwrap();
    assert_eq!(offsets.len(), 5 * 16);

    // The manifest travels verbatim so the restored dir is self-describing.
    assert!(restored.join("user/manifest.json").exists());
}

#[test]
fn restore_refuses_existing_nonempty_dir_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    build_model_dir(&data_dir, 2, 0);
    let archive = tmp.path().join("snap.forgebk");
    forgedb_backup::create(&data_dir, &archive).unwrap();

    let out = tmp.path().join("out");
    fs::create_dir_all(out.join("sub")).unwrap();
    fs::write(out.join("sub/x"), b"pre-existing").unwrap();

    let err = forgedb_backup::restore(&archive, &out, false).unwrap_err();
    assert!(matches!(err, forgedb_backup::BackupError::Layout(_)));

    // With overwrite it succeeds and replaces the prior contents.
    forgedb_backup::restore(&archive, &out, true).unwrap();
    assert!(!out.join("sub/x").exists(), "overwrite replaces the directory");
    assert!(out.join("user/manifest.json").exists());
}

#[test]
fn header_is_readable_without_materializing() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    build_model_dir(&data_dir, 4, 1);
    let archive = tmp.path().join("snap.forgebk");
    forgedb_backup::create(&data_dir, &archive).unwrap();

    let header = forgedb_backup::read_header(&archive).unwrap();
    assert_eq!(header.container_version, 1);
    assert_eq!(header.models[0].row_count, 4);
    // Bad magic is rejected.
    let junk = tmp.path().join("junk.bin");
    fs::write(&junk, b"not an archive at all").unwrap();
    assert!(forgedb_backup::read_header(&junk).is_err());
}
