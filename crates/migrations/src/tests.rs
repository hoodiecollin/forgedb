use super::*;
use tempfile::TempDir;

#[test]
fn test_migration_generate_and_load_roundtrip() {
    // The record/track/load lifecycle (data-at-rest transformation now lives in
    // the offline transformer bin, #74 Phase 3 — not a runtime executor).
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");

    let changes = vec![
        SchemaChange::AddModel {
            model_name: "User".to_string(),
        },
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            field_type: "u32".parse().unwrap(),
            nullable: true,
            default_value: Some("0".to_string()),
        },
    ];

    let migration =
        MigrationGenerator::generate(&migrations_dir, "Create User model".to_string(), changes)
            .unwrap();
    assert_eq!(migration.changes.len(), 2);

    // Create tracker
    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    assert!(!tracker.is_applied(&migration.id));
    tracker
        .mark_applied(migration.id.clone(), migration.checksum.clone())
        .unwrap();
    assert!(tracker.is_applied(&migration.id));

    // Verify we can load migrations
    let all_migrations = MigrationGenerator::load_all_migrations(&migrations_dir).unwrap();
    assert_eq!(all_migrations.len(), 1);
    assert_eq!(all_migrations[0].id, migration.id);
}

#[test]
fn test_breaking_change_detection() {
    let changes = vec![
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "name".to_string(),
            field_type: "string".parse().unwrap(),
            nullable: false,
            default_value: None,
        },
        SchemaChange::RemoveField {
            model_name: "User".to_string(),
            field_name: "old_field".to_string(),
        },
        SchemaChange::ChangeFieldType {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            old_type: "u32".parse().unwrap(),
            new_type: "u64".parse().unwrap(),
        },
    ];

    let migration = Migration::new("Update User model".to_string(), changes);

    assert!(migration.has_breaking_changes());
    // M4: AddField { nullable: false, default_value: None } is also breaking now
    assert_eq!(migration.breaking_changes().len(), 3);
    assert_eq!(migration.safe_changes().len(), 0);
}

#[test]
fn test_migration_tracker_mark_and_rollback() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");

    let changes = vec![SchemaChange::AddModel {
        model_name: "TestModel".to_string(),
    }];

    let migration =
        MigrationGenerator::generate(&migrations_dir, "Add test model".to_string(), changes)
            .unwrap();

    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    tracker
        .mark_applied(migration.id.clone(), migration.checksum.clone())
        .unwrap();
    assert!(tracker.is_applied(&migration.id));

    tracker.mark_rolled_back().unwrap();
    assert!(!tracker.is_applied(&migration.id));
}

#[test]
fn test_schema_differ() {
    use diff::*;

    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "id".to_string(),
                ty: "uuid".parse().unwrap(),
                nullable: false,
                unique: true,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
                depends_on: vec![],
            }],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };

    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![
                SimpleField {
                    name: "id".to_string(),
                    ty: "uuid".parse().unwrap(),
                    nullable: false,
                    unique: true,
                    indexed: false,
                    index_type: "Hash".to_string(),
                    constraints: vec![],
                    depends_on: vec![],
                },
                SimpleField {
                    name: "email".to_string(),
                    ty: "string".parse().unwrap(),
                    nullable: false,
                    unique: true,
                    indexed: true,
                    index_type: "Hash".to_string(),
                    constraints: vec![SimpleConstraint {
                        name: "email".to_string(),
                        params: vec![],
                    }],
                    depends_on: vec![],
                },
            ],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);

    // Should detect: AddField, AddIndex, AddUniqueConstraint, AddConstraint
    assert!(changes.len() >= 1);

    // Check that we detected the new field
    let has_add_field = changes
        .iter()
        .any(|c| matches!(c, SchemaChange::AddField { .. }));
    assert!(has_add_field);
}

#[test]
fn test_diff_classifies_authored_body_residue() {
    // #74 Phase 2 (C8/C9): each hop's new-row body is classified ONCE at create
    // time as `Auto` (the differ can PROVE it) or `Authored` (semantic residue the
    // developer must supply).  Crucially the classifier is DISTINCT from
    // `is_breaking()` — a change can be breaking yet `Auto`.

    // Authored residue: only what the differ genuinely cannot prove a value for.
    let type_change = SchemaChange::ChangeFieldType {
        model_name: "User".to_string(),
        field_name: "age".to_string(),
        old_type: "u32".parse().unwrap(),
        new_type: "string".parse().unwrap(),
    };
    let narrow_nullable = SchemaChange::ChangeFieldNullability {
        model_name: "User".to_string(),
        field_name: "email".to_string(),
        old_nullable: true,
        new_nullable: false,
    };
    let required_no_default = SchemaChange::AddField {
        model_name: "User".to_string(),
        field_name: "role".to_string(),
        field_type: "string".parse().unwrap(),
        nullable: false,
        default_value: None,
    };
    for c in [&type_change, &narrow_nullable, &required_no_default] {
        assert_eq!(
            c.hop_body_class(),
            HopBodyClass::Authored,
            "unprovable hop must be Authored: {c:?}"
        );
    }

    // Breaking-but-Auto: the residue is NOT just "everything breaking".  Dropping a
    // field/model is breaking for readers but the row transform is a pure omit;
    // adding a &unique may fail on dups but the row bytes are identity.  These are
    // Auto despite being breaking — proving the classifier ≠ is_breaking().
    let remove_field = SchemaChange::RemoveField {
        model_name: "User".to_string(),
        field_name: "legacy".to_string(),
    };
    let remove_model = SchemaChange::RemoveModel {
        model_name: "Obsolete".to_string(),
    };
    let add_unique = SchemaChange::AddUniqueConstraint {
        model_name: "User".to_string(),
        field_name: "email".to_string(),
    };
    for c in [&remove_field, &remove_model, &add_unique] {
        assert!(c.is_breaking(), "precondition: {c:?} is breaking");
        assert_eq!(
            c.hop_body_class(),
            HopBodyClass::Auto,
            "breaking-but-provable hop must be Auto: {c:?}"
        );
    }

    // Additive / structural provable hops are Auto.
    let add_nullable = SchemaChange::AddField {
        model_name: "User".to_string(),
        field_name: "bio".to_string(),
        field_type: "string".parse().unwrap(),
        nullable: true,
        default_value: None,
    };
    let rename = SchemaChange::RenameField {
        model_name: "User".to_string(),
        old_name: "e".to_string(),
        new_name: "email".to_string(),
    };
    let add_index = SchemaChange::AddIndex {
        model_name: "User".to_string(),
        field_name: "email".to_string(),
        index_type: "Hash".to_string(),
    };
    for c in [&add_nullable, &rename, &add_index] {
        assert_eq!(c.hop_body_class(), HopBodyClass::Auto, "{c:?}");
    }

    // A migration exposes exactly its Authored residue.
    let mig = Migration::new(
        "mixed".to_string(),
        vec![add_nullable.clone(), type_change.clone(), remove_field.clone()],
    );
    let authored = mig.authored_changes();
    assert_eq!(authored.len(), 1, "only the type change is Authored");
    assert_eq!(authored[0], &type_change);
}

#[test]
fn test_migration_lineage_expands_range() {
    // #74 Phase 2: the committed migrations form a serial version lineage; the
    // transformer generator (Phase 3) walks it to expand a `--from B --to G` range
    // into the ordered hop sequence between those versions (C1 — a closed, fixed
    // serial range; never a synthesized jump).
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("migrations");

    // Empty lineage → baseline version 1, next span 1→2.
    let empty = MigrationLineage::load(&dir).unwrap();
    assert_eq!(empty.current_schema_version(), BASELINE_SCHEMA_VERSION);
    assert_eq!(empty.next_version_span(), (1, 2));

    // Record three contiguous hops: v1→2, v2→3, v3→4.
    for (i, (from, to)) in [(1u32, 2u32), (2, 3), (3, 4)].iter().enumerate() {
        MigrationGenerator::generate_versioned(
            &dir,
            format!("hop{i}"),
            vec![SchemaChange::AddField {
                model_name: "User".to_string(),
                field_name: format!("f{i}"),
                field_type: "string".parse().unwrap(),
                nullable: true,
                default_value: None,
            }],
            *from,
            *to,
        )
        .unwrap();
    }

    let lineage = MigrationLineage::load(&dir).unwrap();
    // Current version is the highest to_version; next span continues the chain.
    assert_eq!(lineage.current_schema_version(), 4);
    assert_eq!(lineage.next_version_span(), (4, 5));

    // Full range expands to all three hops in version order.
    let full = lineage.expand_range(1, 4).unwrap();
    assert_eq!(full.len(), 3);
    assert_eq!(full[0].from_version, 1);
    assert_eq!(full[0].to_version, 2);
    assert_eq!(full[2].to_version, 4);

    // A sub-range is a contiguous slice.
    let sub = lineage.expand_range(2, 4).unwrap();
    assert_eq!(sub.len(), 2);
    assert_eq!(sub[0].from_version, 2);

    // Empty range (from == to) is a valid no-op.
    assert!(lineage.expand_range(3, 3).unwrap().is_empty());

    // A range beyond the recorded lineage is refused (never synthesized).
    assert!(lineage.expand_range(1, 9).is_err());
    assert!(lineage.expand_range(4, 2).is_err());
}

#[test]
fn test_scaffold_authored_body_only_for_residue() {
    // #74 Phase 2 (C8/C9/C13): a migration with an Authored hop scaffolds a frozen
    // `migrations/{id}/transform.rs` for the developer; a fully-automatic migration
    // scaffolds nothing; and an existing authored body is NEVER clobbered.
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("migrations");
    std::fs::create_dir_all(&dir).unwrap();

    // Fully automatic (nullable add) → no scaffold.
    let auto = Migration::new_versioned(
        "auto".to_string(),
        vec![SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "bio".to_string(),
            field_type: "string".parse().unwrap(),
            nullable: true,
            default_value: None,
        }],
        1,
        2,
    );
    assert!(scaffold_authored_body(&dir, &auto).unwrap().is_none());

    // Authored (type change) → scaffold written.
    let authored = Migration::new_versioned(
        "authored".to_string(),
        vec![SchemaChange::ChangeFieldType {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            old_type: "u32".parse().unwrap(),
            new_type: "string".parse().unwrap(),
        }],
        2,
        3,
    );
    let (path, created) = scaffold_authored_body(&dir, &authored).unwrap().unwrap();
    assert!(created, "first scaffold is created");
    assert!(path.exists());
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("TODO"), "scaffold documents the residue to author");

    // Freeze the developer's authored body, then re-scaffold: must NOT clobber.
    std::fs::write(&path, "// FROZEN AUTHORED BODY\n").unwrap();
    let (path2, created2) = scaffold_authored_body(&dir, &authored).unwrap().unwrap();
    assert_eq!(path, path2);
    assert!(!created2, "an existing authored body is never clobbered");
    assert_eq!(std::fs::read_to_string(&path2).unwrap(), "// FROZEN AUTHORED BODY\n");
}

// ---------------------------------------------------------------------------
// #374 step 2 — `SimpleType` is a real type, and its wire form round-trips
// ---------------------------------------------------------------------------

/// Every variant's [`Display`] parses back to itself.
///
/// This is the property the whole design rests on: a `SimpleType` reaches a
/// migration record as a **plain string**, so if rendering and parsing ever
/// disagree the record silently degrades to `Opaque` — which reads exactly like
/// "this record predates #374" and would quietly un-prove every widening.
#[test]
fn simple_type_round_trips_through_its_wire_form() {
    use crate::SimpleType as S;
    let corpus = vec![
        S::U32,
        S::U64,
        S::I32,
        S::I64,
        S::F64,
        S::Bool,
        S::Str,
        S::StrN { chars: 1, exact: false },
        S::StrN { chars: 255, exact: true },
        S::Bytes(16),
        S::Uuid,
        S::Timestamp(0),
        S::Timestamp(1),
        S::Timestamp(2),
        S::Json,
        S::Decimal,
        S::Enum("Status".to_string()),
        S::Struct("Point".to_string()),
        S::Array(Box::new(S::U32), 4),
        S::Array(Box::new(S::Array(Box::new(S::U32), 2)), 3),
        S::Array(Box::new(S::Struct("Point".to_string())), 4),
        S::Relation("User".to_string()),
        S::Collection("Post".to_string()),
    ];
    for t in corpus {
        let rendered = t.to_string();
        let back: S = rendered.parse().unwrap();
        assert_eq!(back, t, "{rendered:?} did not round-trip");
        // And through serde, which is how it actually reaches a record.
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, format!("{:?}", rendered), "wire form is a plain string");
        let de: S = serde_json::from_str(&json).unwrap();
        assert_eq!(de, t);
    }
}

/// A type string written before #374 loads as `Opaque` rather than failing.
///
/// Every already-committed record carries `format!("{:?}", FieldType::U32)`.
/// Refusing to *load* those is worse than refusing to build them: the lineage
/// would become unreadable, `migrate status` included.
#[test]
fn a_pre_374_type_string_loads_as_opaque() {
    use crate::SimpleType as S;
    for legacy in ["U32", "Nullable(String)", "Enum(\"Status\")", "Timestamp(Millis)"] {
        let t: S = serde_json::from_str(&format!("{legacy:?}")).unwrap();
        assert_eq!(t, S::Opaque(legacy.to_string()), "{legacy} must stay opaque");
        assert!(
            !t.widens_to(&S::U64) && !S::U32.widens_to(&t),
            "an opaque type is never provable in either direction"
        );
    }
}

/// Scenario 2 — the widening table, positives **and** negatives.
///
/// The negative rows are the load-bearing ones: a classifier that says "yes" too
/// often silently corrupts data at a boundary value, and no snapshot can see it.
#[test]
fn scenario_2_widenings_are_provable_and_everything_else_is_not() {
    use crate::SimpleType as S;

    let ints = [S::U32, S::U64, S::I32, S::I64, S::F64];
    // The COMPLETE set of provable numeric pairs. Anything not listed must be
    // false, including every int -> f64.
    let provable: &[(S, S)] = &[
        (S::U32, S::U64),
        (S::I32, S::I64),
        (S::U32, S::I64),
    ];
    for a in &ints {
        for b in &ints {
            let expected = provable.iter().any(|(x, y)| x == a && y == b);
            assert_eq!(
                a.widens_to(b),
                expected,
                "{a} -> {b} should be {}provable",
                if expected { "" } else { "un" }
            );
        }
    }

    // string(N): grows only, exactness must be equal.
    let sn = |chars: usize, exact: bool| S::StrN { chars, exact };
    assert!(sn(4, false).widens_to(&sn(8, false)));
    assert!(sn(4, true).widens_to(&sn(8, true)));
    assert!(!sn(8, false).widens_to(&sn(4, false)), "shrinking truncates");
    assert!(!sn(4, false).widens_to(&sn(8, true)), "gaining `!` is a new constraint");
    assert!(!sn(4, true).widens_to(&sn(8, false)), "dropping `!` is a constraint change");
    assert!(!sn(4, false).widens_to(&S::Str), "fixed slot -> variable column");
    assert!(!S::Str.widens_to(&sn(4, false)), "variable column -> fixed slot");

    // timestamp: coarser -> finer only (rank s < ms < us).
    assert!(S::Timestamp(0).widens_to(&S::Timestamp(1)));
    assert!(S::Timestamp(0).widens_to(&S::Timestamp(2)));
    assert!(S::Timestamp(1).widens_to(&S::Timestamp(2)));
    assert!(!S::Timestamp(2).widens_to(&S::Timestamp(1)), "finer -> coarser floors");
    assert!(!S::Timestamp(1).widens_to(&S::Timestamp(0)), "finer -> coarser floors");

    // Identity is not a widening — the differ never emits it, and answering
    // `true` would read as "the widening set includes doing nothing".
    assert!(!S::U32.widens_to(&S::U32));

    // Cross-family changes are never provable.
    assert!(!S::U32.widens_to(&S::Str));
    assert!(!S::Str.widens_to(&S::U32));
    assert!(!S::Uuid.widens_to(&S::Str));
    assert!(!S::Decimal.widens_to(&S::F64));
    assert!(!S::Enum("A".into()).widens_to(&S::Str));
}
