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
        engine_version: 1,
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
        row_anchor: Some(RowAnchor {
            relative_path: "tombstones.bin".into(),
            bytes_per_row: 1,
        }),
        auto_sequences: Default::default(),
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

/// Grow a `user` model dir to `committed` rows within the *same* epoch (append
/// past the previous anchor), preserving the given `epoch`.
fn grow_model_dir(root: &Path, committed: usize, epoch: u64) {
    let dir = root.join("user");
    write_fixed(&dir.join("fixed/u64_0.bin"), committed, 8);
    let names: Vec<String> = (0..committed).map(|i| format!("name-{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    write_variable(&dir, 1, &name_refs);
    fs::write(dir.join("tombstones.bin"), vec![0u8; committed]).unwrap();

    let manifest = Manifest {
        schema_version: 1,
        engine_version: 1,
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
        compaction_epoch: epoch,
        row_anchor: Some(RowAnchor {
            relative_path: "tombstones.bin".into(),
            bytes_per_row: 1,
        }),
        auto_sequences: Default::default(),
    };
    manifest.save_to(&dir.join("manifest.json")).unwrap();
}

#[test]
fn incremental_ships_only_the_tail_and_chain_restores_all_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    // Base: 3 committed rows at epoch 3.
    build_model_dir(&data_dir, 3, 0);

    let base = tmp.path().join("base.forgebk");
    let base_sum = forgedb_backup::create(&data_dir, &base).unwrap();
    assert_eq!(base_sum.models[0].row_count, 3);

    // Append 2 more rows in the same epoch, then take an incremental.
    grow_model_dir(&data_dir, 5, 3);
    let delta = tmp.path().join("delta.forgebk");
    let delta_sum = forgedb_backup::create_incremental(&data_dir, &base, &delta).unwrap();
    assert_eq!(delta_sum.models[0].row_count, 5);

    // The delta payload is far smaller than the base (only the 2-row tail).
    assert!(
        delta_sum.total_bytes < base_sum.total_bytes,
        "incremental payload ({}) must be smaller than the full base ({})",
        delta_sum.total_bytes,
        base_sum.total_bytes
    );

    let dh = forgedb_backup::read_header(&delta).unwrap();
    assert!(dh.is_incremental());
    assert_eq!(dh.sequence, 1);
    assert_eq!(dh.chain_id, forgedb_backup::read_header(&base).unwrap().chain_id);

    // Restore the chain (base + delta) into a fresh dir.
    let restored = tmp.path().join("restored");
    let r = forgedb_backup::restore_chain(
        &[base.clone(), delta.clone()],
        &restored,
        false,
    )
    .unwrap();
    assert_eq!(r.models[0].row_count, 5);

    // id column: exactly 5 rows * 8 bytes (base 3 + delta 2).
    let id_bytes = fs::read(restored.join("user/fixed/u64_0.bin")).unwrap();
    assert_eq!(id_bytes.len(), 5 * 8);
    // tombstones: 5 bytes.
    assert_eq!(fs::read(restored.join("user/tombstones.bin")).unwrap().len(), 5);
    // name data: all 5 committed strings concatenated.
    let expected: String = (0..5).map(|i| format!("name-{i}")).collect();
    assert_eq!(
        fs::read(restored.join("user/variable/string_data_1.bin")).unwrap(),
        expected.as_bytes()
    );
    assert_eq!(
        fs::read(restored.join("user/variable/string_offsets_1.bin")).unwrap().len(),
        5 * 16
    );
}

#[test]
fn incremental_refused_when_epoch_bumped() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    // Base at epoch 3.
    build_model_dir(&data_dir, 3, 0);
    let base = tmp.path().join("base.forgebk");
    forgedb_backup::create(&data_dir, &base).unwrap();

    // Simulate a compaction: epoch bumps 3 -> 4 (row bytes conceptually moved).
    grow_model_dir(&data_dir, 4, 4);

    let delta = tmp.path().join("delta.forgebk");
    let err = forgedb_backup::create_incremental(&data_dir, &base, &delta).unwrap_err();
    match err {
        forgedb_backup::BackupError::Layout(msg) => {
            assert!(
                msg.contains("epoch") && msg.contains("fresh full backup"),
                "epoch-mismatch message should guide to a fresh full backup: {msg}"
            );
        }
        other => panic!("expected a Layout epoch error, got {other:?}"),
    }
    assert!(!delta.exists(), "no delta archive is written on refusal");
}

#[test]
fn restore_chain_rejects_out_of_order_or_wrong_base() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    build_model_dir(&data_dir, 2, 0);
    let base = tmp.path().join("base.forgebk");
    forgedb_backup::create(&data_dir, &base).unwrap();
    grow_model_dir(&data_dir, 4, 3);
    let delta = tmp.path().join("delta.forgebk");
    forgedb_backup::create_incremental(&data_dir, &base, &delta).unwrap();

    // A delta first (no base) is refused: the chain must start at sequence 0.
    let out = tmp.path().join("out1");
    let err = forgedb_backup::restore_chain(&[delta.clone()], &out, false).unwrap_err();
    assert!(matches!(err, forgedb_backup::BackupError::Format(_)));

    // Delta before base (out of order) is refused.
    let out2 = tmp.path().join("out2");
    let err2 = forgedb_backup::restore_chain(&[delta.clone(), base.clone()], &out2, false)
        .unwrap_err();
    assert!(matches!(err2, forgedb_backup::BackupError::Format(_)));
}

#[test]
fn full_restore_of_incremental_alone_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    build_model_dir(&data_dir, 2, 0);
    let base = tmp.path().join("base.forgebk");
    forgedb_backup::create(&data_dir, &base).unwrap();
    grow_model_dir(&data_dir, 3, 3);
    let delta = tmp.path().join("delta.forgebk");
    forgedb_backup::create_incremental(&data_dir, &base, &delta).unwrap();

    // The single-archive `restore` entry point on a delta must refuse (it is
    // not a full base).
    let out = tmp.path().join("out");
    let err = forgedb_backup::restore(&delta, &out, false).unwrap_err();
    assert!(matches!(err, forgedb_backup::BackupError::Format(_)));
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

/// Build one durable-broker frame in the exact `forgedb-changefeed::durable`
/// on-disk framing: `[4: total_len][payload][4: crc]`, payload leads with the
/// 8-byte offset. We only need the offset to be readable, so the rest of the
/// payload is arbitrary opaque bytes.
fn broker_frame(offset: u64, opaque: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(opaque);
    let crc = crc32fast::hash(&payload);
    let total_len = payload.len() + 4;
    let mut frame = Vec::new();
    frame.extend_from_slice(&(total_len as u32).to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

#[test]
fn backup_captures_broker_end_offset_for_pitr() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    build_model_dir(&data_dir, 3, 0);

    // No log yet -> base_offset 0.
    let a0 = tmp.path().join("s0.forgebk");
    forgedb_backup::create(&data_dir, &a0).unwrap();
    assert_eq!(
        forgedb_backup::read_header(&a0).unwrap().base_offset,
        0,
        "absent broker log => base_offset 0"
    );

    // Write a broker log with frames at offsets 1..=5.
    let mut log = Vec::new();
    for off in 1..=5u64 {
        log.extend_from_slice(&broker_frame(off, b"opaque-row-bytes"));
    }
    fs::write(data_dir.join("_replication.log"), &log).unwrap();

    let a5 = tmp.path().join("s5.forgebk");
    forgedb_backup::create(&data_dir, &a5).unwrap();
    assert_eq!(
        forgedb_backup::read_header(&a5).unwrap().base_offset,
        5,
        "base_offset is the highest committed broker offset"
    );

    // A torn tail (a half-written 6th frame) must be ignored: the offset stays 5.
    let mut torn = log.clone();
    torn.extend_from_slice(&[0xff, 0x00]); // 2 stray bytes, not a full length prefix
    fs::write(data_dir.join("_replication.log"), &torn).unwrap();
    let a5b = tmp.path().join("s5b.forgebk");
    forgedb_backup::create(&data_dir, &a5b).unwrap();
    assert_eq!(
        forgedb_backup::read_header(&a5b).unwrap().base_offset,
        5,
        "a torn tail frame is excluded from base_offset"
    );

    // The _replication.log is a root file, not a model dir — never captured as a model.
    assert!(
        forgedb_backup::read_header(&a5)
            .unwrap()
            .models
            .iter()
            .all(|m| m.dir != "_replication.log"),
        "the broker log is not a model directory"
    );
}
