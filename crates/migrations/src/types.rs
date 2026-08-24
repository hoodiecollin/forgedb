use crate::diff::SimpleType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a change detected in a schema migration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaChange {
    /// A new model was added
    AddModel { model_name: String },
    /// A model was removed
    RemoveModel { model_name: String },
    /// A field was added to a model
    AddField {
        model_name: String,
        field_name: String,
        /// The **base** type (nullability is `nullable`), structured since #374
        /// so the classifier can ask whether the value is provable. On disk it
        /// is still a plain string — see [`SimpleType`].
        field_type: SimpleType,
        nullable: bool,
        /// The **JSON literal** existing rows get, resolved from the
        /// destination field's `@default` directive (#374 step 4). `None` when
        /// the field declares no default, or declares one this build cannot
        /// resolve — see `forgedb_codegen::default_fill`.
        ///
        /// This is the *schema's* answer and is distinct from
        /// [`SchemaChange`]'s operator-supplied one: generated code knows a
        /// `@default` and applies it in the reopen backfill, so an add carrying
        /// one is not breaking. An operator's answer is known only to the
        /// transformer, so an add carrying one IS.
        ///
        /// It replaces the never-populated `default_value`, which the differ set
        /// to `None` at its one construction site and no other producer ever
        /// touched (gate 1 finding 3) — a carrier that read as "the default, if
        /// any" while always meaning "no".
        default_json: Option<String>,
    },
    /// A field was removed from a model
    RemoveField {
        model_name: String,
        field_name: String,
    },
    /// A field's type was changed
    ChangeFieldType {
        model_name: String,
        field_name: String,
        old_type: SimpleType,
        new_type: SimpleType,
    },
    /// A field's nullability was changed
    ChangeFieldNullability {
        model_name: String,
        field_name: String,
        old_nullable: bool,
        new_nullable: bool,
    },
    /// A field was renamed
    RenameField {
        model_name: String,
        old_name: String,
        new_name: String,
    },
    /// A model was renamed
    RenameModel { old_name: String, new_name: String },
    /// An index was added
    AddIndex {
        model_name: String,
        field_name: String,
        index_type: String,
    },
    /// An index was removed
    RemoveIndex {
        model_name: String,
        field_name: String,
    },
    /// A unique constraint was added
    AddUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    /// A unique constraint was removed
    RemoveUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    /// A composite index was added
    AddCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    /// A composite index was removed
    RemoveCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    /// A constraint was added
    AddConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
        constraint_params: Vec<String>,
    },
    /// A constraint was removed
    RemoveConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
    },
    /// The variant list of an `enum` a stored field uses changed (#438).
    ///
    /// **Field-scoped on purpose.** The definition is schema-level, but the
    /// *data* it endangers is a column, so the change is reported once per
    /// storing field. That keeps [`target_model`](SchemaChange::target_model)
    /// returning `&str` and keeps the authored scaffold's per-model grouping
    /// working with no surface change.
    ///
    /// Carries the two **ordered lists**, never a pre-baked verdict: the
    /// classification is derived by [`classify_positional`] and so re-derives
    /// identically forever, which is what `HopBodyClass`'s frozen-at-`migrate
    /// create` contract requires.
    ChangeEnumVariants {
        model_name: String,
        field_name: String,
        enum_name: String,
        old_variants: Vec<String>,
        new_variants: Vec<String>,
    },
    /// The layout of an inline `struct` a stored field uses changed (#438).
    ///
    /// `#[repr(C)]`, whole value transmuted into a `size_of::<T>()` slot — so
    /// field order AND per-field width are both on disk. Carries `(name, type)`
    /// in declaration order for the same reason as above.
    ChangeStructLayout {
        model_name: String,
        field_name: String,
        struct_name: String,
        old_fields: Vec<(String, String)>,
        new_fields: Vec<(String, String)>,
    },
}

/// What happened to an **ordered, position-keyed** list of names (#438).
///
/// One classifier for both enum variants and struct field names, because both
/// are stored positionally and the question "did the byte→meaning mapping move?"
/// has exactly one right answer for either. `is_breaking`, `hop_body_class` and
/// `description` all read this; none of them re-derives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionalDelta {
    /// The lists are identical.
    Unchanged,
    /// Names were added and **nothing else moved or went away**, so every added
    /// name necessarily landed at an index `>= old.len()`. Every existing byte
    /// still decodes to what it did. The one safe edit an enum has.
    Appended { added: Vec<String> },
    /// Exactly one name dropped and one added **at the same index**, nothing
    /// moved. Mirrors the differ's existing single-unambiguous-rename heuristic
    /// for fields. The slot is unchanged, but the *name* is what crosses the
    /// transformer's JSON boundary — so this is not benign.
    Renamed { old_name: String, new_name: String },
    /// A name that exists on both sides changed index. Every stored byte at or
    /// past the first moved position now decodes as some other name, with no
    /// byte out of range and therefore no failure mode at all.
    Reordered { moved: Vec<String> },
    /// A name went away. The mapping moved *and* some stored byte may now be out
    /// of range entirely.
    Dropped { dropped: Vec<String> },
}

/// Classify what moved in an ordered list of names (#438).
///
/// Precedence is deliberate and runs strictest-first once `Renamed` (a *narrower*
/// reading of a drop+add pair) has had its chance: a removal in the middle both
/// drops a name and moves its successors, and `Dropped` is the answer that costs
/// the operator an authored body rather than silently promising them an automatic
/// one.
pub fn classify_positional(old: &[String], new: &[String]) -> PositionalDelta {
    if old == new {
        return PositionalDelta::Unchanged;
    }

    let index_in = |list: &[String], name: &str| list.iter().position(|n| n == name);

    let dropped: Vec<String> = old.iter().filter(|n| !new.contains(n)).cloned().collect();
    let added: Vec<String> = new.iter().filter(|n| !old.contains(n)).cloned().collect();
    let moved: Vec<String> = old
        .iter()
        .filter(|n| match (index_in(old, n), index_in(new, n)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        })
        .cloned()
        .collect();

    if moved.is_empty() && dropped.len() == 1 && added.len() == 1 {
        let old_name = &dropped[0];
        let new_name = &added[0];
        if index_in(old, old_name) == index_in(new, new_name) {
            return PositionalDelta::Renamed {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
            };
        }
    }
    if !dropped.is_empty() {
        return PositionalDelta::Dropped { dropped };
    }
    if !moved.is_empty() {
        return PositionalDelta::Reordered { moved };
    }
    if !added.is_empty() {
        return PositionalDelta::Appended { added };
    }
    PositionalDelta::Unchanged
}

/// What happened to an inline `struct`'s layout (#438).
///
/// A superset of [`PositionalDelta`]: a struct has one failure mode an enum does
/// not, because its fields carry a *width* as well as a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutDelta {
    /// A field kept its name and position but changed type. Same-width retypes
    /// (`i32` → `u32`) are the silent ones; different-width retypes re-frame the
    /// column.
    Retyped { fields: Vec<String> },
    /// Nothing was retyped; the field *names* moved (or did not).
    Names(PositionalDelta),
}

/// Classify an inline struct's layout change (#438).
///
/// A retype outranks a name move: it is the case the differ can prove least
/// about, and every struct edit except a pure reorder is `Authored` anyway.
pub fn classify_layout(old: &[(String, String)], new: &[(String, String)]) -> LayoutDelta {
    let retyped: Vec<String> = old
        .iter()
        .filter_map(|(name, old_ty)| {
            new.iter()
                .find(|(n, _)| n == name)
                .filter(|(_, new_ty)| new_ty != old_ty)
                .map(|_| name.clone())
        })
        .collect();
    if !retyped.is_empty() {
        return LayoutDelta::Retyped { fields: retyped };
    }
    let names = |list: &[(String, String)]| -> Vec<String> {
        list.iter().map(|(n, _)| n.clone()).collect()
    };
    LayoutDelta::Names(classify_positional(&names(old), &names(new)))
}

/// How the transformer generator (#74 Phase 3) will produce the new-row body for
/// a single schema change ("hop"), decided ONCE at `migrate create` time and
/// frozen into the migration record (C8/C9).  This is a **dev-time codegen
/// classification** — the transformer bin has no runtime mechanism-selection
/// site; it just runs whichever body was baked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopBodyClass {
    /// The differ can PROVE the new-row body from the diff alone —
    /// **additive / constant / structural**: set a field's default, omit a
    /// removed column, rename a field/model, or an index/constraint change that
    /// leaves the row bytes identical.  The transformer generator emits the body
    /// automatically; no developer authoring is required.
    Auto,
    /// The new-row body needs **semantic understanding the diff cannot supply** —
    /// re-encode a value whose type changed, choose a fill value when narrowing a
    /// nullable field to NOT NULL, or supply a value for a newly-required field
    /// with no default.  Scaffolded at `migrate create` for the developer to
    /// author + freeze (`migrations/{id}/transform.rs`); the transformer embeds
    /// that frozen source verbatim (C13).
    Authored,
}

impl SchemaChange {
    /// The verdict table for an enum variant-list change (#438).
    ///
    /// **The one place** `ChangeEnumVariants`'s severity and hop class are
    /// decided; `is_breaking`, `hop_body_class` and `description` all call it.
    ///
    /// | delta | breaking | class | why |
    /// |---|---|---|---|
    /// | `Appended` | no | `Auto` | every existing byte still decodes to itself; recorded only so the version moves and an older binary is told to migrate rather than panicking on an unknown byte |
    /// | `Reordered` | **yes** | `Auto` | every stored row silently re-maps — but an enum crosses the transformer's JSON boundary as its **name**, so the existing identity hop body re-encodes it with no authoring |
    /// | `Dropped` | **yes** | `Authored` | a row carrying the retired name has nothing to deserialize into |
    /// | `Renamed` | **yes** | `Authored` | same reason: the old name does not deserialize |
    fn enum_verdict(old: &[String], new: &[String]) -> (bool, HopBodyClass) {
        match classify_positional(old, new) {
            PositionalDelta::Unchanged => (false, HopBodyClass::Auto),
            PositionalDelta::Appended { .. } => (false, HopBodyClass::Auto),
            PositionalDelta::Reordered { .. } => (true, HopBodyClass::Auto),
            PositionalDelta::Dropped { .. } | PositionalDelta::Renamed { .. } => {
                (true, HopBodyClass::Authored)
            }
        }
    }

    /// The verdict table for an inline-struct layout change (#438).
    ///
    /// **There is no additive case for a struct.** Unlike an enum, every field's
    /// offset is a function of the whole declaration, so the enum's one safe
    /// edit has no struct analogue — `Appended` is breaking here.
    ///
    /// | delta | breaking | class |
    /// |---|---|---|
    /// | `Names(Reordered)` | **yes** | `Auto` (JSON transport is by field name) |
    /// | `Retyped` / `Names(Appended)` / `Names(Dropped)` / `Names(Renamed)` | **yes** | `Authored` |
    fn struct_verdict(old: &[(String, String)], new: &[(String, String)]) -> (bool, HopBodyClass) {
        match classify_layout(old, new) {
            LayoutDelta::Names(PositionalDelta::Unchanged) => (false, HopBodyClass::Auto),
            LayoutDelta::Names(PositionalDelta::Reordered { .. }) => (true, HopBodyClass::Auto),
            // Retyped, added, dropped, renamed: the width or the JSON key moved
            // and the differ can prove no value for the result. Matches how
            // `ChangeFieldType` and a required `AddField` are already classified.
            _ => (true, HopBodyClass::Authored),
        }
    }

    /// Classify how this hop's new-row body is produced (#74 Phase 2, C8/C9).
    ///
    /// This is deliberately **distinct from [`is_breaking`](Self::is_breaking)**:
    /// a change can be breaking yet still `Auto` (dropping a column or a model is
    /// breaking for readers but the row transform is a pure structural omit; a
    /// `&unique` add may fail on duplicates but the row bytes are identity —
    /// uniqueness is *validated* during replay, not *transformed*).  Only the
    /// residue the differ genuinely cannot PROVE a value for is `Authored`.
    pub fn hop_body_class(&self) -> HopBodyClass {
        match self {
            // Re-encoding a value from one type to another is semantic — UNLESS
            // the change is a value-preserving widening, in which case every
            // value maps to itself and there is nothing for a human to decide.
            // `widens_to` is the one definition of that (#374 direction B); the
            // fixture that motivated this issue demanded a hand-written Rust
            // function for `u32 -> u64`.
            SchemaChange::ChangeFieldType {
                old_type, new_type, ..
            } => {
                if old_type.widens_to(new_type) {
                    HopBodyClass::Auto
                } else {
                    HopBodyClass::Authored
                }
            }
            // Narrowing nullable -> NOT NULL needs a fill value for the existing
            // `None`s; the differ has none to offer. The other direction (`T ->
            // ?T`) is provable: every existing value is still itself, and the
            // column simply gains a presence tag.
            SchemaChange::ChangeFieldNullability {
                old_nullable: true,
                new_nullable: false,
                ..
            } => HopBodyClass::Authored,
            SchemaChange::ChangeFieldNullability { .. } => HopBodyClass::Auto,
            // A newly-required field has no value the differ can synthesize for
            // existing rows — unless the destination schema declares one, in
            // which case the answer is written down in the `.forge` and is the
            // same value the generated reopen-backfill writes.
            SchemaChange::AddField {
                nullable: false,
                default_json: None,
                ..
            } => HopBodyClass::Authored,
            SchemaChange::AddField { .. } => HopBodyClass::Auto,
            // #438: positional, so the answer depends on WHICH way the list
            // moved. Delegated to the one classifier — never re-derived here.
            SchemaChange::ChangeEnumVariants {
                old_variants,
                new_variants,
                ..
            } => Self::enum_verdict(old_variants, new_variants).1,
            SchemaChange::ChangeStructLayout {
                old_fields,
                new_fields,
                ..
            } => Self::struct_verdict(old_fields, new_fields).1,
            // The provable structural residue, named one variant at a time.
            //
            // This match has NO `_ =>` arm, and that is the point (#374 step 3).
            // The catch-all here used to be `Auto`, so any variant added later —
            // #438's `ChangeEnumVariants` among them, had it landed a week later
            // — was silently provable, and a dropped enum variant would have
            // become a hop no human ever looked at. Adding a variant must now be
            // a compile error until someone decides what it means.
            SchemaChange::AddModel { .. }
            | SchemaChange::RemoveModel { .. }
            | SchemaChange::RemoveField { .. }
            | SchemaChange::RenameField { .. }
            | SchemaChange::RenameModel { .. }
            | SchemaChange::AddIndex { .. }
            | SchemaChange::RemoveIndex { .. }
            | SchemaChange::AddUniqueConstraint { .. }
            | SchemaChange::RemoveUniqueConstraint { .. }
            | SchemaChange::AddCompositeIndex { .. }
            | SchemaChange::RemoveCompositeIndex { .. }
            | SchemaChange::AddConstraint { .. }
            | SchemaChange::RemoveConstraint { .. } => HopBodyClass::Auto,
        }
    }

    /// The model this change targets, if it is a single-model change.  Used by the
    /// authored-body scaffold (#74 Phase 2/3) to group `Authored` hops per model.
    /// `RenameModel` reports its *new* name (the destination shape); `AddModel`/
    /// `RemoveModel` report the model itself.
    pub fn target_model(&self) -> &str {
        match self {
            SchemaChange::AddModel { model_name }
            | SchemaChange::RemoveModel { model_name }
            | SchemaChange::AddField { model_name, .. }
            | SchemaChange::RemoveField { model_name, .. }
            | SchemaChange::ChangeFieldType { model_name, .. }
            | SchemaChange::ChangeFieldNullability { model_name, .. }
            | SchemaChange::RenameField { model_name, .. }
            | SchemaChange::AddIndex { model_name, .. }
            | SchemaChange::RemoveIndex { model_name, .. }
            | SchemaChange::AddUniqueConstraint { model_name, .. }
            | SchemaChange::RemoveUniqueConstraint { model_name, .. }
            | SchemaChange::AddCompositeIndex { model_name, .. }
            | SchemaChange::RemoveCompositeIndex { model_name, .. }
            | SchemaChange::AddConstraint { model_name, .. }
            | SchemaChange::RemoveConstraint { model_name, .. }
            | SchemaChange::ChangeEnumVariants { model_name, .. }
            | SchemaChange::ChangeStructLayout { model_name, .. } => model_name,
            SchemaChange::RenameModel { new_name, .. } => new_name,
        }
    }

    /// Returns true if this change is considered breaking (requires manual intervention)
    pub fn is_breaking(&self) -> bool {
        match self {
            SchemaChange::RemoveModel { .. } => true,
            SchemaChange::RemoveField { .. } => true,
            SchemaChange::ChangeFieldType { .. } => true,
            SchemaChange::ChangeFieldNullability {
                old_nullable: true,
                new_nullable: false,
                ..
            } => true,
            SchemaChange::ChangeFieldNullability { .. } => false,
            // M4: Adding a NOT NULL column without a default to a populated table is
            // breaking — existing rows have no value to fill in.
            SchemaChange::AddField {
                nullable: false,
                default_json: None,
                ..
            } => true,
            SchemaChange::AddField { .. } => false,
            // A rename moves the bytes: the model's directory name and the
            // column's file name are both derived from the declared name, so a
            // renamed field's data does NOT follow the name across a reopen. It
            // needs the offline transformer exactly as a drop does, and calling
            // it non-breaking sent the operator down the "additive — just
            // reopen" path, which silently empties the column.
            SchemaChange::RenameField { .. } => true,
            SchemaChange::RenameModel { .. } => true,
            SchemaChange::RemoveUniqueConstraint { .. } => false, // Safe to remove constraints
            SchemaChange::AddUniqueConstraint { .. } => true,     // May fail if duplicates exist
            // #438: an append is benign at rest; every other shape re-maps
            // stored bytes. Same classifier as `hop_body_class`.
            SchemaChange::ChangeEnumVariants {
                old_variants,
                new_variants,
                ..
            } => Self::enum_verdict(old_variants, new_variants).0,
            SchemaChange::ChangeStructLayout {
                old_fields,
                new_fields,
                ..
            } => Self::struct_verdict(old_fields, new_fields).0,
            // Non-breaking at rest, named one variant at a time. Like
            // `hop_body_class` above this match has NO `_ =>` arm, and for the
            // sharper of the two reasons: its catch-all was `false`, so a
            // variant added later was silently declared safe for data at rest.
            SchemaChange::AddModel { .. }
            | SchemaChange::AddIndex { .. }
            | SchemaChange::RemoveIndex { .. }
            | SchemaChange::AddCompositeIndex { .. }
            | SchemaChange::RemoveCompositeIndex { .. }
            | SchemaChange::AddConstraint { .. }
            | SchemaChange::RemoveConstraint { .. } => false,
        }
    }

    /// Returns a human-readable description of the change
    pub fn description(&self) -> String {
        match self {
            SchemaChange::AddModel { model_name } => {
                format!("Add model '{}'", model_name)
            }
            SchemaChange::RemoveModel { model_name } => {
                format!("Remove model '{}' (⚠️  BREAKING)", model_name)
            }
            SchemaChange::AddField {
                model_name,
                field_name,
                field_type,
                nullable,
                ..
            } => {
                let null_str = if *nullable { "?" } else { "" };
                format!(
                    "Add field '{}.{}{}' ({})",
                    model_name, field_name, null_str, field_type
                )
            }
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } => {
                format!(
                    "Remove field '{}.{}' (⚠️  BREAKING)",
                    model_name, field_name
                )
            }
            SchemaChange::ChangeFieldType {
                model_name,
                field_name,
                old_type,
                new_type,
            } => {
                format!(
                    "Change type of '{}.{}' from {} to {} (⚠️  BREAKING)",
                    model_name, field_name, old_type, new_type
                )
            }
            SchemaChange::ChangeFieldNullability {
                model_name,
                field_name,
                old_nullable: _,
                new_nullable,
            } => {
                let change = if *new_nullable {
                    "nullable"
                } else {
                    "non-nullable"
                };
                let breaking = if !new_nullable {
                    " (⚠️  BREAKING)"
                } else {
                    ""
                };
                format!(
                    "Make '{}.{}' {}{}",
                    model_name, field_name, change, breaking
                )
            }
            SchemaChange::RenameField {
                model_name,
                old_name,
                new_name,
            } => {
                format!(
                    "Rename field '{}.{}' to '{}'",
                    model_name, old_name, new_name
                )
            }
            SchemaChange::RenameModel { old_name, new_name } => {
                format!("Rename model '{}' to '{}'", old_name, new_name)
            }
            SchemaChange::AddIndex {
                model_name,
                field_name,
                index_type,
            } => {
                format!(
                    "Add {} index on '{}.{}'",
                    index_type, model_name, field_name
                )
            }
            SchemaChange::RemoveIndex {
                model_name,
                field_name,
            } => {
                format!("Remove index from '{}.{}'", model_name, field_name)
            }
            SchemaChange::AddUniqueConstraint {
                model_name,
                field_name,
            } => {
                format!(
                    "Add unique constraint to '{}.{}' (⚠️  may fail)",
                    model_name, field_name
                )
            }
            SchemaChange::RemoveUniqueConstraint {
                model_name,
                field_name,
            } => {
                format!(
                    "Remove unique constraint from '{}.{}'",
                    model_name, field_name
                )
            }
            SchemaChange::AddCompositeIndex { model_name, fields } => {
                format!(
                    "Add composite index on '{}.{}'",
                    model_name,
                    fields.join(", ")
                )
            }
            SchemaChange::RemoveCompositeIndex { model_name, fields } => {
                format!(
                    "Remove composite index from '{}.{}'",
                    model_name,
                    fields.join(", ")
                )
            }
            SchemaChange::AddConstraint {
                model_name,
                field_name,
                constraint_name,
                ..
            } => {
                format!(
                    "Add @{} constraint to '{}.{}'",
                    constraint_name, model_name, field_name
                )
            }
            SchemaChange::RemoveConstraint {
                model_name,
                field_name,
                constraint_name,
            } => {
                format!(
                    "Remove @{} constraint from '{}.{}'",
                    constraint_name, model_name, field_name
                )
            }
            // #438. The description names the *stored* consequence, not the
            // edit: "reorder" reads as a formatting change, and the operator's
            // whole decision hangs on knowing that every already-written row
            // re-maps.
            SchemaChange::ChangeEnumVariants {
                model_name,
                field_name,
                enum_name,
                old_variants,
                new_variants,
            } => {
                let breaking =
                    Self::breaking_marker(Self::enum_verdict(old_variants, new_variants).0);
                let what = match classify_positional(old_variants, new_variants) {
                    PositionalDelta::Unchanged => "unchanged".to_string(),
                    PositionalDelta::Appended { added } => {
                        format!("append {}", added.join(", "))
                    }
                    PositionalDelta::Renamed { old_name, new_name } => format!(
                        "RENAME '{}' to '{}' — rows holding the old name have nothing to decode into",
                        old_name, new_name
                    ),
                    PositionalDelta::Reordered { moved } => format!(
                        "REORDER {} — every stored discriminant re-maps",
                        moved.join(", ")
                    ),
                    PositionalDelta::Dropped { dropped } => format!(
                        "REMOVE {} — stored discriminants re-map and some fall out of range",
                        dropped.join(", ")
                    ),
                };
                format!(
                    "Enum '{}' behind '{}.{}': {}{}",
                    enum_name, model_name, field_name, what, breaking
                )
            }
            SchemaChange::ChangeStructLayout {
                model_name,
                field_name,
                struct_name,
                old_fields,
                new_fields,
            } => {
                let breaking =
                    Self::breaking_marker(Self::struct_verdict(old_fields, new_fields).0);
                let what = match classify_layout(old_fields, new_fields) {
                    LayoutDelta::Retyped { fields } => format!(
                        "RETYPE {} — the stored bytes are reinterpreted",
                        fields.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Unchanged) => "unchanged".to_string(),
                    LayoutDelta::Names(PositionalDelta::Appended { added }) => format!(
                        "ADD {} — the row width changes, so every stored row re-frames",
                        added.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Renamed { old_name, new_name }) => format!(
                        "RENAME '{}' to '{}' — the old JSON key no longer decodes",
                        old_name, new_name
                    ),
                    LayoutDelta::Names(PositionalDelta::Reordered { moved }) => format!(
                        "REORDER {} — every field reads its neighbour's bytes",
                        moved.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Dropped { dropped }) => format!(
                        "REMOVE {} — the row width shrinks, so every stored row re-frames",
                        dropped.join(", ")
                    ),
                };
                format!(
                    "Struct '{}' behind '{}.{}': {}{}",
                    struct_name, model_name, field_name, what, breaking
                )
            }
        }
    }

    /// The shared breaking marker the descriptions above append.
    fn breaking_marker(breaking: bool) -> &'static str {
        if breaking { " (⚠️  BREAKING)" } else { "" }
    }
}

/// Represents a migration file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// Unique identifier (timestamp-based)
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// List of changes in this migration
    pub changes: Vec<SchemaChange>,
    /// Checksum of the migration file for integrity
    pub checksum: String,
    /// On-disk schema serial this migration expects BEFORE it runs (#74
    /// Phase 2 — the serial version interlock).  `0` for a legacy record written
    /// before versioning existed; a real lineage is contiguous (`to_version` of
    /// one migration == `from_version` of the next).
    #[serde(default)]
    pub from_version: u32,
    /// On-disk schema serial this migration stamps AFTER it runs.  The current
    /// expected version of the whole database is the highest `to_version` in the
    /// lineage (see [`MigrationLineage`](crate::MigrationLineage)); that is what
    /// codegen bakes into `EXPECTED_SCHEMA_VERSION` (red line #8 — lineage-sourced,
    /// never hand-edited).
    #[serde(default)]
    pub to_version: u32,
}

/// What a build can say about a migration file's stored checksum (#366).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChecksumStatus {
    /// Recomputed and matched.
    Verified,
    /// Recomputed and did NOT match — the file changed after it was written. The only
    /// one of these four that means what the old error message said.
    Mismatch,
    /// Written before #366, by a `DefaultHasher` value that is meaningless outside the
    /// compiler that produced it. Nothing can check it, and nothing is wrong with it.
    Unverifiable,
    /// Tagged with a digest this build does not know — written by a NEWER forgedb.
    UnknownAlgorithm(String),
}

impl Migration {
    /// Create a new migration (version fields defaulted to `0` — used by callers
    /// that do not track the lineage, and by the crate's own unit tests).
    pub fn new(description: String, changes: Vec<SchemaChange>) -> Self {
        Self::new_versioned(description, changes, 0, 0)
    }

    /// Create a new migration stamped with its serial version interlock (#74
    /// Phase 2).  `from_version`/`to_version` come from the committed lineage at
    /// `migrate create` time; the checksum covers them like every other field.
    pub fn new_versioned(
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Self {
        let now = Utc::now();
        let id = format!("{}", now.format("%Y%m%d%H%M%S"));

        let mut migration = Migration {
            id: id.clone(),
            description,
            created_at: now,
            changes,
            checksum: String::new(),
            from_version,
            to_version,
        };

        // Calculate checksum
        migration.checksum = migration.calculate_checksum();
        migration
    }

    /// Classify each change's hop body (#74 Phase 2).  A migration is "fully
    /// automatic" when every hop is [`HopBodyClass::Auto`]; any [`Authored`]
    /// residue must be authored + frozen before the transformer can be generated.
    ///
    /// [`Authored`]: HopBodyClass::Authored
    pub fn authored_changes(&self) -> Vec<&SchemaChange> {
        self.changes
            .iter()
            .filter(|c| c.hop_body_class() == HopBodyClass::Authored)
            .collect()
    }

    /// Calculate checksum for migration integrity
    fn calculate_checksum(&self) -> String {
        let mut temp = self.clone();
        temp.checksum = String::new();
        let json = serde_json::to_string(&temp).unwrap_or_default();
        checksum::compute(json.as_bytes())
    }

    /// Verify migration integrity.
    ///
    /// Returns `true` for a file this build cannot verify as well as for one it verifies
    /// successfully — see [`Migration::checksum_status`] for the distinction, which the
    /// loader reports and this bool deliberately flattens for existing callers.
    pub fn verify_checksum(&self) -> bool {
        // Verified and Unverifiable both load; Mismatch and UnknownAlgorithm do not.
        // Written as a POSITIVE list on purpose: the negative form (`!= Mismatch`) let
        // UnknownAlgorithm through here while the loader rejected it, so the bool and
        // the loader disagreed about the same file. A new variant must now be classified
        // deliberately rather than defaulting to "fine".
        matches!(
            self.checksum_status(),
            ChecksumStatus::Verified | ChecksumStatus::Unverifiable
        )
    }

    /// What this build can actually say about the stored checksum (#366).
    ///
    /// Three answers, not two. Collapsing them is what made the old failure mode so
    /// misleading: an unverifiable file and a modified file are not the same event, and
    /// only one of them is the user's problem.
    pub fn checksum_status(&self) -> ChecksumStatus {
        match checksum::classify(&self.checksum) {
            checksum::Kind::Current => {
                if self.checksum == self.calculate_checksum() {
                    ChecksumStatus::Verified
                } else {
                    ChecksumStatus::Mismatch
                }
            }
            checksum::Kind::Legacy => ChecksumStatus::Unverifiable,
            checksum::Kind::Unknown(algo) => ChecksumStatus::UnknownAlgorithm(algo.to_string()),
        }
    }

    /// Get the filename for this migration
    pub fn filename(&self) -> String {
        let safe_desc = self
            .description
            .to_lowercase()
            .replace(' ', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        format!("{}_{}.json", self.id, safe_desc)
    }

    /// Check if migration has breaking changes
    pub fn has_breaking_changes(&self) -> bool {
        self.changes.iter().any(|c| c.is_breaking())
    }

    /// Get list of breaking changes
    pub fn breaking_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| c.is_breaking()).collect()
    }

    /// Get list of safe changes
    pub fn safe_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| !c.is_breaking()).collect()
    }
}

/// Migration status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub migration_id: String,
    pub applied_at: DateTime<Utc>,
    pub checksum: String,
}

/// Migration state file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub applied_migrations: Vec<MigrationRecord>,
}

impl Default for MigrationState {
    fn default() -> Self {
        MigrationState {
            applied_migrations: Vec::new(),
        }
    }
}

impl MigrationState {
    /// Check if a migration has been applied
    pub fn is_applied(&self, migration_id: &str) -> bool {
        self.applied_migrations
            .iter()
            .any(|r| r.migration_id == migration_id)
    }

    /// Add a migration record
    pub fn add_migration(&mut self, migration_id: String, checksum: String) {
        self.applied_migrations.push(MigrationRecord {
            migration_id,
            applied_at: Utc::now(),
            checksum,
        });
    }

    /// Remove the last migration record (for rollback)
    pub fn remove_last_migration(&mut self) -> Option<MigrationRecord> {
        self.applied_migrations.pop()
    }

    /// Get the last applied migration
    pub fn last_migration(&self) -> Option<&MigrationRecord> {
        self.applied_migrations.last()
    }
}

/// The migration-file checksum: a **specified** digest, tagged with its own name.
///
/// ## What was here before
///
/// A module called `md5` that was not MD5. It wrapped `DefaultHasher`, whose algorithm
/// std explicitly does not guarantee across releases — and the value it produced is
/// written into the migration JSON and verified on load. So the checksum was only
/// meaningful to the exact compiler that computed it.
///
/// `cargo install forgedb` builds with whatever toolchain the user has;
/// `rust-toolchain.toml` pins this repo's builds and does not reach an installed user. A
/// rustup upgrade, or two developers on one repo, was enough to make every committed
/// migration file fail to load — reported as
/// `"file may be corrupted"`, which sends you to look for disk damage, the one thing that
/// did not happen (#366).
///
/// The name is part of the defect, not incidental to it: a thing called `md5` that is not
/// MD5 is how this survived review.
///
/// ## Why FNV-1a and not SHA-2
///
/// This detects accidental edits to a file the user committed. It is not an adversarial
/// integrity boundary — anyone who can rewrite the migration can rewrite the checksum
/// beside it, whatever the algorithm. Priced against the shipped graph, `sha2` is +7
/// crates and `twox-hash` +1, against +0 for a specified constant-driven loop.
///
/// ## Why this is NOT shared with `cache::member_hash`
///
/// `src/cache.rs` also implements FNV-1a, and deliberately keeps its own copy. Two
/// reasons, and the first is structural: `crates/migrations` cannot depend on the root
/// crate, which depends on it. The second is that they are the same *algorithm* serving
/// unrelated *contracts* — one keys build-cache directories, this one detects edits to a
/// user's committed file. Sharing them would couple two stability guarantees that have no
/// reason to move together, and each is pinned by its own golden vectors.
pub mod checksum {
    /// The tag written into every checksum this module produces.
    ///
    /// Load-bearing: it is what lets a reader tell "hashed by a version that predates
    /// #366" from "this file was edited". Without it, fixing the algorithm would make
    /// every existing migration file fail with the same misleading corruption error the
    /// fix exists to remove — the fix would detonate exactly the artifact it protects.
    pub const TAG: &str = "fnv1a64";

    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;

    /// FNV-1a (64-bit), rendered as 16 lowercase hex digits behind the tag.
    ///
    /// Specified here in full rather than delegated, because the whole point is that the
    /// bytes do not move when something else does.
    pub fn compute(data: &[u8]) -> String {
        let mut hash = FNV_OFFSET_BASIS;
        for byte in data {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        format!("{TAG}:{hash:016x}")
    }

    /// How a stored checksum relates to what this version can compute.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Kind<'a> {
        /// Written by this algorithm — comparable.
        Current,
        /// No tag at all: written before #366, by a `DefaultHasher` value that is
        /// meaningless outside the compiler that produced it. Unverifiable, not wrong.
        Legacy,
        /// Tagged with something this build does not know — written by a NEWER forgedb.
        /// Distinct from `Legacy` on purpose: downgrading is a real situation and
        /// silently accepting an unknown digest would be the wrong answer.
        Unknown(&'a str),
    }

    pub fn classify(stored: &str) -> Kind<'_> {
        match stored.split_once(':') {
            Some((TAG, _)) => Kind::Current,
            Some((other, _)) => Kind::Unknown(other),
            None => Kind::Legacy,
        }
    }
}
