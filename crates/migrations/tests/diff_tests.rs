use forgedb_migrations::*;

#[test]
fn test_detect_added_model() {
    let old_schema = SimpleSchema {
        models: vec![],
        enums: vec![],
        structs: vec![],
    };
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema).changes;
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::AddModel { .. }));
}

#[test]
fn test_detect_removed_model() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };
    let new_schema = SimpleSchema {
        models: vec![],
        enums: vec![],
        structs: vec![],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema).changes;
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::RemoveModel { .. }));
}

#[test]
fn test_detect_added_field() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "email".to_string(),
                ty: "string".parse().unwrap(),
                nullable: false,
                unique: false,
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

    let changes = SchemaDiffer::diff(&old_schema, &new_schema).changes;
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::AddField { .. }));
}

#[test]
fn test_detect_type_change() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "age".to_string(),
                ty: "u32".parse().unwrap(),
                nullable: false,
                unique: false,
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
            fields: vec![SimpleField {
                name: "age".to_string(),
                ty: "u64".parse().unwrap(),
                nullable: false,
                unique: false,
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

    let changes = SchemaDiffer::diff(&old_schema, &new_schema).changes;
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::ChangeFieldType { .. }));
    assert!(changes[0].is_breaking());
}

fn plain_field(name: &str, ty: &str) -> SimpleField {
    SimpleField {
        name: name.to_string(),
        ty: ty.parse().unwrap(),
        nullable: false,
        unique: false,
        indexed: false,
        index_type: "Hash".to_string(),
        constraints: vec![],
        depends_on: vec![],
    }
}

fn dependent_field(name: &str, ty: &str, deps: &[&str]) -> SimpleField {
    SimpleField {
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
        ..plain_field(name, ty)
    }
}

fn model(name: &str, fields: Vec<SimpleField>) -> SimpleModel {
    SimpleModel {
        name: name.to_string(),
        fields,
        composite_indexes: vec![],
    }
}

fn enum_def(name: &str, variants: &[&str]) -> SimpleEnum {
    SimpleEnum {
        name: name.to_string(),
        variants: variants.iter().map(|v| v.to_string()).collect(),
    }
}

fn struct_def(name: &str, fields: &[(&str, &str)]) -> SimpleStruct {
    SimpleStruct {
        name: name.to_string(),
        fields: fields.iter().map(|(n, t)| plain_field(n, t)).collect(),
    }
}

fn post_with_status(variants: &[&str]) -> SimpleSchema {
    SimpleSchema {
        models: vec![model(
            "Post",
            vec![
                plain_field("id", "Uuid"),
                dependent_field("status", "Enum(\"Status\")", &["Status"]),
            ],
        )],
        enums: vec![enum_def("Status", variants)],
        structs: vec![],
    }
}

fn shape_with_point(fields: &[(&str, &str)]) -> SimpleSchema {
    SimpleSchema {
        models: vec![model(
            "Shape",
            vec![
                plain_field("id", "Uuid"),
                dependent_field("origin", "StructType(\"Point\")", &["Point"]),
            ],
        )],
        enums: vec![],
        structs: vec![struct_def("Point", fields)],
    }
}

fn only_change(old: &SimpleSchema, new: &SimpleSchema) -> SchemaChange {
    let mut changes = SchemaDiffer::diff(old, new).changes;
    assert_eq!(
        changes.len(),
        1,
        "expected exactly one change, got {changes:#?}"
    );
    changes.pop().unwrap()
}

#[test]
fn enum_append_at_the_end_is_detected_and_benign() {
    let change = only_change(
        &post_with_status(&["Draft", "Published", "Archived"]),
        &post_with_status(&["Draft", "Published", "Archived", "Retired"]),
    );
    let SchemaChange::ChangeEnumVariants {
        model_name,
        field_name,
        enum_name,
        ..
    } = &change
    else {
        panic!("expected ChangeEnumVariants, got {change:#?}");
    };
    assert_eq!(
        (model_name.as_str(), field_name.as_str()),
        ("Post", "status")
    );
    assert_eq!(enum_name, "Status");
    assert!(
        !change.is_breaking(),
        "an append at the end is benign at rest"
    );
    assert_eq!(change.hop_body_class(), HopBodyClass::Auto);
}

#[test]
fn enum_insert_in_the_middle_is_breaking() {
    let change = only_change(
        &post_with_status(&["Draft", "Published", "Archived"]),
        &post_with_status(&["Draft", "Review", "Published", "Archived"]),
    );
    assert!(matches!(change, SchemaChange::ChangeEnumVariants { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Auto);
}

#[test]
fn enum_reorder_is_breaking_and_auto() {
    let change = only_change(
        &post_with_status(&["Draft", "Published", "Archived"]),
        &post_with_status(&["Published", "Draft", "Archived"]),
    );
    assert!(matches!(change, SchemaChange::ChangeEnumVariants { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Auto);
    assert!(
        change.description().contains("REORDER"),
        "the description must name the stored consequence: {}",
        change.description()
    );
}

#[test]
fn enum_remove_is_breaking_and_authored() {
    let change = only_change(
        &post_with_status(&["Draft", "Published", "Archived"]),
        &post_with_status(&["Draft", "Archived"]),
    );
    assert!(matches!(change, SchemaChange::ChangeEnumVariants { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Authored);
}

#[test]
fn enum_rename_in_place_is_breaking_and_authored() {
    let change = only_change(
        &post_with_status(&["Draft", "Published", "Archived"]),
        &post_with_status(&["Draft", "Live", "Archived"]),
    );
    assert!(matches!(change, SchemaChange::ChangeEnumVariants { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Authored);
}

#[test]
fn struct_field_reorder_is_breaking_and_auto() {
    let change = only_change(
        &shape_with_point(&[("x", "I32"), ("y", "I32")]),
        &shape_with_point(&[("y", "I32"), ("x", "I32")]),
    );
    let SchemaChange::ChangeStructLayout {
        model_name,
        field_name,
        struct_name,
        ..
    } = &change
    else {
        panic!("expected ChangeStructLayout, got {change:#?}");
    };
    assert_eq!(
        (model_name.as_str(), field_name.as_str()),
        ("Shape", "origin")
    );
    assert_eq!(struct_name, "Point");
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Auto);
}

#[test]
fn struct_field_retype_is_breaking_and_authored() {
    let change = only_change(
        &shape_with_point(&[("x", "I32"), ("y", "I32")]),
        &shape_with_point(&[("x", "U32"), ("y", "I32")]),
    );
    assert!(matches!(change, SchemaChange::ChangeStructLayout { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Authored);
}

#[test]
fn struct_field_add_is_breaking_and_authored() {
    let change = only_change(
        &shape_with_point(&[("x", "I32"), ("y", "I32")]),
        &shape_with_point(&[("x", "I32"), ("y", "I32"), ("z", "I32")]),
    );
    assert!(matches!(change, SchemaChange::ChangeStructLayout { .. }));
    assert!(
        change.is_breaking(),
        "a struct has no benign append — the row width changes"
    );
    assert_eq!(change.hop_body_class(), HopBodyClass::Authored);
}

#[test]
fn struct_field_remove_is_breaking_and_authored() {
    let change = only_change(
        &shape_with_point(&[("x", "I32"), ("y", "I32"), ("z", "I32")]),
        &shape_with_point(&[("x", "I32"), ("y", "I32")]),
    );
    assert!(matches!(change, SchemaChange::ChangeStructLayout { .. }));
    assert!(change.is_breaking());
    assert_eq!(change.hop_body_class(), HopBodyClass::Authored);
}

fn sticker_with_color(colors: &[&str]) -> SimpleSchema {
    SimpleSchema {
        models: vec![model(
            "Sticker",
            vec![
                plain_field("id", "Uuid"),
                dependent_field(
                    "badges",
                    "FixedArray(StructType(\"Badge\"), 4)",
                    &["Badge", "Color"],
                ),
            ],
        )],
        enums: vec![enum_def("Color", colors)],
        structs: vec![SimpleStruct {
            name: "Badge".to_string(),
            fields: vec![
                plain_field("rank", "U32"),
                dependent_field("tint", "Enum(\"Color\")", &["Color"]),
            ],
        }],
    }
}

#[test]
fn an_enum_nested_inside_a_struct_inside_an_array_still_projects_onto_the_model_field() {
    let change = only_change(
        &sticker_with_color(&["Red", "Green", "Blue"]),
        &sticker_with_color(&["Green", "Red", "Blue"]),
    );
    let SchemaChange::ChangeEnumVariants {
        model_name,
        field_name,
        enum_name,
        ..
    } = &change
    else {
        panic!("expected ChangeEnumVariants, got {change:#?}");
    };
    assert_eq!(
        (model_name.as_str(), field_name.as_str(), enum_name.as_str()),
        ("Sticker", "badges", "Color"),
        "the change must land on the model field that STORES the enum, however \
         deeply the type nests it"
    );
    assert!(change.is_breaking());
}

#[test]
fn an_unreferenced_enum_emits_nothing() {
    let schema = |variants: &[&str]| SimpleSchema {
        models: vec![model("Post", vec![plain_field("id", "Uuid")])],
        enums: vec![enum_def("Status", variants)],
        structs: vec![],
    };
    let changes = SchemaDiffer::diff(
        &schema(&["Draft", "Published"]),
        &schema(&["Published", "Draft"]),
    ).changes;
    assert!(
        changes.is_empty(),
        "an enum no storage-backed field reaches has no rows to migrate: {changes:#?}"
    );
}

#[test]
fn an_unreferenced_struct_emits_nothing() {
    let schema = |fields: &[(&str, &str)]| SimpleSchema {
        models: vec![model("Shape", vec![plain_field("id", "Uuid")])],
        enums: vec![],
        structs: vec![struct_def("Point", fields)],
    };
    let changes = SchemaDiffer::diff(
        &schema(&[("x", "I32"), ("y", "I32")]),
        &schema(&[("y", "I32"), ("x", "I32")]),
    ).changes;
    assert!(
        changes.is_empty(),
        "a struct no storage-backed field reaches has no rows to migrate: {changes:#?}"
    );
}

#[test]
fn one_change_is_emitted_per_storing_field() {
    let schema = |variants: &[&str]| SimpleSchema {
        models: vec![model(
            "Post",
            vec![
                plain_field("id", "Uuid"),
                dependent_field("status", "Enum(\"Status\")", &["Status"]),
                dependent_field("prev_status", "Enum(\"Status\")", &["Status"]),
            ],
        )],
        enums: vec![enum_def("Status", variants)],
        structs: vec![],
    };
    let changes = SchemaDiffer::diff(
        &schema(&["Draft", "Published"]),
        &schema(&["Published", "Draft"]),
    ).changes;
    assert_eq!(changes.len(), 2, "one per storing field: {changes:#?}");
    let mut fields: Vec<&str> = changes
        .iter()
        .map(|c| match c {
            SchemaChange::ChangeEnumVariants { field_name, .. } => field_name.as_str(),
            other => panic!("unexpected {other:#?}"),
        })
        .collect();
    fields.sort();
    assert_eq!(fields, ["prev_status", "status"]);
}
