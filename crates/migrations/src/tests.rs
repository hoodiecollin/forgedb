use super::*;
use tempfile::TempDir;

#[test]
fn test_migration_generate_and_load_roundtrip() {
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
            default_json: Some("0".to_string()),
            answer: None,
        },
    ];

    let migration =
        MigrationGenerator::generate(&migrations_dir, "Create User model".to_string(), changes)
            .unwrap();
    assert_eq!(migration.changes.len(), 2);

    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    assert!(!tracker.is_applied(&migration.id));
    tracker
        .mark_applied(migration.id.clone(), migration.checksum.clone())
        .unwrap();
    assert!(tracker.is_applied(&migration.id));

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
            default_json: None,
            answer: None,
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
            answer: None,
        },
    ];

    let migration = Migration::new("Update User model".to_string(), changes);

    assert!(migration.has_breaking_changes());
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

    let changes = SchemaDiffer::diff(&old_schema, &new_schema).changes;

    assert!(changes.len() >= 1);

    let has_add_field = changes
        .iter()
        .any(|c| matches!(c, SchemaChange::AddField { .. }));
    assert!(has_add_field);
}

#[test]
fn test_diff_classifies_authored_body_residue() {

    let type_change = SchemaChange::ChangeFieldType {
        model_name: "User".to_string(),
        field_name: "age".to_string(),
        old_type: "u32".parse().unwrap(),
        new_type: "string".parse().unwrap(),
        answer: None,
    };
    let narrow_nullable = SchemaChange::ChangeFieldNullability {
        model_name: "User".to_string(),
        field_name: "email".to_string(),
        old_nullable: true,
        new_nullable: false,
        answer: None,
    };
    let required_no_default = SchemaChange::AddField {
        model_name: "User".to_string(),
        field_name: "role".to_string(),
        field_type: "string".parse().unwrap(),
        nullable: false,
        default_json: None,
        answer: None,
    };
    for c in [&type_change, &narrow_nullable, &required_no_default] {
        assert_eq!(
            c.hop_body_class(),
            HopBodyClass::Authored,
            "unprovable hop must be Authored: {c:?}"
        );
    }

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

    let add_nullable = SchemaChange::AddField {
        model_name: "User".to_string(),
        field_name: "bio".to_string(),
        field_type: "string".parse().unwrap(),
        nullable: true,
        default_json: None,
        answer: None,
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
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("migrations");

    let empty = MigrationLineage::load(&dir).unwrap();
    assert_eq!(empty.current_schema_version(), BASELINE_SCHEMA_VERSION);
    assert_eq!(empty.next_version_span(), (1, 2));

    for (i, (from, to)) in [(1u32, 2u32), (2, 3), (3, 4)].iter().enumerate() {
        MigrationGenerator::generate_versioned(
            &dir,
            format!("hop{i}"),
            vec![SchemaChange::AddField {
                model_name: "User".to_string(),
                field_name: format!("f{i}"),
                field_type: "string".parse().unwrap(),
                nullable: true,
                default_json: None,
                answer: None,
            }],
            *from,
            *to,
        )
        .unwrap();
    }

    let lineage = MigrationLineage::load(&dir).unwrap();
    assert_eq!(lineage.current_schema_version(), 4);
    assert_eq!(lineage.next_version_span(), (4, 5));

    let full = lineage.expand_range(1, 4).unwrap();
    assert_eq!(full.len(), 3);
    assert_eq!(full[0].from_version, 1);
    assert_eq!(full[0].to_version, 2);
    assert_eq!(full[2].to_version, 4);

    let sub = lineage.expand_range(2, 4).unwrap();
    assert_eq!(sub.len(), 2);
    assert_eq!(sub[0].from_version, 2);

    assert!(lineage.expand_range(3, 3).unwrap().is_empty());

    assert!(lineage.expand_range(1, 9).is_err());
    assert!(lineage.expand_range(4, 2).is_err());
}

#[test]
fn test_scaffold_authored_body_only_for_residue() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("migrations");
    std::fs::create_dir_all(&dir).unwrap();

    let auto = Migration::new_versioned(
        "auto".to_string(),
        vec![SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "bio".to_string(),
            field_type: "string".parse().unwrap(),
            nullable: true,
            default_json: None,
            answer: None,
        }],
        1,
        2,
    );
    assert!(scaffold_authored_body(&dir, &auto).unwrap().is_none());

    let authored = Migration::new_versioned(
        "authored".to_string(),
        vec![SchemaChange::ChangeFieldType {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            old_type: "u32".parse().unwrap(),
            new_type: "string".parse().unwrap(),
            answer: None,
        }],
        2,
        3,
    );
    let (path, created) = scaffold_authored_body(&dir, &authored).unwrap().unwrap();
    assert!(created, "first scaffold is created");
    assert!(path.exists());
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("TODO"), "scaffold documents the residue to author");

    std::fs::write(&path, "// FROZEN AUTHORED BODY\n").unwrap();
    let (path2, created2) = scaffold_authored_body(&dir, &authored).unwrap().unwrap();
    assert_eq!(path, path2);
    assert!(!created2, "an existing authored body is never clobbered");
    assert_eq!(std::fs::read_to_string(&path2).unwrap(), "// FROZEN AUTHORED BODY\n");
}

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
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, format!("{:?}", rendered), "wire form is a plain string");
        let de: S = serde_json::from_str(&json).unwrap();
        assert_eq!(de, t);
    }
}

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

#[test]
fn scenario_2_widenings_are_provable_and_everything_else_is_not() {
    use crate::SimpleType as S;

    let ints = [S::U32, S::U64, S::I32, S::I64, S::F64];
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

    let sn = |chars: usize, exact: bool| S::StrN { chars, exact };
    assert!(sn(4, false).widens_to(&sn(8, false)));
    assert!(sn(4, true).widens_to(&sn(8, true)));
    assert!(!sn(8, false).widens_to(&sn(4, false)), "shrinking truncates");
    assert!(!sn(4, false).widens_to(&sn(8, true)), "gaining `!` is a new constraint");
    assert!(!sn(4, true).widens_to(&sn(8, false)), "dropping `!` is a constraint change");
    assert!(!sn(4, false).widens_to(&S::Str), "fixed slot -> variable column");
    assert!(!S::Str.widens_to(&sn(4, false)), "variable column -> fixed slot");

    assert!(S::Timestamp(0).widens_to(&S::Timestamp(1)));
    assert!(S::Timestamp(0).widens_to(&S::Timestamp(2)));
    assert!(S::Timestamp(1).widens_to(&S::Timestamp(2)));
    assert!(!S::Timestamp(2).widens_to(&S::Timestamp(1)), "finer -> coarser floors");
    assert!(!S::Timestamp(1).widens_to(&S::Timestamp(0)), "finer -> coarser floors");

    assert!(!S::U32.widens_to(&S::U32));

    assert!(!S::U32.widens_to(&S::Str));
    assert!(!S::Str.widens_to(&S::U32));
    assert!(!S::Uuid.widens_to(&S::Str));
    assert!(!S::Decimal.widens_to(&S::F64));
    assert!(!S::Enum("A".into()).widens_to(&S::Str));
}

fn code_of(fn_signature: &str) -> String {
    let src = include_str!("types.rs");
    let start = src
        .find(fn_signature)
        .unwrap_or_else(|| panic!("`{fn_signature}` is not in types.rs — did it get renamed?"));
    let rest = &src[start..];
    let end = rest
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("no closing brace found for `{fn_signature}`"));
    rest[..end]
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn scenario_3_neither_classifier_has_a_catch_all_arm() {
    for sig in [
        "pub fn hop_body_class(&self) -> HopBodyClass {",
        "pub fn is_breaking(&self) -> bool {",
    ] {
        let code = code_of(sig);
        assert!(
            !code.contains("_ =>"),
            "`{sig}` has a catch-all arm again. Every `SchemaChange` variant must \
             be classified deliberately; a wildcard makes the next variant \
             silently provable (or silently safe). Body:\n{code}"
        );
    }
}

#[test]
fn scenario_2_the_classifier_calls_the_widening_table() {
    use crate::SimpleType as S;
    let change = |old: S, new: S| SchemaChange::ChangeFieldType {
        model_name: "Post".into(),
        field_name: "views".into(),
        old_type: old,
        new_type: new,
        answer: None,
    };

    for (old, new) in [
        (S::U32, S::U64),
        (S::I32, S::I64),
        (S::U32, S::I64),
        (S::StrN { chars: 4, exact: false }, S::StrN { chars: 8, exact: false }),
        (S::Timestamp(0), S::Timestamp(2)),
    ] {
        let c = change(old.clone(), new.clone());
        assert_eq!(
            c.hop_body_class(),
            HopBodyClass::Auto,
            "{old} -> {new} is value-preserving and must need no author"
        );
        assert!(c.is_breaking(), "{old} -> {new} still rewrites data at rest");
    }

    for (old, new) in [
        (S::U64, S::U32),
        (S::I32, S::U32),
        (S::U64, S::I64),
        (S::U32, S::F64),
        (S::StrN { chars: 8, exact: false }, S::StrN { chars: 4, exact: false }),
        (S::Timestamp(2), S::Timestamp(0)),
        (S::Str, S::U32),
        (S::Opaque("U32".into()), S::Opaque("U64".into())),
    ] {
        assert_eq!(
            change(old.clone(), new.clone()).hop_body_class(),
            HopBodyClass::Authored,
            "{old} -> {new} is NOT value-preserving and must reach a human"
        );
    }
}

#[test]
fn scenario_3b_widening_nullability_is_provable_and_narrowing_is_not() {
    let n = |old: bool, new: bool| SchemaChange::ChangeFieldNullability {
        model_name: "Post".into(),
        field_name: "views".into(),
        old_nullable: old,
        new_nullable: new,
        answer: None,
    };
    assert_eq!(n(false, true).hop_body_class(), HopBodyClass::Auto);
    assert!(!n(false, true).is_breaking());
    assert_eq!(n(true, false).hop_body_class(), HopBodyClass::Authored);
    assert!(n(true, false).is_breaking());
}

#[test]
fn a_rename_is_breaking_even_though_it_needs_no_author() {
    let f = SchemaChange::RenameField {
        model_name: "Post".into(),
        old_name: "views".into(),
        new_name: "hits".into(),
    };
    assert!(f.is_breaking(), "a rename needs the offline transformer");
    assert_eq!(f.hop_body_class(), HopBodyClass::Auto, "…but no human");

    let m = SchemaChange::RenameModel {
        old_name: "Post".into(),
        new_name: "Article".into(),
    };
    assert!(m.is_breaking());
    assert_eq!(m.hop_body_class(), HopBodyClass::Auto);
}
