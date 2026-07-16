//! Rust database code generator
//!
//! Generates optimized Rust code with columnar storage for ForgeDB schemas.

use crate::{CodegenError, GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Rust code generator
pub struct RustGenerator;

/// On-delete referential-integrity policy for a relation FK field (delete
/// semantics).  Declared via `@on_delete(...)`; `Restrict` is the default when
/// the directive is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnDeletePolicy {
    /// Refuse to delete a parent that still has any live child referencing it
    /// (→ `ValidationError::ReferencedByChildren`, 409).  The safe default.
    Restrict,
    /// Deleting the parent recursively deletes every referencing child (their
    /// own on-delete rules fire in turn).
    Cascade,
    /// Deleting the parent sets each referencing child's (optional) FK to `None`.
    SetNull,
}

impl RustGenerator {
    /// Generate Rust database implementation from schema
    ///
    /// # Arguments
    ///
    /// * `schema` - Parsed schema AST
    ///
    /// # Returns
    ///
    /// Generated Rust code as a string
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        // Baseline on-disk format version (#74 Phase 1): a schema with no migration
        // lineage is at version 1.  The CLI threads the lineage-derived version via
        // `generate_with_format_version`; snapshot tests use this baseline so their
        // output is stable.
        Self::generate_with_format_version(schema, 1)
    }

    /// Generate the Rust database implementation, baking `format_version` into the
    /// app's `EXPECTED_FORMAT_VERSION` (#74 Phase 1/2).  The version is **derived
    /// from the committed migration lineage** by the `generate` CLI command (red
    /// line #8: lineage-sourced, never hand-edited); it is an opaque integer the
    /// generated open guard compares and refuses on mismatch — nothing more.
    pub fn generate_with_format_version(
        schema: &Schema,
        format_version: u32,
    ) -> Result<GeneratedCode> {
        let code = Self::generate_code(schema, format_version)?;

        Ok(GeneratedCode {
            code,
            description: format!(
                "Rust database implementation ({} models)",
                schema.models.len()
            ),
        })
    }

    /// Whether a field can appear in a `@projection` (#113): it must have a
    /// storage column, i.e. be a stored scalar, char/array/struct, or FK scalar.
    /// Relation collections (`OneToMany`/`ManyToMany`), virtual `()` relation
    /// fields, and component refs have no column and are NOT projectable — this
    /// is exactly the set for which `field_read_stmt` returns `None`.
    fn is_projectable(field: &forgedb_parser::Field) -> bool {
        Self::is_variable_column_type(&field.field_type)
            || Self::is_fixed_size_type(&field.field_type)
    }

    /// Compile-time validation of `@projection` directives (#113, PM constraint 2):
    /// every projected field must resolve to a projectable (column-backed) field.
    /// The parser already guarantees the field exists and names are unique; here
    /// we reject relation / virtual / component fields with a clear error.
    fn validate_projections(schema: &Schema) -> Result<()> {
        for model in &schema.models {
            for proj in &model.projections {
                for fname in &proj.fields {
                    let field = model
                        .fields
                        .iter()
                        .find(|f| &f.name == fname)
                        .ok_or_else(|| {
                            CodegenError::InvalidSchema(format!(
                                "@projection '{}' on model '{}' references undefined field '{}'",
                                proj.name, model.name, fname
                            ))
                        })?;
                    if !Self::is_projectable(field) {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@projection '{}' on model '{}' cannot include field '{}': \
                             relation and virtual fields have no column and are not projectable \
                             (use eager-load / relation traversal for related records)",
                            proj.name, model.name, fname
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// The on-delete referential-integrity policy declared on a relation FK field
    /// via `@on_delete(...)` (delete semantics).  Absent → `Restrict` (the safe
    /// default: refuse to delete a parent that still has live children).
    ///
    /// Read purely from `field.constraints` — `@on_delete(cascade)` parses as a
    /// generic directive (bare-identifier arg), so re-introducing it needs no
    /// parser change; the policy lives entirely in generated code.
    fn on_delete_policy(field: &forgedb_parser::ast::Field) -> OnDeletePolicy {
        for c in &field.constraints {
            if c.name == "on_delete" {
                if let Some(forgedb_parser::ast::ConstraintParam::String(p)) = c.params.first() {
                    return match p.as_str() {
                        "cascade" => OnDeletePolicy::Cascade,
                        "set_null" => OnDeletePolicy::SetNull,
                        _ => OnDeletePolicy::Restrict,
                    };
                }
            }
        }
        OnDeletePolicy::Restrict
    }

    /// Compile-time validation of `@on_delete(...)` directives (delete semantics):
    /// - the value must be one of `restrict` / `cascade` / `set_null`;
    /// - `@on_delete` is only meaningful on a relation FK field (`*Target` /
    ///   `?Target`) — reject it elsewhere;
    /// - `set_null` is only valid on an OPTIONAL FK (`?Target`) — a required
    ///   `*Target` cannot be nulled, so this is a HARD codegen error.
    fn validate_on_delete(schema: &Schema) -> Result<()> {
        for model in &schema.models {
            for field in &model.fields {
                let Some(c) = field.constraints.iter().find(|c| c.name == "on_delete") else {
                    continue;
                };
                let value = match c.params.first() {
                    Some(forgedb_parser::ast::ConstraintParam::String(p)) => p.as_str(),
                    _ => {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@on_delete on '{}.{}' requires a policy argument \
                             (restrict | cascade | set_null)",
                            model.name, field.name
                        )));
                    }
                };
                if !matches!(value, "restrict" | "cascade" | "set_null") {
                    return Err(CodegenError::InvalidSchema(format!(
                        "@on_delete('{}') on '{}.{}' is not a valid policy \
                         (expected restrict | cascade | set_null)",
                        value, model.name, field.name
                    )));
                }
                let optional_fk = match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(_),
                    ) => true,
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(_),
                    ) => false,
                    _ => {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@on_delete on '{}.{}' is only valid on a foreign-key field \
                             (`*Target` required or `?Target` optional)",
                            model.name, field.name
                        )));
                    }
                };
                if value == "set_null" && !optional_fk {
                    return Err(CodegenError::InvalidSchema(format!(
                        "@on_delete(set_null) on '{}.{}' is invalid: the field is a required \
                         foreign key (`*`) and cannot be set to null — use an optional FK \
                         (`?Target`), or choose `cascade`/`restrict`",
                        model.name, field.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Generate the Rust code as a string
    fn generate_code(schema: &Schema, format_version: u32) -> Result<String> {
        Self::validate_projections(schema)?;
        Self::validate_on_delete(schema)?;

        let mut tokens = TokenStream::new();

        // File header comments
        let header = quote! {
            //! Generated by ForgeDB
            //! DO NOT EDIT - This file is auto-generated
        };
        tokens.extend(header);

        // Attributes and imports
        let imports = quote! {
            // `irrefutable_let_patterns`: the WAL recovery path matches
            // `WalOperation::Raw` via `if let` — currently the only variant, so
            // the pattern is irrefutable, but the `if let` stays forward-compatible
            // if the op set ever grows again (#89 durable write path).
            #![allow(dead_code, unused_imports, irrefutable_let_patterns)]

            use std::collections::HashMap;
            use std::path::{Path, PathBuf};
            use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};
            use forgedb_types::{Uuid, Timestamp, Value};
            use serde::{Deserialize, Serialize};
            use utoipa::ToSchema;
        };
        tokens.extend(imports);

        // WAL checkpoint interval (#89 step 2 — bound the WAL).  After this many
        // WAL-recorded mutations a storage fsyncs its columns and truncates its
        // WAL, so the WAL never grows without limit and reopen never replays a
        // huge WAL.  Fixed for v1 (not yet configurable — the same posture as the
        // fixed `FsyncPolicy::Always`).  Bounds the WAL to ~this many records
        // between checkpoints (bytes are bounded by interval × max row size).
        tokens.extend(quote! {
            const WAL_CHECKPOINT_INTERVAL: u64 = 1000;
        });

        // Compaction trigger threshold (#92 Phase 4 — bound column storage).
        // Each update/delete leaves a dead (superseded/tombstoned) row version
        // behind (#66); once this many accumulate since the last compaction, the
        // storage compacts IN-PROCESS under the single-writer lock, reclaiming the
        // dead bytes.  Fixed for v1 (not yet configurable — the same posture as
        // `WAL_CHECKPOINT_INTERVAL` and `FsyncPolicy::Always`; a tuning knob rides
        // the Phase 4 bounded-storage follow-up).  Storage is bounded to roughly
        // this many dead versions between compactions.
        tokens.extend(quote! {
            const COMPACTION_DEAD_THRESHOLD: u64 = 1000;
        });

        // On-disk format version this binary was generated for (#74 Phase 1 —
        // version guard).  An OPAQUE lineage-derived integer: it is compared, on
        // open, against the `format_version` stamped in each collection's
        // `manifest.json`, and a mismatch is a fail-fast refusal — never a
        // self-healing reshape (red line DV-6: the guard reads ONE integer and
        // refuses; it never inspects column names/types to adapt).  A migration
        // bin (the offline transformer, #74 Phase 3) is what rewrites a data dir
        // from an old version to a new one; the app never migrates in place.
        // Derived from the committed migration lineage (#74 Phase 2 — the CLI
        // threads `MigrationLineage::current_format_version`); baseline `1` for a
        // schema with no lineage.  A fresh data dir is stamped with exactly this
        // value, so a dir this binary wrote always matches its own expectation.
        let __expected_format_version =
            proc_macro2::Literal::u32_unsuffixed(format_version);
        tokens.extend(quote! {
            const EXPECTED_FORMAT_VERSION: u32 = #__expected_format_version;
        });

        // Data-integrity error type (#91 Phase 3).  Generated once per schema; a
        // generic *shape* (three integrity classes) but every field name, rule,
        // and message is filled in by generated per-field checks — there is no
        // runtime schema-reading validator.  Carried out of `insert`/`update`
        // (and the `Database::create_*`/`update_*` wrappers) so a write that would
        // violate integrity is refused, not committed.
        tokens.extend(quote! {
            /// A rejected write (#91).  `status_code()` maps each class to the
            /// HTTP status the generated REST boundary returns.
            #[derive(Debug, Clone, PartialEq)]
            pub enum ValidationError {
                /// A `&unique` field already holds this value on another row → 409.
                Unique { field: &'static str },
                /// A required/optional foreign key points at a non-existent row → 409.
                DanglingReference { field: &'static str, target: &'static str },
                /// A `delete` was refused because live child rows still reference
                /// this parent under an `@on_delete(restrict)` FK (the default
                /// on-delete policy) → 409.
                ReferencedByChildren { model: &'static str, field: &'static str },
                /// A field-level constraint (`@min`/`@max`/`@length`/`@email`/`@url`)
                /// was violated → 422.
                Constraint { field: &'static str, rule: &'static str, message: String },
            }

            impl ValidationError {
                /// The HTTP status the generated API boundary returns for this class:
                /// integrity conflicts (unique / dangling FK) → 409, field-constraint
                /// violations → 422.
                pub fn status_code(&self) -> u16 {
                    match self {
                        ValidationError::Unique { .. }
                        | ValidationError::DanglingReference { .. }
                        | ValidationError::ReferencedByChildren { .. } => 409,
                        ValidationError::Constraint { .. } => 422,
                    }
                }
            }

            impl std::fmt::Display for ValidationError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        ValidationError::Unique { field } => {
                            write!(f, "unique constraint violated on field `{}`", field)
                        }
                        ValidationError::DanglingReference { field, target } => {
                            write!(f, "field `{}` references a non-existent {}", field, target)
                        }
                        ValidationError::ReferencedByChildren { model, field } => {
                            write!(
                                f,
                                "cannot delete: {} rows still reference it via `{}` (on_delete=restrict)",
                                model, field
                            )
                        }
                        ValidationError::Constraint { field, rule, message } => {
                            write!(f, "field `{}` violates `{}`: {}", field, rule, message)
                        }
                    }
                }
            }

            impl std::error::Error for ValidationError {}
        });

        // Replication apply error (#110 browser read-replica follower).  The
        // follower decodes an opaque `PersistedEvent` and replays it through the
        // SAME generated mutation surface the server uses (`insert`/`update`/
        // `delete`); this is the union of what that replay can fail with.  Like
        // `ValidationError` it is a fixed *shape* — no schema is read at runtime.
        tokens.extend(quote! {
            /// An error applying a replicated change frame on the read-replica
            /// follower (#110 Milestone C).  Emitted by the generated
            /// `<Model>Storage::apply` / `Database::apply_frame`.
            #[derive(Debug)]
            pub enum ApplyError {
                /// The opaque row bytes did not deserialize into the model record
                /// (a corrupt frame, or a schema skew between server and replica).
                Decode(String),
                /// The replayed write failed the generated integrity validators.
                /// Should not happen for a faithful replica of already-valid data,
                /// but is surfaced rather than silently dropped.
                Validation(ValidationError),
            }

            impl std::fmt::Display for ApplyError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        ApplyError::Decode(m) => {
                            write!(f, "failed to decode replication frame: {}", m)
                        }
                        ApplyError::Validation(e) => {
                            write!(f, "replicated write rejected: {}", e)
                        }
                    }
                }
            }

            impl std::error::Error for ApplyError {}

            impl From<ValidationError> for ApplyError {
                fn from(e: ValidationError) -> Self {
                    ApplyError::Validation(e)
                }
            }
        });

        // Transaction error type (MVCC Tier 1, #83).  A transaction's scoped
        // write methods call the SAME generated validated write path
        // (`validate_<model>` + FK / `&unique` checks), so a `?` on any staged
        // write propagates a #91 `ValidationError` and the transaction rolls back
        // — one validator, no second copy.  `Io` surfaces a WAL/journal write
        // failure; `Conflict` is reserved for Tier 2 optimistic concurrency (the
        // `try_commit` loser) and is unused in Tier 1.
        tokens.extend(quote! {
            /// A rejected or failed transaction (MVCC Tier 1, #83).  Wraps the #91
            /// `ValidationError` (so `?` on a scoped write rolls the txn back),
            /// plus an `Io` variant for a durable-write failure and a `Conflict`
            /// variant reserved for Tier 2 optimistic concurrency.
            #[derive(Debug)]
            pub enum TxError {
                /// A staged write failed the generated integrity validators
                /// (field constraints / `&unique` / FK existence).  The whole
                /// transaction rolls back.
                Validation(ValidationError),
                /// A durable-write failure while committing (WAL/journal fsync).
                Io(String),
                /// A write-write conflict lost the serialized commit (Tier 2).
                /// Never produced by Tier 1's single-writer transactions.
                Conflict,
                /// The Tier-3 commit coordinator is unreachable — either
                /// `Database::connect` could not establish the turn-channel, or a
                /// live commit exhausted its retries against a down/stuck
                /// coordinator.  The data dir is NOT opened lock-free unless the
                /// coordinator is live (MVCC spec T3-5 / PM re-gate #84), so this
                /// never leaves an unserialized writer running.
                CoordinatorUnavailable(String),
            }

            impl TxError {
                /// The HTTP status a generated boundary would return: a validation
                /// failure maps to its #91 status (409/422); an I/O or conflict
                /// failure is a 500/409 server-side condition.
                pub fn status_code(&self) -> u16 {
                    match self {
                        TxError::Validation(e) => e.status_code(),
                        TxError::Io(_) => 500,
                        TxError::Conflict => 409,
                        // Coordinator down ⇒ writes are temporarily unavailable.
                        TxError::CoordinatorUnavailable(_) => 503,
                    }
                }
            }

            impl std::fmt::Display for TxError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        TxError::Validation(e) => write!(f, "transaction rejected: {}", e),
                        TxError::Io(m) => write!(f, "transaction I/O failure: {}", m),
                        TxError::Conflict => write!(f, "transaction conflict (retry)"),
                        TxError::CoordinatorUnavailable(m) => {
                            write!(f, "commit coordinator unavailable: {}", m)
                        }
                    }
                }
            }

            impl std::error::Error for TxError {}

            impl From<ValidationError> for TxError {
                fn from(e: ValidationError) -> Self {
                    TxError::Validation(e)
                }
            }
        });

        // Generate embedded struct definitions before the models that reference
        // them.  Models store structs as raw fixed-size bytes, so the struct
        // types must exist for the emitted `size_of`/`read_unaligned` code to
        // compile (without this, struct-typed fields fail with E0425).
        for struct_def in &schema.structs {
            let struct_tokens = Self::generate_struct(struct_def);
            tokens.extend(struct_tokens);
        }

        // Generate user-declared enum definitions (#enum) before the models that
        // reference them.  Each becomes a fieldless Rust enum (serialized as its
        // variant name string) plus a private `__to_u8`/`__from_u8` pair that maps
        // the variant to/from its 1-byte stored discriminant (`0..N` in declaration
        // order) — the encode/decode path in `insert`/`read_at` calls these so the
        // variant list lives in exactly one place.
        for enum_def in &schema.enums {
            tokens.extend(Self::generate_enum(enum_def)?);
        }

        // Generate models
        for model in &schema.models {
            let model_tokens = Self::generate_model(model, schema)?;
            tokens.extend(model_tokens);
        }

        // Generate per-model change-feed event structs (#62 Direction A + #66
        // mutation surface): the typed payloads the WS handler emits.  The substrate
        // feed only carries (model, row_index, kind); generated code turns that into
        // `<Model>Inserted` / `<Model>Updated` / `<Model>Deleted`.
        for model in &schema.models {
            tokens.extend(Self::generate_change_event_structs(model));
        }

        // Generate per-model live-query delta enums (#62 Direction B): the typed,
        // removal-aware result-set deltas the `/live-query/<model>` WS handler
        // pushes (`Init` / `Added` / `Removed` / `Updated`).  Typed model records
        // + the model's own id type — never an arbitrary runtime value.
        for model in &schema.models {
            tokens.extend(Self::generate_live_delta_enum(model));
        }

        // Generate M2M junction storage structs (referenced by the Database
        // struct fields, so emitted before it).
        let junction_tokens = Self::generate_junction_structs(schema);
        tokens.extend(junction_tokens);

        // Generate database struct
        let db_tokens = Self::generate_database(schema)?;
        tokens.extend(db_tokens);

        // Generate relation-traversal helpers (forward FK getters, reverse
        // one-to-many collections, and many-to-many link/query methods) as a
        // second `impl Database` block.
        let traversal_tokens = Self::generate_traversal_impl(schema);
        tokens.extend(traversal_tokens);

        // Generate the Database-level validated write wrappers (#91 Phase 3):
        // create_<model> / update_<model>, which add foreign-key existence checks
        // (sibling-collection access) on top of the storage-level field + unique
        // validation, then delegate.  The REST boundary routes through these.
        let validated_writes = Self::generate_validated_writes(schema);
        tokens.extend(validated_writes);

        // Generate the transaction surface (MVCC Tier 1, #83): the `TxHandle`
        // struct + its scoped per-model write/read methods, `Database::transaction`,
        // and the commit/rollback machinery (atomic multi-model commit journal,
        // deferred-maintenance run, buffered changefeed/broker emit).
        let transaction_impl = Self::generate_transaction_impl(schema);
        tokens.extend(transaction_impl);

        // Generate the Tier 2 concurrent-prepare surface (#83): SharedDatabase +
        // ConcurrentTxHandle for genuine concurrent prepare (multiple threads prepare
        // concurrently, commit is serialized under the exclusive write lock).
        let shared_db_impl = Self::generate_shared_database_impl(schema);
        tokens.extend(shared_db_impl);

        // Generate the snapshot-scoped M2M traversal on the read-only
        // `DatabaseReader` (#56 Direction B), so a concurrent reader can resolve
        // cross-table many-to-many links consistently as of its snapshot.
        let reader_traversal_tokens = Self::generate_reader_traversal_impl(schema);
        tokens.extend(reader_traversal_tokens);

        // Generate eager-load typed structs (`<Model>WithRelations`) plus their
        // Database getters, which resolve a record's forward references in one call.
        let eager_tokens = Self::generate_eager_load(schema);
        tokens.extend(eager_tokens);

        // Generate the Tier 3 coordinator client surface (#84): CoordinatedDatabase
        // wraps SharedDatabase and routes the serialized commit section through the
        // coordinator socket instead of the in-process sequencer.
        let coord_tokens = Self::generate_coordinated_client(schema);
        tokens.extend(coord_tokens);

        // Parse and format with prettyplease
        let syntax_tree = syn::parse_file(&tokens.to_string())
            .map_err(|e| crate::CodegenError::GenerationFailed(format!("Failed to parse generated code: {}", e)))?;

        Ok(prettyplease::unparse(&syntax_tree))
    }

    /// Generate a single model struct
    fn generate_model(model: &forgedb_parser::Model, schema: &Schema) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let storage_name = format_ident!("{}Storage", model.name);
        let model_doc = format!("{} model", model.name);
        let storage_doc = format!("Storage for {} model", model.name);

        // Generate field definitions
        let fields: Vec<_> = model
            .fields
            .iter()
            .map(Self::model_struct_field)
            .collect();

        // Generate columnar storage fields
        let storage_fields = Self::generate_storage_fields(model);

        // Generate storage initialization
        let storage_inits = Self::generate_storage_inits(model, schema);

        // The model's identity type (Uuid for uuid PKs, u64/u32 for integer PKs).
        let id_type = Self::id_type_tokens(model);

        // Generate the field-constraint validator (#91 Phase 3), a free fn the
        // generated insert/update call before committing.
        let field_validation = Self::generate_field_validation(model);

        // Generate insert logic
        let insert_logic = Self::generate_insert_logic(model);

        // Generate mutation surface (#66): superseding-version `update` / `delete`.
        // `None` (skipped) for a model with no id field, which cannot be mutated by
        // id (and cannot `insert` either).
        let mutation_methods = match (
            Self::generate_update_logic(model),
            Self::generate_delete_logic(model),
        ) {
            (Some(update_logic), Some(delete_logic)) => quote! {
                /// Append a superseding version of an existing record (#66).
                /// Reads resolve newest-version-wins; the prior version's bytes are
                /// never mutated, so append-only (and thus backup / snapshots) hold.
                /// Validates field constraints + `&unique` FIRST (#91): a rejected
                /// update commits nothing and returns `Err(ValidationError)`.
                /// `Ok(false)` if `id` is absent; `Ok(true)` on a successful update.
                /// Storage grows with each update until compaction reclaims versions.
                /// (Foreign-key existence is checked by `Database::update_<model>`,
                /// which has sibling-collection access this storage-scoped method lacks.)
                pub fn update(&mut self, id: #id_type, record: #model_name) -> Result<bool, ValidationError> {
                    #update_logic
                }

                /// Logically delete a record by appending a tombstoned superseding
                /// version (#66).  `get` then reads the row as absent.  Returns
                /// `false` if `id` is already absent.  Prior versions stay committed
                /// (backup-faithful) until compaction reclaims them.
                pub fn delete(&mut self, id: #id_type) -> bool {
                    #delete_logic
                }
            },
            _ => quote! {},
        };

        // Replication apply surface (#110 Milestone C): the follower's per-model
        // entry point, replaying an opaque frame through the same insert/update/
        // delete above.  `None` (skipped) for a model with no id (cannot mutate).
        let apply_method = Self::generate_apply_method(model).unwrap_or_else(|| quote! {});
        // Additive commit (#110): flush every column + tombstone (native fsync;
        // wasm arena no-op — durability is the transport's IndexedDB/OPFS write).
        let commit_method = Self::generate_commit_method(model);

        // Snapshot-scoped newest-version resolution (#66 + #56): read the id at a
        // physical row so `get_at` / `all_at` can resolve the newest version
        // *within a watermark* — a snapshot captured before an update/delete still
        // sees the version live as-of capture.  Present only for id-bearing models.
        let row_index_ident = format_ident!("row_index");
        let id_read_at_row =
            Self::generate_id_read_expr(model, &quote! { self }, &row_index_ident);

        // Generate the snapshot accessors (`get_at` / `all_at`).  For an id-bearing
        // model they resolve newest-within-watermark (mutation-aware); a model with
        // no id keeps the plain prefix scan (it cannot be mutated, so no duplicate
        // versions ever exist).
        let snapshot_accessors = Self::generate_snapshot_accessors(
            &id_type,
            &model_name,
            id_read_at_row.as_ref(),
        );

        // Generate the shared read-by-row-index logic (feeds get / get_at / all_at)
        let read_at_logic = Self::generate_read_at_logic(model);

        // Generate declared column projections (#113): per @projection, a tailored
        // struct + narrow reads that touch only PK + selected columns.  Reuses the
        // shared `field_read_stmt` decode (one read path) and the id-at-row
        // expression for the snapshot `_at` variants.
        let (projection_structs, projection_methods) =
            Self::generate_projections(model, &id_type, id_read_at_row.as_ref());

        // Generate the secondary-index probe methods (#90): find_by_* / get_by_*
        // (+ snapshot `_at` variants) for each `^index` / `&unique` field.
        let index_lookups = Self::generate_index_lookups(model);

        // Generate reopen/rehydration logic (#65): rebuild the in-memory
        // `row_count` + `id_to_row` index from the persisted columns so a fresh
        // process can read data written by a previous one.
        let rehydrate_logic = Self::generate_rehydrate_logic(model);

        // Generate crash-recovery (#89): torn-tail repair + WAL replay, run in
        // `new_at` before the identity-index rebuild so `id_to_row` reflects the
        // recovered rows.
        let recover_method = Self::generate_recover_method(model);

        // Generate the WAL checkpoint (#89 step 2): fsync columns + truncate the
        // WAL so it stays bounded and reopen replays only the post-checkpoint tail.
        let checkpoint_method = Self::generate_checkpoint_method(model);

        // Generate in-process compaction (#92 Phase 4): reclaim dead row versions
        // under the writer lock, then reopen to rebuild the in-memory maps.
        let compact_method = Self::generate_compact_method(model);

        // Generate the transaction storage helpers (MVCC Tier 1, #83): the
        // `__stage_append` low-level staged-write path (WAL + columns + row_count,
        // NO id_to_row / index / changefeed / broker) the `TxHandle` uses, plus
        // `run_deferred_maintenance` (runs a checkpoint/compaction that the txn
        // guard deferred).  `None`-skipped for a model with no id (cannot mutate).
        let txn_storage_methods = Self::generate_txn_storage_methods(model);

        // Generate the per-model layout manifest writer (#57): physical column
        // layout + row-count anchor, written at open, read by schema-blind backup.
        let write_manifest = Self::generate_write_manifest(model);

        // Read-only reader handle (#56 Direction B): a per-model bundle of
        // read-only column views sharing the writer's file descriptors, so many
        // `&self` readers read the committed prefix lock-free while the single
        // `&mut self` writer keeps appending.  It reuses the *exact* `read_at` /
        // `get_at` / `all_at` token streams as the writer storage — identical
        // tailored column-decode, just reading through the shared-fd reader
        // columns — so there is no second decode path to drift.
        let reader_name = format_ident!("{}StorageReader", model.name);
        let reader_fields = Self::generate_reader_storage_fields(model);
        let reader_inits = Self::generate_reader_inits(model);
        // Reader index probes (#103): `_at`-only (no live `get` on a reader).
        let reader_index_probes = Self::generate_index_probes(model, false);
        let reader_doc = format!(
            "Read-only, lock-free reader handle for {} storage (#56 Direction B)",
            model.name
        );

        // Generate the model struct, storage struct, and implementation
        let tokens = quote! {
            #[doc = #model_doc]
            #[repr(C)]
            #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
            pub struct #model_name {
                #(#fields),*
            }

            #projection_structs

            #[doc = #storage_doc]
            pub struct #storage_name {
                #storage_fields
            }

            impl #storage_name {
                /// Open the model's storage, rehydrating the in-memory identity
                /// index from disk.  The column files are append-only and persist
                /// across processes; this reconstructs `row_count` and `id_to_row`
                /// so a fresh process reads data written by a previous one (#65).
                ///
                /// The row-count anchor is the tombstone file length (1 byte/row,
                /// appended last per insert), so a torn insert (columns written
                /// but the process died before the tombstone append) is excluded
                /// and its orphaned column tail is overwritten by the next insert.
                pub fn new() -> Self {
                    Self::new_at(std::path::Path::new("."))
                }

                /// Open the model's storage rooted at `root` (#59): every column
                /// and the tombstone file is joined under it.  `new()` passes `.`
                /// (the historical CWD-relative layout); a per-tenant process
                /// passes that tenant's data dir, so filesystem isolation is the
                /// directory boundary — no query logic, no schema read at runtime.
                pub fn new_at(root: &std::path::Path) -> Self {
                    let mut db = Self {
                        #storage_inits
                    };
                    // Crash recovery (#89): repair any torn column tail and replay
                    // the WAL BEFORE rebuilding the identity index, so `row_count`
                    // and `id_to_row` below reflect the recovered committed rows.
                    db.recover_from_wal();
                    #rehydrate_logic
                    // Refresh the physical-layout manifest on open (#57): cheap,
                    // off the insert hot path, gives schema-blind backup/inspector
                    // a current column map + row-count anchor.
                    db.write_manifest(root);
                    db
                }

                #write_manifest

                /// Attach a shared change feed (#62 Direction A).  Called by
                /// `Database::new()`; afterwards each `insert` emits a field-blind
                /// `(model, row_index)` signal into the feed.
                pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
                    self.changefeed = Some(feed);
                }

                /// Attach the shared durable replication broker (#82 Direction C).
                /// Called by `Database::open_at()`; afterwards each mutation is
                /// durably recorded (with the opaque row bytes + a global offset)
                /// for a resumable follower, in addition to the change-feed emit.
                pub fn attach_broker(
                    &mut self,
                    broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                ) {
                    self.broker = broker;
                }

                /// Insert a record.  Validates field constraints + `&unique` FIRST
                /// (#91 Phase 3): a rejected insert commits nothing and returns
                /// `Err(ValidationError)`; on success returns `Ok(id)`.  (Foreign-key
                /// existence is checked by `Database::create_<model>`, which has the
                /// sibling-collection access this storage-scoped method lacks.)
                pub fn insert(&mut self, record: #model_name) -> Result<#id_type, ValidationError> {
                    #insert_logic
                }

                #mutation_methods

                #apply_method

                #commit_method

                #recover_method

                #checkpoint_method

                #compact_method

                #txn_storage_methods

                /// Materialize the record at a physical row index, or `None` if
                /// the row is tombstoned.  Shared read path (#56): `get`,
                /// `get_at`, and `all_at` all funnel through here.  Public so the
                /// generated change-feed WS handler (#62) can turn a broadcast
                /// `(model, row_index)` signal into a typed record.
                pub fn read_at(&self, row_index: usize) -> Option<#model_name> {
                    #read_at_logic
                }

                pub fn get(&self, id: #id_type) -> Option<#model_name> {
                    let row_index = *self.id_to_row.get(&id)?;
                    self.read_at(row_index)
                }

                /// The number of physically committed rows (the snapshot
                /// watermark).  Append-only storage guarantees a row's index is
                /// stable for life, so this count fully defines a read view.
                pub fn row_count(&self) -> usize {
                    self.row_count
                }

                /// Capture a read snapshot at the current committed row count
                /// (#56, Direction A).  Rows appended after this call are
                /// invisible to accessors passed the returned snapshot.
                pub fn snapshot(&self) -> forgedb_storage::Snapshot {
                    forgedb_storage::Snapshot::new(self.row_count)
                }

                #snapshot_accessors

                /// Return every live (non-deleted) record.  Used by generated
                /// reverse-relation traversal helpers, which filter this by a
                /// foreign key.  Order is unspecified (follows the id map).
                pub fn all(&self) -> Vec<#model_name> {
                    let mut records = Vec::new();
                    for id in self.id_to_row.keys() {
                        if let Some(record) = self.get(*id) {
                            records.push(record);
                        }
                    }
                    records
                }

                #index_lookups

                // Declared column projections (#113): narrow reads over PK +
                // selected columns, plus their snapshot `_at` variants.
                #projection_methods

                /// Open a read-only handle over this storage (#56 Direction B).
                /// The handle shares this storage's column files via independent
                /// (`try_clone`d) descriptors, so it reads the committed prefix
                /// lock-free — concurrently with, and never blocking, the single
                /// `&mut self` writer.  Pair it with a `DatabaseSnapshot` captured
                /// on the writer for a cross-model-consistent read view.
                pub fn reader(&self) -> #reader_name {
                    #reader_name {
                        #reader_inits
                    }
                }
            }

            #[doc = #reader_doc]
            pub struct #reader_name {
                #reader_fields
            }

            impl #reader_name {
                /// Materialize the record at a physical row index (or `None` if
                /// tombstoned) — the same shared read path as the writer storage,
                /// reading through the shared-fd reader columns (#56 Direction B).
                pub fn read_at(&self, row_index: usize) -> Option<#model_name> {
                    #read_at_logic
                }

                #snapshot_accessors

                // Snapshot-only index probes (#103): `find_by_*_at` / `get_by_*_at`
                // over the cloned index maps, resolved through this reader's
                // `get_at`.  No live `find_by_*` — a reader has no live `get`.
                #reader_index_probes
            }

            // Field-constraint validator (#91), called by insert/update.
            #field_validation
        };

        Ok(tokens)
    }

    /// Generate the typed change-feed event structs for a model: `<Model>Inserted`
    /// (#62 Direction A) plus `<Model>Updated` / `<Model>Deleted` (#66 mutation
    /// surface).  Each is `{ <model_snake>: <Model> }` — the WS handler materializes
    /// the record from the broadcast row index and serializes the matching struct to
    /// JSON.  All are `Deserialize` too so subscribers/tests can round-trip them.
    /// A `Deleted` event carries the record as it was *before* the delete (the
    /// emitter passes the pre-delete row index, which is still materializable).
    fn generate_change_event_structs(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let field_name = format_ident!("{}", Self::to_snake_case(&model.name));

        let variants = [
            ("Inserted", format!("Change-feed event: a `{}` was inserted (#62).", model.name)),
            ("Updated", format!("Change-feed event: a `{}` was updated (#66); carries the new version.", model.name)),
            ("Deleted", format!("Change-feed event: a `{}` was deleted (#66); carries the record as it was before deletion.", model.name)),
        ];

        let structs = variants.iter().map(|(suffix, doc)| {
            let event_name = format_ident!("{}{}", model.name, suffix);
            quote! {
                #[doc = #doc]
                #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
                pub struct #event_name {
                    pub #field_name: #model_name,
                }
            }
        });

        quote! { #(#structs)* }
    }

    /// Generate the typed live-query delta enum for a model (#62 Direction B).
    ///
    /// The `/live-query/<model>` WS handler tracks the result set of a generated
    /// closed-set query and pushes these removal-aware deltas as membership
    /// changes: `Init` (the full initial set), `Added` / `Updated` (a typed
    /// record entered or changed within the set), `Removed` (an id left the set —
    /// now expressible because the mutation surface (#66) makes `all()` exclude
    /// retracted rows).  Every payload is a *generated* typed record or the
    /// model's own id type — never an arbitrary runtime value.  Tagged JSON
    /// (`{"kind":"added","row":{…}}`) so a client can dispatch on `kind`.
    fn generate_live_delta_enum(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let delta_name = format_ident!("{}LiveDelta", model.name);
        let id_type = Self::id_type_tokens(model);
        let doc = format!(
            "Live-query result-set delta for `{}` (#62 Direction B): removal-aware \
             membership changes over a generated closed-set query.",
            model.name
        );
        quote! {
            #[doc = #doc]
            #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
            #[serde(tag = "kind", rename_all = "lowercase")]
            pub enum #delta_name {
                /// The full matching set at subscription time.
                Init { rows: Vec<#model_name> },
                /// A record newly entered the matching set.
                Added { row: #model_name },
                /// A record already in the set changed.
                Updated { row: #model_name },
                /// An id left the matching set (deleted, or no longer matches).
                Removed { id: #id_type },
            }
        }
    }

    /// Generate the snapshot-scoped read accessors (`get_at` / `all_at`) for a model
    /// (#56 watermark reads, made mutation-aware by #66).
    ///
    /// - **Id-bearing models** (`id_read` = `Some`) resolve **newest-version-within-
    ///   the-watermark** per id.  Under superseding-version append an id can have
    ///   several physical rows; a raw prefix scan would return duplicate versions
    ///   (`all_at`) or the wrong version relative to the snapshot (`get_at`).  Reading
    ///   the id at each row and keeping the highest row `< watermark` gives each
    ///   snapshot the row state as-of capture — so a snapshot taken *before* a later
    ///   update/delete still sees the old value, the property in-place mutation could
    ///   not provide.
    /// - **Models without an id** (`id_read` = `None`) cannot be mutated, so no
    ///   superseding versions ever exist and the plain prefix scan is already correct.
    fn generate_snapshot_accessors(
        id_type: &TokenStream,
        model_name: &proc_macro2::Ident,
        id_read: Option<&TokenStream>,
    ) -> TokenStream {
        match id_read {
            Some(id_read) => quote! {
                /// Snapshot-scoped point read (#56 + #66): resolve the newest version
                /// of `id` committed as of `snap`.  A snapshot captured before a later
                /// update/delete still resolves the version live as-of capture; a
                /// tombstoned newest reads as `None` (deleted as-of the snapshot).
                pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#model_name> {
                    let watermark = snap.watermark();
                    let mut newest: Option<usize> = None;
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        if row_id == id {
                            newest = Some(row_index);
                        }
                    }
                    self.read_at(newest?)
                }

                /// Return every live record committed as of `snap` (#56 + #66).
                /// Resolves the newest version per id within `0..watermark`, so an
                /// updated row appears once (its newest version) and a deleted row is
                /// excluded — no duplicate physical versions leak into the view.
                pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<#model_name> {
                    let watermark = snap.watermark();
                    // Highest physical row per id within the watermark (ascending scan
                    // ⇒ last write wins ⇒ newest version).
                    let mut newest: HashMap<#id_type, usize> = HashMap::new();
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        newest.insert(row_id, row_index);
                    }
                    // Second pass in row order for a deterministic result; materialize a
                    // row only when it is the newest for its id (a tombstoned newest is
                    // filtered by `read_at`).
                    let mut records = Vec::new();
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        if newest.get(&row_id) == Some(&row_index) {
                            if let Some(record) = self.read_at(row_index) {
                                records.push(record);
                            }
                        }
                    }
                    records
                }
            },
            None => quote! {
                /// Snapshot-scoped point read (#56).  This model has no id field and
                /// so cannot be mutated; a row's index is stable and unique, so the
                /// live-index lookup clamped to the watermark is correct.
                pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#model_name> {
                    let row_index = *self.id_to_row.get(&id)?;
                    if !snap.visible(row_index) {
                        return None;
                    }
                    self.read_at(row_index)
                }

                /// Return every live record committed as of `snap` (#56).  With no
                /// mutation there are no superseding versions, so the plain prefix
                /// scan `0..watermark` is already a consistent point-in-time view.
                pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<#model_name> {
                    let mut records = Vec::new();
                    for row_index in 0..snap.watermark() {
                        if let Some(record) = self.read_at(row_index) {
                            records.push(record);
                        }
                    }
                    records
                }
            },
        }
    }

    /// Whether a model's primary key is a `Uuid` (vs an integer PK).
    ///
    /// Relation traversal is generated only between UUID-keyed models: FK scalar
    /// fields are always emitted as `Uuid` (see `map_field_type_ident`), so a
    /// forward getter `target.get(record.fk)` or a reverse filter `r.fk == id`
    /// only type-checks when the referenced key is also `Uuid`.  Integer-PK
    /// models are skipped (with a comment) rather than emitting code that won't
    /// compile.
    pub(crate) fn is_uuid_pk(model: &forgedb_parser::Model) -> bool {
        match model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
        {
            Some(f) => matches!(f.field_type, forgedb_parser::FieldType::Uuid),
            None => true,
        }
    }

    /// Many-to-many relations that can be safely generated: both endpoints must
    /// be UUID-keyed (the junction stores two `Uuid` columns).
    pub(crate) fn valid_m2m(schema: &Schema) -> Vec<forgedb_parser::ast::ManyToManyRelation> {
        schema
            .detect_many_to_many_relations()
            .into_iter()
            .filter(|m| {
                schema.find_model(&m.model1).is_some_and(Self::is_uuid_pk)
                    && schema.find_model(&m.model2).is_some_and(Self::is_uuid_pk)
            })
            .collect()
    }

    /// Junction struct identifier for an M2M pair, e.g. `AuthorBookLink`.
    fn junction_struct_ident(m: &forgedb_parser::ast::ManyToManyRelation) -> proc_macro2::Ident {
        format_ident!("{}{}Link", m.model1, m.model2)
    }

    /// Database field / storage-path name for an M2M junction, e.g.
    /// `author_book_link`.
    pub(crate) fn junction_field_ident(
        m: &forgedb_parser::ast::ManyToManyRelation,
    ) -> proc_macro2::Ident {
        format_ident!(
            "{}_{}_link",
            Self::to_snake_case(&m.model1),
            Self::to_snake_case(&m.model2)
        )
    }

    /// The Rust type used as a model's primary-key / identity type.
    ///
    /// Derived from the `id` field (or the first auto-generate field). UUID PKs
    /// yield `Uuid`; integer PKs (`id: +u64` / `+u32`) yield the matching integer
    /// type.  Keeping this in one place makes the `id_to_row` map key, the
    /// `insert` return type, and the `get` parameter type all agree with
    /// `record.id` — otherwise an integer PK fails with `expected Uuid, found u64`.
    fn id_type_tokens(model: &forgedb_parser::Model) -> TokenStream {
        match model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
        {
            Some(f) => Self::map_field_type_ident(&f.field_type),
            None => quote! { Uuid },
        }
    }

    /// Generate a fixed-size embedded struct definition.
    ///
    /// Structs are referenced by models via `StructType` / `OptionalStructType`
    /// and persisted as raw fixed-size bytes, so they are `#[repr(C)]`.  The
    /// parser guarantees struct fields are all fixed-size, so the same rendering
    /// rules as model fields apply: `Timestamp` needs a `#[schema(value_type)]`
    /// annotation for utoipa, and nullable fields get an `Option<>` wrapper.
    fn generate_struct(struct_def: &forgedb_parser::Struct) -> TokenStream {
        let struct_name = format_ident!("{}", struct_def.name);
        let struct_doc = format!("{} embedded struct", struct_def.name);

        let fields: Vec<_> = struct_def
            .fields
            .iter()
            .map(|field| {
                let field_name = format_ident!("{}", field.name);
                let field_type = Self::map_field_type_ident(&field.field_type);

                let schema_attr = if Self::is_timestamp_type(&field.field_type) {
                    if field.is_nullable() {
                        quote! { #[schema(value_type = Option<i64>)] }
                    } else {
                        quote! { #[schema(value_type = i64)] }
                    }
                } else {
                    quote! {}
                };

                if field.is_nullable() {
                    quote! { #schema_attr pub #field_name: Option<#field_type> }
                } else {
                    quote! { #schema_attr pub #field_name: #field_type }
                }
            })
            .collect();

        quote! {
            #[doc = #struct_doc]
            #[repr(C)]
            // `PartialEq` (not `Eq` — a struct may hold `f64`) so a struct-typed
            // field participates in the live-query typed change detector (#84).
            #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
            pub struct #struct_name {
                #(#fields),*
            }
        }
    }

    /// Generate a user-declared enum (#enum): a fieldless Rust enum plus the
    /// private discriminant codec used by the storage path.
    ///
    /// * The derives match the surrounding generated types and add what an enum
    ///   used as an index/sort key needs — `Copy`+`Eq`+`Hash`+`Ord` — while the
    ///   default serde derive serializes each variant as its NAME string (so
    ///   REST / TS / JSON all agree on `"Active"`).
    /// * Variants map to `0..N` in declaration order.  `__to_u8`/`__from_u8`
    ///   localize that mapping to one place; the generated 1-byte fixed column
    ///   stores `__to_u8()` and reads back through `__from_u8()` (which hard-fails
    ///   on an out-of-range byte — a corrupt column / schema skew).
    /// * A hard codegen error if the enum has more than 256 variants — the
    ///   discriminant must fit in one byte (u8 = `0..=255`).
    fn generate_enum(enum_def: &forgedb_parser::EnumDef) -> Result<TokenStream> {
        if enum_def.variants.len() > 256 {
            return Err(CodegenError::InvalidSchema(format!(
                "enum '{}' has {} variants; a maximum of 256 is supported (the stored \
                 discriminant is a single byte)",
                enum_def.name,
                enum_def.variants.len()
            )));
        }

        let enum_name = format_ident!("{}", enum_def.name);
        let enum_doc = format!("{} enum (#enum)", enum_def.name);

        let variant_idents: Vec<_> = enum_def
            .variants
            .iter()
            .map(|v| format_ident!("{}", v))
            .collect();

        // `__to_u8`: variant -> declaration-order discriminant.
        let to_arms: Vec<_> = variant_idents
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let disc = i as u8;
                quote! { #enum_name::#v => #disc }
            })
            .collect();

        // `__from_u8`: discriminant -> variant (panics on an out-of-range byte).
        let from_arms: Vec<_> = variant_idents
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let disc = i as u8;
                quote! { #disc => #enum_name::#v }
            })
            .collect();

        let name_str = &enum_def.name;

        Ok(quote! {
            #[doc = #enum_doc]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
            pub enum #enum_name {
                #(#variant_idents),*
            }

            impl #enum_name {
                /// This variant's 1-byte stored discriminant (declaration order).
                fn __to_u8(&self) -> u8 {
                    match self {
                        #(#to_arms),*
                    }
                }

                /// Decode a stored discriminant back to a variant.  Panics on an
                /// out-of-range byte (a corrupt column or server/replica schema skew).
                fn __from_u8(__b: u8) -> #enum_name {
                    match __b {
                        #(#from_arms,)*
                        other => panic!(
                            "invalid {} discriminant byte {} (out of range)",
                            #name_str, other
                        ),
                    }
                }
            }
        })
    }

    // ---- Secondary indexes (#90) --------------------------------------------
    //
    // An index is in-memory derived state, exactly like `id_to_row`: a
    // `HashMap<String, HashSet<Id>>` mapping a field's canonical string key to
    // the set of logical ids whose *current* (newest-version) value equals it.
    // It is maintained after the #89 WAL commit boundary on insert/update/delete
    // and rebuilt from the committed rows on reopen (folded into the same scan
    // that rebuilds `id_to_row`).  Probes resolve candidate ids back through the
    // version-aware read path (`get` / `get_at`), so they never bypass the
    // superseding-version (#66) / watermark (#56) semantics the rest of the read
    // path enforces — the index only narrows the candidate set, it does not
    // short-circuit version resolution.

    /// The scalar fields of a model that carry a secondary index.  Three sources:
    ///   * **explicit** `^index` / `&unique` scalar fields (#90) — now including
    ///     `Nullable(_)` scalars (#102), keyed null-distinctly by `index_key_expr`;
    ///   * **foreign-key** scalars `*Target` (`RequiredReference` → `Uuid`) and
    ///     `?Target` (`OptionalReference` → `Option<Uuid>`) (#100) — always indexed
    ///     (a reverse one-to-many getter that would otherwise scan always exists),
    ///     so the index intent is the relation itself, no `^`/`&` needed.
    ///
    /// Excludes the primary id (already covered by `id_to_row`).  Empty when the
    /// model has no id/auto field (it cannot be inserted, so nothing to index).
    fn indexed_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        if !model.fields.iter().any(|f| f.name == "id" || f.auto_generate) {
            return Vec::new();
        }
        model
            .fields
            .iter()
            .filter(|f| {
                let is_fk = matches!(
                    f.field_type,
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(_)
                            | forgedb_parser::RelationType::OptionalReference(_),
                    )
                );
                let is_explicit = (f.indexed || f.unique)
                    && f.name != "id"
                    && !f.auto_generate
                    && Self::is_filterable_scalar(&f.field_type);
                is_explicit || is_fk
            })
            .collect()
    }

    /// Whether a field type is a non-relation scalar an index / REST filter can
    /// key on.  Mirrors `ApiGenerator::is_filterable_field` (kept in sync).
    fn is_filterable_scalar(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64
            | FieldType::Bool
            | FieldType::Uuid
            | FieldType::Timestamp
            | FieldType::String
            // decimal is `Ord` + `Hash`, so it is filterable / sortable / indexable
            // (index keyed scale-invariantly via `index_value_expr`).
            | FieldType::Decimal
            // enum derives `Ord` + `Hash`, so it is filterable / sortable /
            // indexable (index key = the variant name string, via `index_key_expr`).
            | FieldType::Enum(_)
            | FieldType::Char(_) => true,
            FieldType::Nullable(inner) => Self::is_filterable_scalar(inner),
            _ => false,
        }
    }

    /// Whether a field type is (or wraps) a numeric scalar `@min`/`@max` can bound.
    fn is_numeric_type(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64 => true,
            FieldType::Nullable(inner) => Self::is_numeric_type(inner),
            _ => false,
        }
    }

    /// The `i64` params of a constraint, in order (skips string params).
    fn constraint_numbers(c: &forgedb_parser::ast::Constraint) -> Vec<i64> {
        c.params
            .iter()
            .filter_map(|p| match p {
                forgedb_parser::ast::ConstraintParam::Number(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    /// The first string param of a constraint (`@pattern("...")`/`@default("...")`).
    fn constraint_first_string(c: &forgedb_parser::ast::Constraint) -> Option<&str> {
        c.params.iter().find_map(|p| match p {
            forgedb_parser::ast::ConstraintParam::String(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Generate the per-model field-constraint validator (#91 Phase 3):
    /// `validate_<snake>(record) -> Result<(), ValidationError>`, enforcing the
    /// declared `@min`/`@max` (numeric), `@length`/`@email`/`@url`/`@pattern`/`@regex`
    /// (string) directives.  Every check is generated from a specific field +
    /// directive — there is no runtime constraint interpreter.  Nullable fields are
    /// validated only when `Some` (a `None` value violates no value-constraint).
    ///
    /// `@pattern`/`@regex` (#104) are aliases: the value must match a schema-declared
    /// regex, compiled once into a per-(model,field) `LazyLock<regex::Regex>` static.
    fn generate_field_validation(model: &forgedb_parser::Model) -> TokenStream {
        let fn_name = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let model_name = format_ident!("{}", model.name);

        // `LazyLock<Regex>` statics for each `@pattern`/`@regex` field (#104),
        // compiled once and shared across every validate call.
        let pattern_statics = std::cell::RefCell::new(Vec::<TokenStream>::new());

        let field_blocks: Vec<_> = model
            .fields
            .iter()
            .filter_map(|field| {
                let is_string = Self::is_string_type(&field.field_type);
                let is_numeric = Self::is_numeric_type(&field.field_type);
                let fname_str = field.name.as_str();

                let mut checks: Vec<TokenStream> = Vec::new();
                for c in &field.constraints {
                    match c.name.as_str() {
                        "min" if is_numeric => {
                            if let Some(&n) = Self::constraint_numbers(c).first() {
                                let msg = format!("must be >= {n}");
                                checks.push(quote! {
                                    if (*__v as f64) < #n as f64 {
                                        return Err(ValidationError::Constraint {
                                            field: #fname_str, rule: "min",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        "max" if is_numeric => {
                            if let Some(&n) = Self::constraint_numbers(c).first() {
                                let msg = format!("must be <= {n}");
                                checks.push(quote! {
                                    if (*__v as f64) > #n as f64 {
                                        return Err(ValidationError::Constraint {
                                            field: #fname_str, rule: "max",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        "length" if is_string => {
                            let nums = Self::constraint_numbers(c);
                            if nums.len() == 1 {
                                let max = nums[0];
                                let msg = format!("length must be <= {max}");
                                checks.push(quote! {
                                    if __v.chars().count() > #max as usize {
                                        return Err(ValidationError::Constraint {
                                            field: #fname_str, rule: "length",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            } else if nums.len() >= 2 {
                                let (min, max) = (nums[0], nums[1]);
                                let msg = format!("length must be between {min} and {max}");
                                checks.push(quote! {
                                    let __len = __v.chars().count();
                                    if __len < #min as usize || __len > #max as usize {
                                        return Err(ValidationError::Constraint {
                                            field: #fname_str, rule: "length",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        "email" if is_string => {
                            checks.push(quote! {
                                let __parts: Vec<&str> = __v.split('@').collect();
                                let __ok = __parts.len() == 2
                                    && !__parts[0].is_empty()
                                    && __parts[1].contains('.')
                                    && !__parts[1].starts_with('.')
                                    && !__parts[1].ends_with('.')
                                    && !__v.chars().any(|__c| __c.is_whitespace());
                                if !__ok {
                                    return Err(ValidationError::Constraint {
                                        field: #fname_str, rule: "email",
                                        message: "must be a valid email address".to_string(),
                                    });
                                }
                            });
                        }
                        "url" if is_string => {
                            checks.push(quote! {
                                if !(__v.starts_with("http://") || __v.starts_with("https://")) {
                                    return Err(ValidationError::Constraint {
                                        field: #fname_str, rule: "url",
                                        message: "must be an http(s) URL".to_string(),
                                    });
                                }
                            });
                        }
                        // `@pattern("...")` / `@regex("...")` are aliases (#104): the
                        // value must match the declared regex.  The pattern is authored
                        // at schema-write time, so it is compiled ONCE into a per-(model,
                        // field) `LazyLock<Regex>` static; the whole value is tested with
                        // `is_match` (a match anywhere unless the pattern is anchored,
                        // consistent with common regex-constraint semantics).
                        "pattern" | "regex" if is_string => {
                            if let Some(pat) = Self::constraint_first_string(c) {
                                let rname = format!(
                                    "__PAT_{}_{}",
                                    Self::to_snake_case(&model.name).to_uppercase(),
                                    field.name.to_uppercase()
                                );
                                let rident = format_ident!("{}", rname);
                                let msg = format!("must match pattern {pat}");
                                pattern_statics.borrow_mut().push(quote! {
                                    static #rident: std::sync::LazyLock<regex::Regex> =
                                        std::sync::LazyLock::new(|| {
                                            regex::Regex::new(#pat)
                                                .expect("schema-declared @pattern/@regex is a valid regex")
                                        });
                                });
                                checks.push(quote! {
                                    if !#rident.is_match(__v.as_str()) {
                                        return Err(ValidationError::Constraint {
                                            field: #fname_str, rule: "pattern",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                }

                if checks.is_empty() {
                    return None;
                }
                let fident = format_ident!("{}", field.name);
                // `__v` is always `&T` (numeric derefs via `*__v`; string methods
                // take `&self`).  Nullable fields validate only their `Some` value.
                if field.is_nullable() {
                    Some(quote! {
                        if let Some(__v) = &record.#fident {
                            #(#checks)*
                        }
                    })
                } else {
                    Some(quote! {
                        {
                            let __v = &record.#fident;
                            #(#checks)*
                        }
                    })
                }
            })
            .collect();

        let pattern_statics = pattern_statics.into_inner();

        quote! {
            /// Field-constraint validation (#91): reject a record whose values
            /// violate a declared `@min`/`@max`/`@length`/`@email`/`@url`/`@pattern`
            /// directive.
            fn #fn_name(record: &#model_name) -> Result<(), ValidationError> {
                #(#pattern_statics)*
                #(#field_blocks)*
                Ok(())
            }
        }
    }

    /// Generate the `&unique` enforcement checks (#91) for insert/update, run
    /// before the commit.  Probes the Phase-2 unique index (`<field>_index`):
    /// on insert any non-empty bucket for the key is a conflict; on update a
    /// bucket entry for a *different* id is a conflict (the record's own id may
    /// already be present with an unchanged value — index maintenance runs AFTER
    /// commit, so the pre-commit index still keys the old value to `id`).
    fn generate_unique_checks(model: &forgedb_parser::Model, exclude_self: bool) -> Vec<TokenStream> {
        Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let key = Self::index_key_expr(Self::index_value_expr(
                    &f.field_type,
                    quote! { record.#fident },
                ));
                if exclude_self {
                    quote! {
                        {
                            let __uk: String = { #key };
                            if let Some(__ids) = self.#ident.get(&__uk) {
                                if __ids.iter().any(|__i| *__i != id) {
                                    return Err(ValidationError::Unique { field: #fname });
                                }
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let __uk: String = { #key };
                            if self.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                                return Err(ValidationError::Unique { field: #fname });
                            }
                        }
                    }
                }
            })
            .collect()
    }

    /// The in-memory index field name for an indexed field, e.g. `email_index`.
    fn index_field_ident(field: &forgedb_parser::Field) -> proc_macro2::Ident {
        format_ident!("{}_index", field.name)
    }

    /// The parameter type a `find_by_<field>` / `get_by_<field>` probe accepts:
    ///   * `string`               → `&str` (ergonomic)
    ///   * `string?`              → `Option<&str>` (#102 — `None` probes the unset bucket)
    ///   * `T?` (other scalar)    → `Option<T>` (#102)
    ///   * `*Target` FK           → `Uuid` (#100 — the FK is a plain `Uuid`)
    ///   * `?Target` FK           → `Option<Uuid>` (#100 — `None` probes unlinked rows)
    ///   * other scalar           → the (Copy) mapped type by value
    ///
    /// The arg-side key (`index_key_expr(value)`) and record-side key
    /// (`index_key_expr(record.field)`) must serialize identically; the `Option<_>`
    /// param types above are exactly the record field types, so both agree.
    fn index_param_type(field: &forgedb_parser::Field) -> TokenStream {
        use forgedb_parser::{FieldType, RelationType};
        match &field.field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => quote! { Uuid },
            FieldType::Relation(RelationType::OptionalReference(_)) => quote! { Option<Uuid> },
            FieldType::Nullable(inner) if Self::is_string_type(inner) => quote! { Option<&str> },
            FieldType::Nullable(inner) => {
                let ty = Self::map_field_type_ident(inner);
                quote! { Option<#ty> }
            }
            _ if Self::is_string_type(&field.field_type) => quote! { &str },
            _ => {
                let ty = Self::map_field_type_ident(&field.field_type);
                quote! { #ty }
            }
        }
    }

    /// Canonical, **null-distinct** string key for a value expression.  A leading
    /// tag byte encodes the JSON type class so values that stringify alike can
    /// never collide across classes — critically `None`/`Value::Null` (`\u{0}`)
    /// vs the literal string `"null"` (`\u{1}null`), the collision that made #90
    /// gate nullable/optional-FK fields out (#102).  The tags:
    ///   `\u{0}`        → JSON null (a `None` / unset optional / unlinked FK)
    ///   `\u{1}` + raw  → JSON string (raw contents, incl. uuid hyphenated form)
    ///   `\u{2}` + text → any other scalar (number / bool) via JSON `to_string`
    ///   `\u{3}`        → serialization error (unreachable for scalar fields)
    /// The key is internal to the index probes — it is NOT the value the REST
    /// filter (`<model>_event_matches`) compares by — so tagging is free of any
    /// cross-path constraint.  Computing it identically for the stored field value
    /// and the probe argument is what keeps an index hit self-consistent.
    /// Pre-transform a field value expression before it is fed to `index_key_expr`,
    /// so the resulting index key is **scale-invariant** for `decimal` fields.
    ///
    /// `rust_decimal::Decimal` preserves scale (`1.0` != `1.00` in `to_string()` /
    /// serde form) even though they are `Ord`-equal; feeding them raw into
    /// `index_key_expr` would bucket them separately (the same key-collision class
    /// #102 fixed for nullable).  `Decimal::normalize()` collapses trailing-zero
    /// scale, so normalizing both the stored value and the probe argument keys them
    /// identically.  Nullable decimal normalizes through `.map(...)`; every other
    /// field type is returned unchanged.
    fn index_value_expr(field_type: &forgedb_parser::FieldType, value_expr: TokenStream) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::Decimal => quote! { (#value_expr).normalize() },
            forgedb_parser::FieldType::Nullable(inner) if Self::is_decimal_type(inner) => {
                quote! { (#value_expr).map(|__d| __d.normalize()) }
            }
            _ => value_expr,
        }
    }

    fn index_key_expr(value_expr: TokenStream) -> TokenStream {
        quote! {
            match serde_json::to_value(&(#value_expr)) {
                Ok(serde_json::Value::Null) => String::from('\u{0}'),
                Ok(serde_json::Value::String(__s)) => {
                    let mut __k = String::from('\u{1}');
                    __k.push_str(&__s);
                    __k
                }
                Ok(__other) => {
                    let mut __k = String::from('\u{2}');
                    __k.push_str(&__other.to_string());
                    __k
                }
                Err(_) => String::from('\u{3}'),
            }
        }
    }

    /// Emit an "add `id` under the key of `value_expr`" statement for one index.
    fn index_add_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        value_expr: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        let key = Self::index_key_expr(value_expr);
        quote! {
            {
                let __k: String = { #key };
                #receiver.#index_ident.entry(__k).or_default().insert(#id_expr);
            }
        }
    }

    /// Emit a "remove `id` from the key of `value_expr`" statement for one index,
    /// pruning the bucket when it empties (structured to avoid a double mutable
    /// borrow of the map).
    fn index_remove_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        value_expr: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        let key = Self::index_key_expr(value_expr);
        quote! {
            {
                let __k: String = { #key };
                let mut __empty = false;
                if let Some(__set) = #receiver.#index_ident.get_mut(&__k) {
                    __set.remove(&(#id_expr));
                    __empty = __set.is_empty();
                }
                if __empty {
                    #receiver.#index_ident.remove(&__k);
                }
            }
        }
    }

    /// The composite `@index(a, b, …)` indexes of a model (#101), resolved to their
    /// component `&Field`s.  A composite is emitted only when the model has an
    /// identity field, has ≥2 components, and **every** component resolves to an
    /// indexable scalar or FK field (same eligibility as a single-field index);
    /// an unresolvable or non-scalar component skips the whole composite (the
    /// schema validator is the place to hard-error — here we skip defensively).
    fn composite_indexes(
        model: &forgedb_parser::Model,
    ) -> Vec<(proc_macro2::Ident, Vec<&forgedb_parser::Field>)> {
        if !model.fields.iter().any(|f| f.name == "id" || f.auto_generate) {
            return Vec::new();
        }
        model
            .composite_indexes
            .iter()
            .filter_map(|ci| {
                if ci.fields.len() < 2 {
                    return None;
                }
                let comps: Option<Vec<&forgedb_parser::Field>> = ci
                    .fields
                    .iter()
                    .map(|name| {
                        model
                            .fields
                            .iter()
                            .find(|f| &f.name == name)
                            .filter(|f| Self::is_composite_component(&f.field_type))
                    })
                    .collect();
                let comps = comps?;
                let ident = format_ident!("{}_index", ci.fields.join("_"));
                Some((ident, comps))
            })
            .collect()
    }

    /// Whether a field type may be a component of a composite index: the same set
    /// a single-field index accepts — a filterable scalar (incl. `Nullable`, #102)
    /// or an FK reference (`*Target` / `?Target`, #100).
    fn is_composite_component(field_type: &forgedb_parser::FieldType) -> bool {
        Self::is_filterable_scalar(field_type)
            || matches!(
                field_type,
                forgedb_parser::FieldType::Relation(
                    forgedb_parser::RelationType::RequiredReference(_)
                        | forgedb_parser::RelationType::OptionalReference(_),
                )
            )
    }

    /// The base name of a composite index probe, e.g. `find_by_status_and_created_at`.
    fn composite_probe_ident(components: &[&forgedb_parser::Field]) -> proc_macro2::Ident {
        let joined = components
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join("_and_");
        format_ident!("find_by_{}", joined)
    }

    /// Build a collision-free composite key from N per-component key expressions.
    /// Each component's null-distinct `index_key_expr` string is length-prefixed
    /// (`<byte-len>:<key>`), so `("ab","c")` and `("a","bc")` can never collide.
    fn composite_key_build(part_key_exprs: &[TokenStream]) -> TokenStream {
        let pushes = part_key_exprs.iter().map(|k| {
            quote! {
                {
                    let __p: String = { #k };
                    __ck.push_str(&__p.len().to_string());
                    __ck.push(':');
                    __ck.push_str(&__p);
                }
            }
        });
        quote! {
            {
                let mut __ck = String::new();
                #(#pushes)*
                __ck
            }
        }
    }

    /// Emit an "add `id` under the composite key of the component value exprs".
    fn composite_add_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        part_value_exprs: &[TokenStream],
        id_expr: &TokenStream,
    ) -> TokenStream {
        let part_keys: Vec<_> = part_value_exprs
            .iter()
            .map(|v| Self::index_key_expr(v.clone()))
            .collect();
        let key = Self::composite_key_build(&part_keys);
        quote! {
            {
                let __k: String = #key;
                #receiver.#index_ident.entry(__k).or_default().insert(#id_expr);
            }
        }
    }

    /// Emit a "remove `id` from the composite key" statement, pruning empty buckets.
    fn composite_remove_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        part_value_exprs: &[TokenStream],
        id_expr: &TokenStream,
    ) -> TokenStream {
        let part_keys: Vec<_> = part_value_exprs
            .iter()
            .map(|v| Self::index_key_expr(v.clone()))
            .collect();
        let key = Self::composite_key_build(&part_keys);
        quote! {
            {
                let __k: String = #key;
                let mut __empty = false;
                if let Some(__set) = #receiver.#index_ident.get_mut(&__k) {
                    __set.remove(&(#id_expr));
                    __empty = __set.is_empty();
                }
                if __empty {
                    #receiver.#index_ident.remove(&__k);
                }
            }
        }
    }

    /// Generate storage field declarations for columnar storage
    fn generate_storage_fields(model: &forgedb_parser::Model) -> TokenStream {
        let mut column_fields = Vec::new();

        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_fixed_size_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: FixedColumn
                });
            } else if Self::is_variable_column_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: VariableColumn
                });
            }
            // Skip relations and other non-storage types
        }

        let id_type = Self::id_type_tokens(model);

        // Secondary index fields (#90): one `value-key -> {id}` map per `^index`
        // / `&unique` scalar field, maintained alongside `id_to_row`.
        let index_fields: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! {
                    #ident: std::collections::HashMap<String, std::collections::HashSet<#id_type>>
                }
            })
            .collect();

        // Composite `@index(a, b)` fields (#101): same `key -> {id}` shape, keyed
        // by the collision-free length-prefixed composite key.
        let composite_fields: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! {
                    #ident: std::collections::HashMap<String, std::collections::HashSet<#id_type>>
                }
            })
            .collect();

        quote! {
            id_to_row: HashMap<#id_type, usize>,
            row_count: usize,
            #(#column_fields,)*
            #(#index_fields,)*
            #(#composite_fields,)*
            tombstones: forgedb_storage::Tombstones,
            // Write-ahead log (#89 durable write path): every mutation records an
            // opaque row blob here + fsync BEFORE touching columns, so a crash
            // mid-append is recovered on reopen.  The WAL stores bytes only — it
            // never decodes a field (identity: schema-agnostic substrate).
            wal: forgedb_wal::WalManager,
            // Mutations recorded to the WAL since the last checkpoint (#89 step
            // 2).  When it reaches `WAL_CHECKPOINT_INTERVAL`, `checkpoint()` runs:
            // columns are fsync'd and the WAL is truncated, bounding it.
            writes_since_checkpoint: u64,
            // Data root this storage was opened at (#92 Phase 4).  Remembered so
            // in-process compaction can rewrite this model's files under it and
            // then reopen the column handles from the compacted files.
            root: std::path::PathBuf,
            // Dead (superseded/tombstoned) row versions accumulated since the last
            // compaction (#92 Phase 4).  When it reaches `COMPACTION_DEAD_THRESHOLD`,
            // `compact()` reclaims them under the writer lock.  Incremented by
            // update/delete only — an insert creates no dead version.
            dead_since_compaction: u64,
            // Transaction critical-section guard (MVCC Tier 1, #83).  Set while a
            // `TxHandle` stages into this collection.  Auto-checkpoint (#96) and
            // auto-compaction (#92) MUST NOT fire mid-transaction — a checkpoint
            // would truncate the per-model WAL underneath the rollback, and a
            // compaction would renumber rows underneath the rollback marks.  While
            // set, the auto-triggers only RECORD that they are due (`*_deferred`
            // below); `TxHandle::commit`/rollback runs any pending trigger once
            // after the critical section closes.
            in_transaction: bool,
            // A checkpoint became due while `in_transaction`; deferred (#96).
            checkpoint_deferred: bool,
            // A compaction became due while `in_transaction`; deferred (#92) so it
            // never renumbers rows a live transaction's marks refer to.
            compact_deferred: bool,
            // Change-feed handle (#62 Direction A): `None` for standalone use;
            // `Database::new()` attaches a shared feed so `insert` can emit.
            changefeed: Option<forgedb_changefeed::ChangeFeed>,
            // Durable replication broker (#82 Direction C): `None` for standalone
            // use; `Database::open_at()` attaches a shared, offset-addressed broker
            // so each mutation is durably recorded for a resumable follower (#110).
            // Shared across every collection behind an `Arc<Mutex<..>>` so one
            // global offset sequences changes across models.
            broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
        }
    }

    /// Generate storage initialization code
    fn generate_storage_inits(model: &forgedb_parser::Model, _schema: &Schema) -> TokenStream {
        let mut inits = Vec::new();
        let mut column_index = 0usize;
        // Namespace all storage paths under the model name to prevent collision
        let model_snake = Self::to_snake_case(&model.name);

        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_fixed_size_type(&field.field_type) {
                let col_path = format!(
                    "{}/fixed/{}_{}.bin",
                    model_snake,
                    Self::type_name(&field.field_type),
                    column_index
                );
                // C3: emit size_of in generated code so column size matches the Rust type
                let value_size_expr =
                    Self::column_value_size_expr(&field.field_type);

                inits.push(quote! {
                    #field_col_name: FixedColumn::new(
                        root.join(#col_path),
                        #value_size_expr
                    ).expect("Failed to create fixed column")
                });
                column_index += 1;
            } else if Self::is_variable_column_type(&field.field_type) {
                let data_path = format!("{}/variable/string_data_{}.bin", model_snake, column_index);
                let offsets_path = format!("{}/variable/string_offsets_{}.bin", model_snake, column_index);

                inits.push(quote! {
                    #field_col_name: VariableColumn::new(
                        root.join(#data_path),
                        root.join(#offsets_path)
                    ).expect("Failed to create variable column")
                });
                column_index += 1;
            }
        }

        // Every column/tombstone path is joined under `root` — the data-dir the
        // enclosing `new_at(root)` was handed.  `new()` passes `.` (CWD), so the
        // no-arg path is byte-for-byte the old CWD-relative behavior; a per-tenant
        // process (#59) passes that tenant's dir.  `root` is a plain value threaded
        // through generated code — the substrate never learns the word "tenant".
        let tombstones_path = format!("{}/tombstones.bin", model_snake);
        let wal_path = format!("{}/wal.log", model_snake);

        // Empty secondary indexes (#90); `new_at` rebuilds them from committed
        // rows via `#rehydrate_logic` right after recovery.
        let index_inits: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { #ident: std::collections::HashMap::new() }
            })
            .collect();

        // Composite index maps (#101), also rebuilt by `#rehydrate_logic`.
        let composite_inits: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! { #ident: std::collections::HashMap::new() }
            })
            .collect();

        quote! {
            id_to_row: HashMap::new(),
            row_count: 0,
            #(#inits,)*
            #(#index_inits,)*
            #(#composite_inits,)*
            tombstones: forgedb_storage::Tombstones::new(
                root.join(#tombstones_path)
            ).expect("Failed to create tombstones"),
            // One WAL per model, namespaced under the same data root (#89).
            // `FsyncPolicy::Always` makes each commit record durable before the
            // columns are appended — the crash-durability boundary.
            wal: forgedb_wal::WalManager::open(
                root.join(#wal_path),
                forgedb_wal::FsyncPolicy::Always,
            ).expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            // Remember the data root so `compact()` can rewrite + reopen files (#92).
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            // Not in a transaction on open; the txn guard sets these (MVCC Tier 1).
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            changefeed: None,
            broker: None,
        }
    }

    /// Generate the read-only reader column field declarations (#56 Direction B).
    ///
    /// Mirrors `generate_storage_fields` field-for-field, but every column is the
    /// read-only `*Reader` variant sharing the writer's file descriptor.  No
    /// `row_count` / `changefeed` (a reader never appends and never emits); it
    /// keeps `id_to_row` only because a no-id model's `get_at` consults it (an
    /// id-bearing model's snapshot reads scan the id column instead).
    fn generate_reader_storage_fields(model: &forgedb_parser::Model) -> TokenStream {
        let mut column_fields = Vec::new();
        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);
            if Self::is_fixed_size_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: forgedb_storage::FixedColumnReader
                });
            } else if Self::is_variable_column_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: forgedb_storage::VariableColumnReader
                });
            }
        }
        let id_type = Self::id_type_tokens(model);
        // Index maps (#103): the reader carries a point-in-time CLONE of every
        // secondary + composite index, captured on the writer at `reader()` time
        // (same instant its column readers open), so its `_at` probes are O(1)
        // like the writer's.  Same field names/types as the writer storage.
        let index_fields: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! {
                    #ident: std::collections::HashMap<String, std::collections::HashSet<#id_type>>
                }
            })
            .collect();
        let composite_fields: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! {
                    #ident: std::collections::HashMap<String, std::collections::HashSet<#id_type>>
                }
            })
            .collect();
        quote! {
            id_to_row: HashMap<#id_type, usize>,
            #(#column_fields,)*
            #(#index_fields,)*
            #(#composite_fields,)*
            tombstones: forgedb_storage::TombstonesReader,
        }
    }

    /// Build a `*StorageReader` literal from `&self` writer storage (#56
    /// Direction B): each column shares the writer's fd via `col.reader()`;
    /// `id_to_row` is cloned so a no-id model's `get_at` still resolves.
    fn generate_reader_inits(model: &forgedb_parser::Model) -> TokenStream {
        let mut inits = Vec::new();
        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);
            if Self::is_fixed_size_type(&field.field_type)
                || Self::is_variable_column_type(&field.field_type)
            {
                inits.push(quote! {
                    #field_col_name: self.#field_col_name.reader()
                        .expect("Failed to open column reader")
                });
            }
        }
        // Clone every index map (#103) — captured on the single writer, off the
        // mutation path (same discipline as `id_to_row.clone()` and
        // `DatabaseSnapshot::snapshot()`), so the reader's view is coherent with
        // its column readers.  Plain clone (O(rows) per index), amortized per
        // reader thread; an `Arc`-swap is the escape hatch if it ever matters.
        let index_clones: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { #ident: self.#ident.clone() }
            })
            .collect();
        let composite_clones: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! { #ident: self.#ident.clone() }
            })
            .collect();
        quote! {
            id_to_row: self.id_to_row.clone(),
            #(#inits,)*
            #(#index_clones,)*
            #(#composite_clones,)*
            tombstones: self.tombstones.reader()
                .expect("Failed to open tombstones reader"),
        }
    }

    /// Emit a `size_of`-based expression for the column's value size so it matches
    /// the actual Rust type layout (C3).  For complex/struct types we defer to
    /// `std::mem::size_of::<T>()` evaluated at compile time in the generated code.
    fn column_value_size_expr(field_type: &forgedb_parser::FieldType) -> TokenStream {
        match field_type {
            // enum: a 1-byte discriminant column (`__to_u8`/`__from_u8`), or a
            // 2-byte `[present, disc]` column for nullable enum — both stored via
            // an explicit `append_bytes`/`read_bytes` path (never `read_unaligned`).
            _ if Self::is_enum_type(field_type) => {
                if matches!(field_type, forgedb_parser::FieldType::Nullable(_)) {
                    quote! { 2usize }
                } else {
                    quote! { 1usize }
                }
            }
            forgedb_parser::FieldType::StructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { std::mem::size_of::<#ident>() }
            }
            forgedb_parser::FieldType::OptionalStructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { std::mem::size_of::<Option<#ident>>() }
            }
            forgedb_parser::FieldType::Nullable(inner) => {
                let inner_tokens = Self::map_field_type_ident(inner);
                quote! { std::mem::size_of::<Option<#inner_tokens>>() }
            }
            // RequiredReference behaves exactly like Uuid (16-byte fixed column)
            forgedb_parser::FieldType::Relation(
                forgedb_parser::RelationType::RequiredReference(_),
            ) => {
                quote! { 16usize }
            }
            // OptionalReference behaves exactly like Nullable(Uuid) — Option<Uuid> bytes
            forgedb_parser::FieldType::Relation(
                forgedb_parser::RelationType::OptionalReference(_),
            ) => {
                quote! { std::mem::size_of::<Option<Uuid>>() }
            }
            _ => {
                // For all other fixed-size types the constant is authoritative
                let size = match field_type {
                    forgedb_parser::FieldType::U32 | forgedb_parser::FieldType::I32 => 4usize,
                    forgedb_parser::FieldType::U64 | forgedb_parser::FieldType::I64 => 8,
                    forgedb_parser::FieldType::F64 => 8,
                    forgedb_parser::FieldType::Bool => 1,
                    // enum = 1-byte u8 discriminant.
                    forgedb_parser::FieldType::Enum(_) => 1,
                    forgedb_parser::FieldType::Uuid => 16,
                    // decimal = rust_decimal::Decimal, a fixed 16-byte value
                    // (Decimal::serialize() -> [u8; 16]).
                    forgedb_parser::FieldType::Decimal => 16,
                    forgedb_parser::FieldType::Timestamp => 8,
                    forgedb_parser::FieldType::Char(n) => *n,
                    forgedb_parser::FieldType::FixedArray(inner, count) => {
                        // Fall back to map_field_type_ident for array sizing
                        let inner_tokens = Self::map_field_type_ident(inner);
                        return quote! { std::mem::size_of::<[#inner_tokens; #count]>() };
                    }
                    _ => 8, // Default
                };
                quote! { #size }
            }
        }
    }

    /// Map a field type to the substrate `ColumnType` for the per-model layout
    /// manifest (#57).  Primitives map to their exact variant; every other
    /// fixed-size type (char(N), fixed array, struct, nullable-fixed, optional
    /// FK) is a `FixedBytes(width)` — a physical byte-width fact, never schema
    /// semantics.  Variable strings are handled by the caller (`ColumnType::String`).
    fn storage_column_type_tokens(field_type: &forgedb_parser::FieldType) -> TokenStream {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32 => quote! { forgedb_storage::ColumnType::U32 },
            FieldType::I32 => quote! { forgedb_storage::ColumnType::I32 },
            FieldType::U64 => quote! { forgedb_storage::ColumnType::U64 },
            FieldType::I64 => quote! { forgedb_storage::ColumnType::I64 },
            FieldType::F64 => quote! { forgedb_storage::ColumnType::F64 },
            FieldType::Bool => quote! { forgedb_storage::ColumnType::Bool },
            FieldType::Uuid => quote! { forgedb_storage::ColumnType::Uuid },
            FieldType::Timestamp => quote! { forgedb_storage::ColumnType::Timestamp },
            // FK scalar persists as a raw [u8; 16], same as Uuid.
            FieldType::Relation(forgedb_parser::RelationType::RequiredReference(_)) => {
                quote! { forgedb_storage::ColumnType::Uuid }
            }
            // Any remaining fixed-size type is opaque fixed bytes of its width.
            _ => {
                let size = Self::column_value_size_expr(field_type);
                quote! { forgedb_storage::ColumnType::FixedBytes(#size) }
            }
        }
    }

    /// Generate the per-model layout `manifest.json` writer (#57).  Emits a
    /// `write_manifest` method describing *physical* column layout only —
    /// index, `ColumnType`, `value_size`, fixed/variable kind, and the file
    /// path relative to the model dir — plus a `row_anchor` naming the file
    /// whose length counts committed rows (`tombstones.bin`, 1 byte/row).  It
    /// carries **no** schema semantics (no relations/directives/validation).
    /// Called from `new()`, off the insert hot path; the manifest's first
    /// reader is schema-blind backup (#57) / the inspector (#63).
    ///
    /// INCREMENTAL-BACKUP CHAIN TOKEN (#76): `compaction_epoch` is the
    /// chain-validity generation counter.  Within one epoch the columns are
    /// strictly append-only, so an incremental backup can ship just the byte
    /// tail appended since the base.  A compaction renumbers rows and so *breaks*
    /// the chain — `compact()` bumps the epoch, and an incremental against a base
    /// in a different epoch is refused.  Because `write_manifest` runs on *every*
    /// reopen, it must NOT hardcode the epoch (that would clobber a bumped epoch
    /// back to 0 on the next open and silently corrupt a chain); instead it
    /// *loads any existing manifest and preserves* its `compaction_epoch`,
    /// stamping the baseline `0` only for a fresh directory.
    fn generate_write_manifest(model: &forgedb_parser::Model) -> TokenStream {
        let model_snake = Self::to_snake_case(&model.name);
        let manifest_path = format!("{}/manifest.json", model_snake);

        let mut col_entries = Vec::new();
        let mut column_index = 0usize;
        for field in &model.fields {
            let name = &field.name;
            if Self::is_fixed_size_type(&field.field_type) {
                let rel_path = format!(
                    "fixed/{}_{}.bin",
                    Self::type_name(&field.field_type),
                    column_index
                );
                let col_type = Self::storage_column_type_tokens(&field.field_type);
                let value_size = Self::column_value_size_expr(&field.field_type);
                col_entries.push(quote! {
                    forgedb_storage::ColumnMetadata {
                        name: #name.to_string(),
                        column_type: #col_type,
                        column_index: #column_index,
                        value_size: #value_size,
                        kind: forgedb_storage::ColumnKind::Fixed,
                        relative_path: #rel_path.to_string(),
                    }
                });
                column_index += 1;
            } else if Self::is_variable_column_type(&field.field_type) {
                let rel_path = format!("variable/string_data_{}.bin", column_index);
                col_entries.push(quote! {
                    forgedb_storage::ColumnMetadata {
                        name: #name.to_string(),
                        // Physical-layout tag only (variable-length column); `json`
                        // and `string` share the same on-disk representation.
                        column_type: forgedb_storage::ColumnType::String,
                        column_index: #column_index,
                        value_size: 0usize,
                        kind: forgedb_storage::ColumnKind::Variable,
                        relative_path: #rel_path.to_string(),
                    }
                });
                column_index += 1;
            }
        }

        quote! {
            /// Write the physical-layout manifest for this model (#57).  Layout
            /// metadata only — schema-blind backup/inspector read it; it carries
            /// no `.forge` semantics.  Written at open, off the insert hot path.
            fn write_manifest(&self, root: &std::path::Path) {
                let columns = vec![ #(#col_entries),* ];
                let __manifest_abs = root.join(#manifest_path);
                // Preserve the incremental-backup chain token (#76): load any
                // existing manifest and carry its `compaction_epoch` forward, so a
                // reopen never clobbers a bumped epoch back to 0.  Baseline 0 for a
                // fresh directory (no manifest yet).
                let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
                    .map(|m| m.compaction_epoch)
                    .unwrap_or(0);
                // Preserve the on-disk format version (#74 Phase 1) exactly as the
                // `compaction_epoch` above: a reopen must NOT clobber a version a
                // migration bumped (that would silently defeat the open-time guard
                // and let stale bytes be mis-decoded).  A fresh directory is stamped
                // with this binary's `EXPECTED_FORMAT_VERSION` baseline, so a dir
                // this binary writes always matches its own expectation.
                let __format_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
                    .map(|m| m.format_version)
                    .unwrap_or(EXPECTED_FORMAT_VERSION);
                let manifest = forgedb_storage::Manifest {
                    schema_version: 1,
                    row_count: self.row_count,
                    columns,
                    wal_enabled: false,
                    // Observability only (#89 step 2): rows durable on disk at open
                    // (== row_count — recovery already realigned the columns).  NOT
                    // load-bearing for recovery, which reads the column lengths.
                    last_checkpoint: self.row_count as u64,
                    compaction_epoch: __compaction_epoch,
                    format_version: __format_version,
                    row_anchor: Some(forgedb_storage::RowAnchor {
                        relative_path: "tombstones.bin".to_string(),
                        bytes_per_row: 1usize,
                    }),
                };
                // Best-effort: a failed manifest write must not abort the app;
                // reopen/compaction anchor on the tombstone file, not this file.
                let _ = manifest.save_to(&__manifest_abs);
            }

            /// Bump this model's `compaction_epoch` (#76): compaction renumbers
            /// rows, breaking the append-only byte-tail chain incremental backups
            /// rely on, so the epoch is incremented to invalidate any incremental
            /// taken against a pre-compaction base.  Reads + rewrites the manifest
            /// (temp + rename via `save_to`); best-effort, off the hot path.
            fn bump_compaction_epoch(&self, root: &std::path::Path) {
                let __manifest_abs = root.join(#manifest_path);
                if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
                    __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
                    let _ = __m.save_to(&__manifest_abs);
                }
            }
        }
    }

    /// Generate insert method logic
    /// Generate the per-field column-append statements for one row.
    ///
    /// Reads each stored field from a local `record` binding and appends it to the
    /// matching column, in declared order.  Shared by `insert` (appends the new
    /// record), `update` (appends the superseding version), and `delete` (re-appends
    /// the current values under a tombstone) so all three stay byte-for-byte
    /// consistent about column encoding — the whole point of the append-only /
    /// superseding-version model (#66).
    fn generate_append_statements(model: &forgedb_parser::Model) -> Vec<TokenStream> {
        let mut append_statements = Vec::new();

        for field in &model.fields {
            let field_name = format_ident!("{}", field.name);
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_json_type(&field.field_type) {
                if field.is_nullable() {
                    // Nullable json (`json?` -> Option<serde_json::Value>).  Encode
                    // with a 1-byte presence tag so `None` and `Some(Value::Null)`
                    // round-trip distinctly (0x00 = None, 0x01 = Some), then store
                    // the serialized JSON (always valid UTF-8) as a variable-length
                    // string — the same column machinery `string` uses.
                    append_statements.push(quote! {
                        {
                            let encoded = match &record.#field_name {
                                Some(v) => {
                                    let s = serde_json::to_string(v)
                                        .expect("Failed to serialize json");
                                    let mut e = String::with_capacity(s.len() + 1);
                                    e.push('\u{1}');
                                    e.push_str(&s);
                                    e
                                }
                                None => String::from('\u{0}'),
                            };
                            self.#field_col_name.append_string(&encoded)
                                .expect("Failed to append string");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        {
                            let s = serde_json::to_string(&record.#field_name)
                                .expect("Failed to serialize json");
                            self.#field_col_name.append_string(&s)
                                .expect("Failed to append string");
                        }
                    });
                }
            } else if Self::is_string_type(&field.field_type) {
                if field.is_nullable() {
                    // Nullable string (`string?` -> Option<String>).  Encode with a
                    // 1-byte presence tag so `None` and `Some("")` round-trip
                    // distinctly (0x00 = None, 0x01 = Some), then store as an
                    // ordinary variable-length string.
                    append_statements.push(quote! {
                        {
                            let encoded = match &record.#field_name {
                                Some(s) => {
                                    let mut e = String::with_capacity(s.len() + 1);
                                    e.push('\u{1}');
                                    e.push_str(s);
                                    e
                                }
                                None => String::from('\u{0}'),
                            };
                            self.#field_col_name.append_string(&encoded)
                                .expect("Failed to append string");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        self.#field_col_name.append_string(&record.#field_name)
                            .expect("Failed to append string");
                    });
                }
            } else if Self::is_enum_type(&field.field_type) {
                // Enum (#enum): store the 1-byte discriminant via the generated
                // `__to_u8`.  Nullable enum stores a 2-byte `[present, disc]` so
                // `None` and `Some(variant-0)` round-trip distinctly.
                if field.is_nullable() {
                    append_statements.push(quote! {
                        {
                            let bytes: [u8; 2] = match &record.#field_name {
                                Some(v) => [1u8, v.__to_u8()],
                                None => [0u8, 0u8],
                            };
                            self.#field_col_name.append_bytes(&bytes)
                                .expect("Failed to append to column");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        self.#field_col_name.append_bytes(&[record.#field_name.__to_u8()])
                            .expect("Failed to append to column");
                    });
                }
            } else if Self::is_fixed_size_type(&field.field_type) {
                // Check if this is a complex type that needs byte conversion.
                // OptionalReference is treated like Nullable(Uuid) — stored as Option<Uuid> bytes.
                let needs_byte_conversion = matches!(
                    &field.field_type,
                    forgedb_parser::FieldType::Char(_)
                        | forgedb_parser::FieldType::FixedArray(_, _)
                        | forgedb_parser::FieldType::StructType(_)
                        | forgedb_parser::FieldType::OptionalStructType(_)
                        | forgedb_parser::FieldType::Nullable(_)
                        | forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(_),
                        )
                );

                if needs_byte_conversion {
                    // C3: use size_of_val so the byte count matches the column size
                    append_statements.push(quote! {
                        {
                            let bytes = unsafe {
                                std::slice::from_raw_parts(
                                    &record.#field_name as *const _ as *const u8,
                                    std::mem::size_of_val(&record.#field_name)
                                )
                            };
                            self.#field_col_name.append_bytes(bytes)
                                .expect("Failed to append to column");
                        }
                    });
                } else {
                    // For primitives, use typed method
                    let append_method = Self::get_append_method(&field.field_type);

                    // UUID and RequiredReference (FK scalar) both store a raw [u8; 16]
                    let is_uuid_like = matches!(
                        &field.field_type,
                        forgedb_parser::FieldType::Uuid
                            | forgedb_parser::FieldType::Relation(
                                forgedb_parser::RelationType::RequiredReference(_),
                            )
                    );
                    // Timestamp is a newtype over i64; append_timestamp expects i64
                    let is_timestamp =
                        matches!(&field.field_type, forgedb_parser::FieldType::Timestamp);
                    // Non-null decimal: a fixed 16-byte column stored via
                    // Decimal::serialize() -> [u8; 16], on the raw uuid byte path.
                    let is_decimal =
                        matches!(&field.field_type, forgedb_parser::FieldType::Decimal);

                    if is_decimal {
                        append_statements.push(quote! {
                            self.#field_col_name.append_uuid(record.#field_name.serialize())
                                .expect("Failed to append to column");
                        });
                    } else if is_uuid_like {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(*record.#field_name.as_bytes())
                                .expect("Failed to append to column");
                        });
                    } else if is_timestamp {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(i64::from(record.#field_name))
                                .expect("Failed to append to column");
                        });
                    } else {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(record.#field_name)
                                .expect("Failed to append to column");
                        });
                    }
                }
            }
        }

        append_statements
    }

    /// The field identifier used as a model's identity, e.g. `id`.  Falls back to
    /// `id` when no explicit id / auto-generate field is present (matching the
    /// historical `insert` behavior).
    fn id_field_ident(model: &forgedb_parser::Model) -> proc_macro2::Ident {
        match model.fields.iter().find(|f| f.name == "id" || f.auto_generate) {
            Some(f) => format_ident!("{}", f.name),
            None => format_ident!("id"),
        }
    }

    /// Generate an expression that reads a model's id at a physical row via
    /// `#receiver.<id_col>.read_*(#row_var)`, yielding the id-typed value.
    ///
    /// `None` when the model has no id / auto-generate field (such a model cannot
    /// `insert`, `update`, or `delete`).  Used by reopen rehydration (receiver
    /// `db`, row `i`) and by the snapshot newest-version resolution added for the
    /// mutation surface (receiver `self`, row `row_index`) so both read the id the
    /// same way (#66 + #56).
    fn generate_id_read_expr(
        model: &forgedb_parser::Model,
        receiver: &TokenStream,
        row_var: &proc_macro2::Ident,
    ) -> Option<TokenStream> {
        let f = model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)?;
        let col = format_ident!("{}_col", f.name);

        let is_uuid_like = matches!(
            &f.field_type,
            forgedb_parser::FieldType::Uuid
                | forgedb_parser::FieldType::Relation(
                    forgedb_parser::RelationType::RequiredReference(_),
                )
        );
        let is_timestamp = matches!(&f.field_type, forgedb_parser::FieldType::Timestamp);
        let is_string = Self::is_string_type(&f.field_type);

        let expr = if is_uuid_like {
            quote! {
                {
                    let bytes = #receiver.#col.read_uuid(#row_var).expect("Failed to read id column");
                    Uuid::from_bytes(bytes)
                }
            }
        } else if is_timestamp {
            quote! {
                Timestamp::from(
                    #receiver.#col.read_timestamp(#row_var).expect("Failed to read id column"),
                )
            }
        } else if is_string {
            quote! {
                #receiver.#col.read_string(#row_var).expect("Failed to read id column")
            }
        } else {
            let read_method = Self::get_read_method(&f.field_type);
            quote! {
                #receiver.#col.#read_method(#row_var).expect("Failed to read id column")
            }
        };
        Some(expr)
    }

    /// Emit the WAL commit record for one mutation (#89 durable write path).
    ///
    /// The generated code serializes the row (`serde_json`) and hands the WAL
    /// opaque bytes via the schema-agnostic `Raw` op — the WAL never decodes a
    /// field.  Layout: `[row_index: u64 LE][deleted: u8][serde_json(record)]`.
    /// The absolute `row_index` makes replay idempotent (recovery skips a record
    /// whose row is already materialized).  Emitted BEFORE the column appends, so
    /// with `FsyncPolicy::Always` the row is durable in the WAL before it is
    /// applied; a crash mid-append is recovered from the WAL on reopen.  Assumes
    /// a `record` binding (the row's values) is in scope.
    fn generate_wal_record_write(model: &forgedb_parser::Model, deleted: bool) -> TokenStream {
        let model_name_str = model.name.clone();
        let deleted_tok = if deleted {
            quote! { 1u8 }
        } else {
            quote! { 0u8 }
        };
        quote! {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(#deleted_tok);
                __wal_payload.extend_from_slice(
                    &serde_json::to_vec(&record).expect("Failed to serialize record for WAL")
                );
                self.wal
                    .write(&forgedb_wal::WalEntry::raw(#model_name_str, __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
    }

    /// Emit the durable replication record for one mutation (#82 Direction C).
    ///
    /// Alongside the best-effort in-process change-feed emit, record the change to
    /// the durable, offset-addressed broker so a cross-process / browser follower
    /// (#110) can resume from a watermark. The broker is **field-blind**: it is
    /// handed the model name (an opaque routing tag), the absolute `row_index` of
    /// the appended physical row, the `kind`, and the **opaque serialized row
    /// bytes** — the same `serde_json(record)` bytes the WAL journals. The broker
    /// assigns the global offset and never interprets the bytes.
    ///
    /// Assumes `record` (the row's values) and `row_index` (the appended physical
    /// row) are in scope — true at every mutation emit site (insert / update /
    /// delete). `None` on the standalone `Database::new()` path where no broker is
    /// attached; a per-tenant `open_at` process attaches one.
    fn generate_broker_record(model: &forgedb_parser::Model, kind: TokenStream) -> TokenStream {
        let model_name_str = model.name.clone();
        quote! {
            if let Some(__broker) = &self.broker {
                // Reuse the WAL's opaque row encoding; the broker never decodes it.
                let __row_bytes = serde_json::to_vec(&record).unwrap_or_default();
                if let Ok(mut __b) = __broker.lock() {
                    let _ = __b.record(
                        #model_name_str,
                        row_index as u64,
                        #kind,
                        __row_bytes,
                    );
                }
            }
        }
    }

    /// Emit one "append a default entry" statement per storage-backed field —
    /// the same column encoding as `generate_append_statements`, but with the
    /// field's type default instead of a record value.  Used to backfill a newly
    /// added column so existing rows keep a well-formed value (#92 Phase 4 additive
    /// migration).  Defaults are type-zero / null (nullable → None, string → "",
    /// numeric → 0, uuid/FK → nil); the schema `@default` is not yet honored here
    /// (a follow-up), which is why additive fields should be nullable or carry a
    /// meaningful zero.
    fn generate_backfill_appends(model: &forgedb_parser::Model) -> Vec<(proc_macro2::Ident, TokenStream)> {
        let mut out = Vec::new();
        for field in &model.fields {
            let col = format_ident!("{}_col", field.name);
            if Self::is_json_type(&field.field_type) {
                let one = if field.is_nullable() {
                    // Nullable json: the 1-byte presence tag `\u{0}` = None.
                    quote! {
                        self.#col.append_string(&String::from('\u{0}'))
                            .expect("Failed to backfill json column");
                    }
                } else {
                    // Non-null json backfill: the serialized form of
                    // `serde_json::Value::Null` (`"null"`), so a read decodes to
                    // `Value::Null` — the meaningful zero for a JSON column.
                    quote! {
                        self.#col.append_string("null")
                            .expect("Failed to backfill json column");
                    }
                };
                out.push((col, one));
            } else if Self::is_enum_type(&field.field_type) {
                // enum backfill: nullable → `[0, 0]` (None); non-null → `[0]`
                // (variant 0, the meaningful zero — matches the append encoding).
                let one = if field.is_nullable() {
                    quote! {
                        self.#col.append_bytes(&[0u8, 0u8])
                            .expect("Failed to backfill enum column");
                    }
                } else {
                    quote! {
                        self.#col.append_bytes(&[0u8])
                            .expect("Failed to backfill enum column");
                    }
                };
                out.push((col, one));
            } else if Self::is_string_type(&field.field_type) {
                let one = if field.is_nullable() {
                    // Nullable string: the 1-byte presence tag `\u{0}` = None.
                    quote! {
                        self.#col.append_string(&String::from('\u{0}'))
                            .expect("Failed to backfill string column");
                    }
                } else {
                    quote! {
                        self.#col.append_string("")
                            .expect("Failed to backfill string column");
                    }
                };
                out.push((col, one));
            } else if Self::is_fixed_size_type(&field.field_type) {
                let needs_byte_conversion = matches!(
                    &field.field_type,
                    forgedb_parser::FieldType::Char(_)
                        | forgedb_parser::FieldType::FixedArray(_, _)
                        | forgedb_parser::FieldType::StructType(_)
                        | forgedb_parser::FieldType::OptionalStructType(_)
                        | forgedb_parser::FieldType::Nullable(_)
                        | forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(_),
                        )
                );
                let one = if needs_byte_conversion {
                    // Byte-blob types (char(N), arrays, inline struct, nullable /
                    // optional-FK) backfill as zeroed bytes of the column width —
                    // decodes to None / all-zero, matching the append encoding.
                    let value_size = Self::column_value_size_expr(&field.field_type);
                    quote! {
                        self.#col.append_bytes(&vec![0u8; #value_size])
                            .expect("Failed to backfill column");
                    }
                } else {
                    let append_method = Self::get_append_method(&field.field_type);
                    let is_uuid_like = matches!(
                        &field.field_type,
                        forgedb_parser::FieldType::Uuid
                            | forgedb_parser::FieldType::Relation(
                                forgedb_parser::RelationType::RequiredReference(_),
                            )
                    );
                    let is_timestamp =
                        matches!(&field.field_type, forgedb_parser::FieldType::Timestamp);
                    let is_decimal =
                        matches!(&field.field_type, forgedb_parser::FieldType::Decimal);
                    if is_decimal {
                        // Non-null decimal backfills to Decimal::ZERO (its 16-byte
                        // serialized form), so a read decodes to `0` — the zero value.
                        quote! {
                            self.#col.append_uuid(rust_decimal::Decimal::ZERO.serialize())
                                .expect("Failed to backfill column");
                        }
                    } else if is_uuid_like {
                        quote! {
                            self.#col.#append_method([0u8; 16])
                                .expect("Failed to backfill column");
                        }
                    } else if is_timestamp {
                        quote! {
                            self.#col.#append_method(0i64)
                                .expect("Failed to backfill column");
                        }
                    } else if matches!(&field.field_type, forgedb_parser::FieldType::F64) {
                        quote! {
                            self.#col.#append_method(0.0)
                                .expect("Failed to backfill column");
                        }
                    } else if matches!(&field.field_type, forgedb_parser::FieldType::Bool) {
                        quote! {
                            self.#col.#append_method(false)
                                .expect("Failed to backfill column");
                        }
                    } else {
                        quote! {
                            self.#col.#append_method(0)
                                .expect("Failed to backfill column");
                        }
                    }
                };
                out.push((col, one));
            }
        }
        out
    }

    /// Emit `recover_from_wal(&mut self)` (#89): torn-tail repair + WAL replay on
    /// reopen — plus additive-field backfill (#92 Phase 4).  The tombstone file is
    /// the authoritative committed row count (appended LAST in every insert, so it
    /// never over-counts).  Realign every column to that anchor: a column SHORTER
    /// than the anchor is a newly-added field (the schema was regenerated with an
    /// extra field) — backfill it with the field's default so existing rows stay
    /// intact; a column LONGER than the anchor is a torn insert tail — truncate it.
    /// Then replay the WAL, re-driving every record whose absolute row index is
    /// `>= row_count`.  Because `FsyncPolicy::Always` made each record durable
    /// before its columns were touched, a row acked to the client survives even if
    /// its column append was lost to the page cache.  Recovery decode is generated
    /// per-model here (the columns are baked in) — the WAL crate stays field-blind,
    /// and there is no runtime `model_name` dispatch.  Emits no change-feed events.
    fn generate_recover_method(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let append_statements = Self::generate_append_statements(model);
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(&f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();

        // Additive-migration backfill (#92 Phase 4): for each column shorter than
        // the anchor, append its default until it reaches the anchor.
        let backfill_loops: Vec<_> = Self::generate_backfill_appends(model)
            .into_iter()
            .map(|(col, append_default)| {
                quote! {
                    while self.#col.len() < __anchor {
                        #append_default
                    }
                }
            })
            .collect();

        quote! {
            fn recover_from_wal(&mut self) {
                // The tombstone file is the authoritative committed row count.
                let __anchor = self.tombstones.len();
                // Backfill newly-added (short) columns up to the anchor so existing
                // rows keep a well-formed default value (#92 additive migration).
                #(#backfill_loops)*
                // Drop any torn / ahead column tail so all columns realign at the
                // anchor (a crash can leave one column ahead of the tombstone).
                #(
                    self.#col_idents
                        .truncate_to_rows(__anchor)
                        .expect("Failed to truncate column on recovery");
                )*
                self.row_count = __anchor;

                // Replay the WAL, re-driving records the columns don't yet have.
                let __entries = self
                    .wal
                    .replay(|_| -> std::io::Result<()> { Ok(()) })
                    .expect("Failed to replay WAL on recovery");
                for __entry in &__entries {
                    if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                        if payload.len() < 9 {
                            continue;
                        }
                        let mut __ri = [0u8; 8];
                        __ri.copy_from_slice(&payload[0..8]);
                        let __row_index = u64::from_le_bytes(__ri) as usize;
                        // Already materialized in the columns — idempotent skip.
                        if __row_index < self.row_count {
                            continue;
                        }
                        let __deleted = payload[8] != 0;
                        let record: #model_name = match serde_json::from_slice(&payload[9..]) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        #(#append_statements)*
                        self.tombstones
                            .append(__deleted)
                            .expect("Failed to append tombstone on recovery");
                        self.row_count += 1;
                    }
                }
            }
        }
    }

    /// Emit the follower's per-model `apply(kind, bytes)` (#110 Milestone C).
    ///
    /// The browser read-replica receives an opaque `PersistedEvent` frame and,
    /// once `Database::apply_frame` has dispatched by the field-blind model tag,
    /// hands the row bytes here.  This method decodes them into the typed record
    /// and **replays through the SAME generated `insert` / `update` / `delete`**
    /// the server uses — no second write path, so there is no drift risk.  On a
    /// follower the change feed and durable broker are `None` and the WAL is the
    /// in-memory wasm variant, so those side effects are inert; the replay is
    /// purely the column-append + `id_to_row` + index maintenance the server did.
    ///
    /// Idempotent by absolute row position for a from-zero replay: applying the
    /// frames in offset order reproduces the server's exact physical layout.
    /// `None` (no method emitted) for a model with no id — it cannot be mutated
    /// and so is never replicated.
    fn generate_apply_method(model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.fields.iter().find(|f| f.name == "id" || f.auto_generate)?;
        let model_name = format_ident!("{}", model.name);
        let id_field = Self::id_field_ident(model);
        Some(quote! {
            /// Apply a replicated change for this model on the read-replica
            /// follower (#110).  Decodes the opaque row bytes the server journaled
            /// and replays through the same generated mutation surface.  The
            /// follower never decodes a field to *route* — `Database::apply_frame`
            /// already dispatched by the model tag; this only materializes the
            /// typed record to *apply* it.
            pub fn apply(
                &mut self,
                kind: forgedb_changefeed::ChangeKind,
                bytes: &[u8],
            ) -> Result<(), ApplyError> {
                match kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        self.insert(record)?;
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        let id = record.#id_field;
                        self.update(id, record)?;
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        let id = record.#id_field;
                        self.delete(id);
                    }
                    forgedb_changefeed::ChangeKind::Linked => {
                        // A junction link is not a model event; it is applied by
                        // `Database::apply_frame` on the junction table directly.
                    }
                }
                Ok(())
            }
        })
    }

    /// Emit the additive `commit(&mut self)` (#110 Milestone C).
    ///
    /// Flushes every column + the tombstone file.  Natively this is an `fsync`
    /// (the same durability boundary `checkpoint` uses, minus the WAL truncate);
    /// on the wasm arena backend `flush` is a no-op and durability instead comes
    /// from the transport writing the arena blobs to IndexedDB/OPFS after this
    /// returns (`forgedb_storage_web::dump`).  Target-agnostic generated code —
    /// the difference lives entirely in the storage facade.
    fn generate_commit_method(model: &forgedb_parser::Model) -> TokenStream {
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(&f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();
        quote! {
            /// Flush this model's columns + tombstones (#110 additive commit).
            /// Native fsync; wasm arena no-op (durability at the transport's
            /// IndexedDB/OPFS write boundary).
            pub fn commit(&mut self) -> std::io::Result<()> {
                #(
                    self.#col_idents.flush()?;
                )*
                self.tombstones.flush()?;
                Ok(())
            }
        }
    }

    /// Emit `checkpoint(&mut self)` (#89 step 2 — bound the WAL).  Makes every
    /// column + the tombstone file durable (`flush()` == `sync_all`), THEN
    /// truncates the WAL.  The order is load-bearing: the WAL is discarded only
    /// after the data it protects is durable, so a crash *between* the two leaves
    /// the (not-yet-truncated) WAL able to replay, and a crash *after* leaves
    /// durable columns and an empty WAL.  Recovery derives the durable prefix
    /// from the column file lengths themselves (`recover_from_wal`), so no
    /// persisted checkpoint marker is load-bearing — this only bounds the WAL and
    /// shortens reopen.  Single-writer only, and called between mutations, so all
    /// columns are aligned at `row_count` when it runs.
    fn generate_checkpoint_method(model: &forgedb_parser::Model) -> TokenStream {
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(&f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();

        quote! {
            /// Force a WAL checkpoint (#89 step 2): fsync this model's columns,
            /// then truncate its WAL.  Bounds the WAL and shortens reopen; not
            /// required for correctness (recovery reads the column lengths).
            pub fn checkpoint(&mut self) {
                // 1. Make the committed columns durable up to `row_count`.
                #(
                    self.#col_idents
                        .flush()
                        .expect("Failed to fsync column on checkpoint");
                )*
                self.tombstones
                    .flush()
                    .expect("Failed to fsync tombstones on checkpoint");
                // 2. Only now that the data is durable, discard the redundant WAL.
                self.wal
                    .truncate()
                    .expect("Failed to truncate WAL on checkpoint");
                self.writes_since_checkpoint = 0;
            }
        }
    }

    /// Emit `compact(&mut self)` (#92 Phase 4 — bound column storage).  Reclaims
    /// the dead (superseded/tombstoned) row versions the mutation surface (#66)
    /// leaves behind, IN-PROCESS under the single-writer discipline.  Never a
    /// background thread: #89's `DirLock` makes single-writer the durability
    /// contract, so compaction must run on the writer, between mutations.
    ///
    /// Order is load-bearing:
    /// 1. `checkpoint()` first — fsync columns + truncate the WAL, so no
    ///    index-relative WAL tail (#89, which replays by *absolute* row index)
    ///    survives the row renumbering compaction is about to do.
    /// 2. Generated code computes the LIVE physical-row set from `id_to_row` +
    ///    tombstone liveness (the field-aware decision) and hands that opaque set
    ///    to `forgedb_compaction::Compactor::compact_model_keeping`, which keeps
    ///    exactly those rows and drops the rest.  The substrate is schema-blind
    ///    byte GC keyed by the model *directory name* + a list of row indices — it
    ///    reads no `.forge`, no field types.  (Only `compact_model_keeping` is
    ///    linked — never `BackgroundCompactor`, which would compact off the writer
    ///    lock.)  The keep-set model — not `compact_model`'s tombstone model — is
    ///    load-bearing: #66 leaves superseded update-versions orphaned but NOT
    ///    tombstoned, and marks a delete with a *new* tombstoned marker row, so a
    ///    tombstone-driven drop reclaims nothing from updates and would resurrect
    ///    deletes.
    /// 3. Compaction renumbers surviving rows, invalidating every in-memory
    ///    `id_to_row` / secondary-index entry, so reopen the storage to rebuild
    ///    them from the compacted files via the SAME reopen-scan as `new_at`.
    ///
    /// Best-effort: a compaction error leaves the (still-correct) un-compacted
    /// files in place and retries after the next interval — it never corrupts
    /// live state.
    fn generate_compact_method(model: &forgedb_parser::Model) -> TokenStream {
        let model_snake = Self::to_snake_case(&model.name);
        quote! {
            /// Reclaim dead row versions in-process (#92 Phase 4): checkpoint,
            /// rewrite this model's files dropping dead rows, then reopen to
            /// rebuild `id_to_row` + secondary indexes from the compacted files.
            pub fn compact(&mut self) {
                // MVCC Tier 1 (#83, M1d): never compact mid-transaction — the
                // renumbering below would invalidate a live transaction's rollback
                // marks (physical row lengths) and could reclaim a version the
                // transaction's read snapshot still needs.  Record it as due and
                // run it after the transaction closes (`run_deferred_maintenance`).
                if self.in_transaction {
                    self.compact_deferred = true;
                    return;
                }
                // 1. Make columns durable + discard the WAL tail (no index-relative
                //    replay may survive the renumbering below).
                self.checkpoint();
                // 2. Compute the LIVE physical-row set — the field-aware decision,
                //    made here in generated code, never in the substrate.  For the
                //    superseding-version mutation surface (#66), `id_to_row` maps
                //    each id to its newest physical row: a live id's newest row is
                //    non-tombstoned; a deleted id's newest row is the tombstoned
                //    marker, which we OMIT so the delete is truly reclaimed (and
                //    never resurrected).  Orphaned old versions are absent from
                //    `id_to_row.values()` entirely, so they drop out too.
                let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
                for &__row in self.id_to_row.values() {
                    // Keep-on-error: an unclassifiable row is retained (never lost),
                    // and reopen's version-resolving read still reads it correctly —
                    // worst case it is simply reclaimed on a later pass.
                    if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                        __keep.push(__row);
                    }
                }
                // 3. Hand the opaque live-row set to the schema-blind byte GC.
                let __config = forgedb_compaction::CompactionConfig::default();
                let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
                if __compactor.compact_model_keeping(#model_snake, &__keep).is_ok() {
                    // 3. Rows renumbered: reopen to rebuild the in-memory maps from
                    //    the compacted files (preserving the shared change feed and
                    //    the durable replication broker across the reopen).
                    let __root = self.root.clone();
                    let __feed = self.changefeed.take();
                    let __broker = self.broker.take();
                    *self = Self::new_at(&__root);
                    self.changefeed = __feed;
                    self.broker = __broker;
                    // 4. Break the incremental-backup chain (#76): renumbering
                    //    moved every row's bytes, so a byte-tail delta computed
                    //    against a pre-compaction base is invalid.  Bump the epoch
                    //    AFTER the reopen (which rewrote the manifest preserving the
                    //    old epoch) so an incremental across this boundary is
                    //    refused rather than silently corrupt.
                    self.bump_compaction_epoch(&__root);
                }
                // Reset unconditionally: a failed compaction waits a full interval
                // rather than re-attempting on every subsequent mutation.
                self.dead_since_compaction = 0;
            }
        }
    }

    /// Generate the transaction storage helpers on a model's `*Storage` (MVCC
    /// Tier 1, #83): `__stage_append` (the low-level staged write the `TxHandle`
    /// drives) and `run_deferred_maintenance` (runs a checkpoint/compaction the
    /// txn guard deferred).  `None`-skipped for a model without an id (which
    /// cannot be mutated, so it is never touched by a transaction).
    ///
    /// `__stage_append` is deliberately the SAME column + WAL write as the
    /// committed `insert`/`update`/`delete` path (it reuses the shared
    /// `generate_append_statements` and `generate_wal_record_write`), but it does
    /// **not** advance `id_to_row`, maintain indexes, emit the changefeed / broker,
    /// or checkpoint.  That is
    /// exactly the split the note requires: staged rows are physically appended
    /// (past the public watermark) and durable in the per-model WAL, but invisible
    /// to outside readers and unindexed until the transaction's commit applies the
    /// buffered index ops and advances the watermark.  Read-your-writes inside the
    /// txn comes from `get_at` with the watermark raised to the staged length — one
    /// decode path, no fork.
    fn generate_txn_storage_methods(model: &forgedb_parser::Model) -> TokenStream {
        if model.fields.iter().find(|f| f.name == "id" || f.auto_generate).is_none() {
            return quote! {};
        }
        let model_name = format_ident!("{}", model.name);
        let append_statements = Self::generate_append_statements(model);
        let wal_write_live = Self::generate_wal_record_write(model, false);
        let wal_write_deleted = Self::generate_wal_record_write(model, true);
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(&f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();
        // Reindex body: clear every in-memory map, then rebuild `id_to_row` +
        // secondary indexes from the committed prefix via the SAME narrow decode
        // path `new_at` uses (one body, no drift) — but on `self` in place.
        let clear_indexes: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { self.#ident.clear(); }
            })
            .chain(Self::composite_indexes(model).iter().map(|(ident, _)| {
                quote! { self.#ident.clear(); }
            }))
            .collect();
        let rehydrate_self = Self::generate_rehydrate_body(model, &quote! { self });
        quote! {
            /// Rebuild `id_to_row` + secondary indexes from the committed prefix
            /// in place (MVCC Tier 1, #83).  A transaction stages rows via
            /// `__stage_append`, which deliberately does NOT touch the identity or
            /// secondary index maps; its `commit` calls this once per touched
            /// collection to make the newly-visible rows resolvable.  It clears the
            /// maps and re-runs the SAME reopen-rehydration body `new_at` uses
            /// (last-write-wins per id, tombstone-gated index keys), so the maps
            /// end up exactly as a fresh reopen would build them — no incremental
            /// remove-old/add-new bookkeeping to drift.
            pub fn __reindex_committed(&mut self) {
                self.id_to_row.clear();
                #(#clear_indexes)*
                #rehydrate_self
            }

            /// Refresh EVERY column + the tombstone's in-memory `row_count` from
            /// the on-disk file lengths, then adopt the peer-visible row count
            /// (MVCC Tier 3, #84 — peer read-currency).  Reads are positional and
            /// bounded by each column's cached `row_count`; a *peer* process's
            /// appended rows stay invisible until this re-derives those counts
            /// from the shared files.  Syncing ONLY the tombstone (as an earlier
            /// cut did) bumps the row count but leaves every data column bounded
            /// at its open-time length, so `get`/`read_at` of a peer row reads out
            /// of bounds — this syncs them all.  Uses the additive
            /// `*::sync_from_disk` substrate (`storage-native`); writes nothing
            /// (T3-8: generated data-plane read-refresh, no column append/flush).
            /// Caller pairs this with `__reindex_committed` to rebuild the maps.
            pub fn __sync_columns_from_disk(&mut self) {
                #(
                    let _ = self.#col_idents.sync_from_disk();
                )*
                let _ = self.tombstones.sync_from_disk();
                self.row_count = self.tombstones.len();
            }

            /// Truncate every column + the tombstone back to `rows` (MVCC Tier 1,
            /// #83).  The transaction rollback primitive: erases staged ahead-rows
            /// so the collection returns to its pre-txn physical length.  Reuses the
            /// published `truncate_to_rows` on each column type (no substrate change).
            /// A no-op past the current length (nothing staged).
            pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
                #(
                    self.#col_idents.truncate_to_rows(rows)?;
                )*
                self.tombstones.truncate_to_rows(rows)?;
                Ok(())
            }

            /// Journal-driven recovery to a committed row-count (MVCC Tier 1, #83,
            /// M1b).  Called at `open_at` when the `_txn_journal` names a committed
            /// length for this model that is BELOW the length the columns/WAL
            /// recovered to — i.e. a transaction staged rows into this collection
            /// but its journal commit record never landed (a torn / aborted
            /// commit).  Truncate every column + the tombstone back to
            /// `committed_len` (erasing the aborted ahead-rows), discard the WAL
            /// (the committed prefix is already durable in the columns, and the
            /// aborted staged WAL records must not replay), then reopen to rebuild
            /// `id_to_row` + secondary indexes from the truncated committed prefix.
            /// A no-op when `committed_len >= row_count` (nothing aborted).
            pub fn __recover_to_committed(&mut self, committed_len: usize) {
                if committed_len >= self.row_count {
                    return;
                }
                #(
                    self.#col_idents
                        .truncate_to_rows(committed_len)
                        .expect("Failed to truncate column on journal recovery");
                )*
                self.tombstones
                    .truncate_to_rows(committed_len)
                    .expect("Failed to truncate tombstones on journal recovery");
                self.row_count = committed_len;
                // Discard the WAL: its committed prefix is durable in the columns
                // (which we just made authoritative at `committed_len`), and its
                // aborted staged tail must NOT replay on the reopen below.
                self.wal
                    .truncate()
                    .expect("Failed to truncate WAL on journal recovery");
                // Reopen to rebuild the in-memory maps from the committed prefix,
                // preserving the shared feed / broker (as compaction's reopen does).
                let __root = self.root.clone();
                let __feed = self.changefeed.take();
                let __broker = self.broker.take();
                *self = Self::new_at(&__root);
                self.changefeed = __feed;
                self.broker = __broker;
            }

            /// Stage one physically-appended row for a transaction (MVCC Tier 1,
            /// #83).  WAL-records the row (durable, `FsyncPolicy::Always`) then
            /// appends every column + the tombstone at the current physical row
            /// index, bumps `row_count`, and returns that index — WITHOUT touching
            /// `id_to_row`, secondary indexes, the change feed, the broker, or the
            /// checkpoint counter.  The transaction's `commit` applies those
            /// deferred effects once, atomically; a `rollback` truncates every
            /// staged row + the staged WAL tail back to the pre-txn mark.  The row
            /// is invisible to outside readers (whose `get`/`all` clamp at the
            /// public watermark) until commit advances the watermark past it.
            pub fn __stage_append(&mut self, record: #model_name, deleted: bool) -> usize {
                let row_index = self.row_count;
                // Durability boundary (#89): WAL-record the row before any column
                // is touched, so a crash mid-stage is repaired on reopen and the
                // aborted tail is dropped by journal-driven recovery.
                if deleted {
                    #wal_write_deleted
                } else {
                    #wal_write_live
                }
                // Append all columns from `record`, then the tombstone flag.
                #(#append_statements)*
                self.tombstones
                    .append(deleted)
                    .expect("Failed to append tombstone");
                self.row_count += 1;
                row_index
            }

            /// Run any checkpoint/compaction the transaction guard deferred (MVCC
            /// Tier 1, #83).  Called by `TxHandle::commit`/rollback once the
            /// critical section closes and `in_transaction` is cleared, so the
            /// #96/#92 auto-maintenance still runs — just after the transaction,
            /// never underneath its rollback marks.  Compaction subsumes a
            /// checkpoint, so if both are due only compaction runs.
            pub fn run_deferred_maintenance(&mut self) {
                if self.compact_deferred {
                    self.compact_deferred = false;
                    self.checkpoint_deferred = false;
                    self.compact();
                } else if self.checkpoint_deferred {
                    self.checkpoint_deferred = false;
                    self.checkpoint();
                }
            }
        }
    }

    /// Emit the tail every mutation runs after its columns are appended and
    /// `row_count` is bumped: count the write and checkpoint once the interval is
    /// reached.  Placed AFTER the append + increment so the checkpoint always sees
    /// a consistent, fully-appended column prefix.
    fn generate_maybe_checkpoint() -> TokenStream {
        quote! {
            // Bound the WAL (#89 step 2): after enough mutations, fsync columns
            // and truncate the WAL so it never grows without limit.
            self.writes_since_checkpoint += 1;
            if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
                // MVCC Tier 1 (#83): a checkpoint mid-transaction would truncate
                // the per-model WAL underneath the rollback — defer it to after
                // the transaction commits/rolls back (`run_deferred_maintenance`).
                if self.in_transaction {
                    self.checkpoint_deferred = true;
                } else {
                    self.checkpoint();
                }
            }
        }
    }

    /// Emit the tail an update/delete runs after its dead-version bookkeeping:
    /// count the dead row and compact once enough accumulate (#92 Phase 4).  Only
    /// update/delete emit this — an insert produces no dead version.  Placed AFTER
    /// `#maybe_checkpoint` so a compaction (which itself checkpoints) sees a
    /// consistent, bounded WAL.
    fn generate_maybe_compact() -> TokenStream {
        quote! {
            // Bound column storage (#92 Phase 4): this mutation left a dead
            // (superseded/tombstoned) row version behind; once enough accumulate,
            // reclaim them in-process under the writer lock.
            self.dead_since_compaction += 1;
            if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
                // MVCC Tier 1 (#83): compaction renumbers physical rows, which
                // would invalidate a live transaction's rollback marks (physical
                // lengths).  Defer it to after commit/rollback.
                if self.in_transaction {
                    self.compact_deferred = true;
                } else {
                    self.compact();
                }
            }
        }
    }

    fn generate_insert_logic(model: &forgedb_parser::Model) -> TokenStream {
        let append_statements = Self::generate_append_statements(model);
        let id_field_name = Self::id_field_ident(model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, false);
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Inserted });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        // Data-integrity gate (#91): validate field constraints + `&unique` BEFORE
        // any durable side effect, so a rejected insert commits absolutely nothing.
        let validate_fn = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let unique_checks = Self::generate_unique_checks(model, false);

        // Secondary-index maintenance (#90): after the row is committed and the id
        // is mapped, add this id under each indexed field's value key.  Sequenced
        // after the WAL commit + `id_to_row` insert so the index only ever keys a
        // durable row.
        let recv = quote! { self };
        let id_tok = quote! { id };
        let index_adds: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fname = format_ident!("{}", f.name);
                let val = Self::index_value_expr(&f.field_type, quote! { record.#fname });
                Self::index_add_block(&recv, &ident, val, &id_tok)
            })
            .collect();
        // Composite-index maintenance (#101): same timing, keyed by the tuple.
        let composite_adds: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| {
                        let cf = format_ident!("{}", c.name);
                        Self::index_value_expr(&c.field_type, quote! { record.#cf })
                    })
                    .collect();
                Self::composite_add_block(&recv, ident, &parts, &id_tok)
            })
            .collect();

        quote! {
            // Integrity gate (#91): field constraints, then `&unique`.  Both run
            // before the WAL write, so a rejected insert leaves storage untouched.
            #validate_fn(&record)?;
            #(#unique_checks)*

            let row_index = self.row_count;
            let id = record.#id_field_name;

            // Durability boundary (#89): record + fsync the row in the WAL before
            // any column is touched, so a crash mid-append is recovered on reopen.
            #wal_write

            // Append all fields to their respective columns
            #(#append_statements)*

            // Mark as not deleted
            self.tombstones.append(false).expect("Failed to append tombstone");

            // Store ID mapping
            self.id_to_row.insert(id, row_index);
            self.row_count += 1;

            // Secondary-index maintenance (#90): index the committed row.
            #(#index_adds)*
            // Composite-index maintenance (#101).
            #(#composite_adds)*

            // Change-feed emit (#62 Direction A): this generated method knows a
            // #model_name_str was inserted; the substrate feed only carries the
            // field-blind (model, row_index) signal.  Best-effort, never blocks.
            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, row_index, forgedb_changefeed::ChangeKind::Inserted);
            }
            // Durable replication record (#82 Direction C): same field-blind signal
            // plus the opaque row bytes + a global offset, for a resumable follower.
            #broker_record

            #maybe_checkpoint

            Ok(id)
        }
    }

    /// Generate the body of `update(id, record)` (#66 mutation surface).
    ///
    /// Superseding-version append: the new field values are written at a fresh
    /// physical row and the id is repointed to it.  Committed bytes are never
    /// mutated in place, so append-only holds and backup / watermark snapshots
    /// (#57 / #56) keep working unchanged.  Returns `false` if the id is absent.
    /// `None` when the model has no id field (cannot be mutated by id).
    fn generate_update_logic(model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.fields.iter().find(|f| f.name == "id" || f.auto_generate)?;
        let append_statements = Self::generate_append_statements(model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, false);
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Updated });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        let maybe_compact = Self::generate_maybe_compact();
        // Integrity gate (#91): validate field constraints + `&unique` (excluding
        // the record's own id) before any durable side effect.
        let validate_fn = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let unique_checks = Self::generate_unique_checks(model, true);

        // Secondary-index maintenance (#90): an update that changes an indexed
        // field must drop the OLD value key and add the NEW one, else the index
        // keeps a stale hit pointing at a superseded row.  We remove using the
        // pre-update record (`__old`, resolved before the repoint) and add using
        // the incoming `record`.
        let indexed = Self::indexed_fields(model);
        let composites = Self::composite_indexes(model);
        // Fetch the pre-update record when any index (single or composite) needs
        // the old value keys to remove.
        let fetch_old = if indexed.is_empty() && composites.is_empty() {
            quote! {}
        } else {
            quote! { let __old = self.get(id); }
        };
        let recv = quote! { self };
        let id_tok = quote! { id };
        let index_updates: Vec<_> = indexed
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fname = format_ident!("{}", f.name);
                let remove_old = Self::index_remove_block(
                    &recv,
                    &ident,
                    Self::index_value_expr(&f.field_type, quote! { __old_rec.#fname }),
                    &id_tok,
                );
                let add_new = Self::index_add_block(
                    &recv,
                    &ident,
                    Self::index_value_expr(&f.field_type, quote! { record.#fname }),
                    &id_tok,
                );
                quote! {
                    if let Some(__old_rec) = &__old {
                        #remove_old
                    }
                    #add_new
                }
            })
            .collect();
        // Composite-index update maintenance (#101): remove the old tuple key
        // (from `__old`), add the new one (from `record`).
        let composite_updates: Vec<_> = composites
            .iter()
            .map(|(ident, comps)| {
                let old_parts: Vec<_> = comps
                    .iter()
                    .map(|c| {
                        let cf = format_ident!("{}", c.name);
                        Self::index_value_expr(&c.field_type, quote! { __old_rec.#cf })
                    })
                    .collect();
                let new_parts: Vec<_> = comps
                    .iter()
                    .map(|c| {
                        let cf = format_ident!("{}", c.name);
                        Self::index_value_expr(&c.field_type, quote! { record.#cf })
                    })
                    .collect();
                let remove_old = Self::composite_remove_block(&recv, ident, &old_parts, &id_tok);
                let add_new = Self::composite_add_block(&recv, ident, &new_parts, &id_tok);
                quote! {
                    if let Some(__old_rec) = &__old {
                        #remove_old
                    }
                    #add_new
                }
            })
            .collect();

        Some(quote! {
            if !self.id_to_row.contains_key(&id) {
                return Ok(false);
            }
            // Integrity gate (#91): validate the incoming record before touching
            // storage; `&unique` excludes this id so an unchanged value is fine.
            #validate_fn(&record)?;
            #(#unique_checks)*
            // Resolve the pre-update record (if live) for index removal.
            #fetch_old
            let row_index = self.row_count;

            // Durability boundary (#89): WAL-record the superseding version first.
            #wal_write

            // Append the superseding version's columns from `record`.
            #(#append_statements)*

            // The new version is live (not a tombstone).
            self.tombstones.append(false).expect("Failed to append tombstone");

            // Repoint the id at its newest physical row; the prior version's bytes
            // remain committed (backup-faithful) until compaction reclaims them.
            self.id_to_row.insert(id, row_index);
            self.row_count += 1;

            // Secondary-index maintenance (#90): drop stale value keys, add new.
            #(#index_updates)*
            // Composite-index maintenance (#101): drop stale tuple keys, add new.
            #(#composite_updates)*

            // #62: an Updated event carries the new live row index.
            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, row_index, forgedb_changefeed::ChangeKind::Updated);
            }
            // Durable replication record (#82): the superseding version's new live
            // row index + its opaque bytes, at a fresh global offset.
            #broker_record

            #maybe_checkpoint

            // Bound column storage (#92): this update superseded a live version,
            // leaving a dead row behind — compact once enough accumulate.
            #maybe_compact

            Ok(true)
        })
    }

    /// Generate the body of `delete(id)` (#66 mutation surface).
    ///
    /// Appends a *tombstoned* superseding version for the id.  The current values
    /// are re-appended (their content is irrelevant — the row reads as absent) only
    /// to keep every column aligned to the row count.  Reads resolve the newest
    /// version, now tombstoned, so `get` returns `None`.  Returns `false` if the id
    /// is already absent.  `None` when the model has no id field.
    fn generate_delete_logic(model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.fields.iter().find(|f| f.name == "id" || f.auto_generate)?;
        let append_statements = Self::generate_append_statements(model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, true);
        // The broker records the *tombstoned* physical row (`row_index`), so a
        // follower applies the tombstone at the same position; `record` (the
        // pre-delete values re-appended under the tombstone) supplies the bytes.
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Deleted });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        let maybe_compact = Self::generate_maybe_compact();

        // Secondary-index maintenance (#90): a delete appends a tombstoned
        // superseding row, so the id's index entry must be DROPPED (the physical
        // row still exists but the record reads as absent).  `record` below is
        // the pre-delete live version — its field values are the keys to remove.
        let recv = quote! { self };
        let id_tok = quote! { id };
        let index_removes: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fname = format_ident!("{}", f.name);
                let val = Self::index_value_expr(&f.field_type, quote! { record.#fname });
                Self::index_remove_block(&recv, &ident, val, &id_tok)
            })
            .collect();
        // Composite-index removal (#101): drop the deleted id's tuple key.
        let composite_removes: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| {
                        let cf = format_ident!("{}", c.name);
                        Self::index_value_expr(&c.field_type, quote! { record.#cf })
                    })
                    .collect();
                Self::composite_remove_block(&recv, ident, &parts, &id_tok)
            })
            .collect();

        Some(quote! {
            // Materialize the current live record; also gives the values to
            // re-append.  A missing / already-deleted id resolves to `None`.
            let record = match self.get(id) {
                Some(r) => r,
                None => return false,
            };
            // The pre-delete live row is still non-tombstoned, so a subscriber can
            // materialize what was deleted from it (the new tombstoned row cannot).
            let deleted_row = *self
                .id_to_row
                .get(&id)
                .expect("id present: get succeeded above");

            let row_index = self.row_count;

            // Durability boundary (#89): WAL-record the tombstoned version first.
            #wal_write

            // Re-append the current column values under a tombstone.
            #(#append_statements)*

            self.tombstones.append(true).expect("Failed to append tombstone");

            // Repoint the id at the tombstoned newest row so `get` reads absent.
            self.id_to_row.insert(id, row_index);
            self.row_count += 1;

            // Secondary-index maintenance (#90): drop the deleted id's value keys.
            #(#index_removes)*
            // Composite-index removal (#101).
            #(#composite_removes)*

            // #62: a Deleted event carries the pre-delete row so the record is
            // still materializable by a subscriber.
            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, deleted_row, forgedb_changefeed::ChangeKind::Deleted);
            }
            // Durable replication record (#82): the tombstoned physical row so a
            // follower reproduces the delete by position.
            #broker_record

            #maybe_checkpoint

            // Bound column storage (#92): this delete left a tombstoned dead row
            // behind — compact once enough accumulate.
            #maybe_compact

            true
        })
    }

    /// Generate the secondary-index probe methods on a model's storage (#90):
    /// `find_by_<field>` (live) + `find_by_<field>_at` (snapshot) for every
    /// `^index` / `&unique` field, plus `get_by_<field>` / `get_by_<field>_at`
    /// for `&unique` fields (at most one match).
    ///
    /// A probe is O(1) in the index (candidate ids), NOT a scan.  It then
    /// resolves each candidate through the version-aware read path — `get` (live
    /// newest version) or `get_at` (the snapshot's version) — so it never
    /// bypasses superseding-version (#66) / watermark (#56) resolution.  The
    /// `_at` probe additionally post-filters on the resolved value, so a
    /// candidate whose value changed *after* the snapshot is excluded.
    fn generate_index_lookups(model: &forgedb_parser::Model) -> TokenStream {
        Self::generate_index_probes(model, true)
    }

    /// Emit the secondary-index probe methods for a model's storage, factored so
    /// the writer storage gets the full set (live `find_by`/`get_by` + snapshot
    /// `_at`) while the read-only `*StorageReader` (#103) gets **only** the `_at`
    /// forms — a reader has no live `get`, so a "live" probe would be meaningless.
    /// Covers single-field indexes (#90/#100/#102) and composite indexes (#101).
    ///
    /// `include_live = true` → writer (has `get` + `get_at`); `false` → reader
    /// (has `get_at` only, index maps cloned at `reader()` time).
    fn generate_index_probes(model: &forgedb_parser::Model, include_live: bool) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let mut methods: Vec<TokenStream> = Vec::new();

        // --- Single-field probes (#90 + #100 FK + #102 nullable) --------------
        for f in Self::indexed_fields(model) {
            let ident = Self::index_field_ident(f);
            let fname = format_ident!("{}", f.name);
            let param_ty = Self::index_param_type(f);
            let find_fn = format_ident!("find_by_{}", f.name);
            let find_at_fn = format_ident!("find_by_{}_at", f.name);
            let params = quote! { value: #param_ty };
            let key_from_arg =
                Self::index_key_expr(Self::index_value_expr(&f.field_type, quote! { value }));
            let key_from_rec = Self::index_key_expr(Self::index_value_expr(
                &f.field_type,
                quote! { __rec.#fname },
            ));
            let subject = format!("`{}`.`{}`", model.name, f.name);
            let unique = if f.unique {
                Some((
                    format_ident!("get_by_{}", f.name),
                    format_ident!("get_by_{}_at", f.name),
                ))
            } else {
                None
            };
            methods.push(Self::emit_probe(
                &model_name,
                &ident,
                &find_fn,
                &find_at_fn,
                &params,
                &key_from_arg,
                &key_from_rec,
                include_live,
                unique.as_ref(),
                &subject,
            ));
        }

        // --- Composite probes (#101) — `find_by_<a>_and_<b>` (never unique) ---
        for (ident, comps) in Self::composite_indexes(model) {
            let find_fn = Self::composite_probe_ident(&comps);
            let find_at_fn = format_ident!("{}_at", find_fn);
            let params_list: Vec<_> = comps
                .iter()
                .map(|c| {
                    let pn = format_ident!("{}", c.name);
                    let pt = Self::index_param_type(c);
                    quote! { #pn: #pt }
                })
                .collect();
            let params = quote! { #(#params_list),* };
            let arg_keys: Vec<_> = comps
                .iter()
                .map(|c| {
                    let pn = format_ident!("{}", c.name);
                    Self::index_key_expr(Self::index_value_expr(&c.field_type, quote! { #pn }))
                })
                .collect();
            let rec_keys: Vec<_> = comps
                .iter()
                .map(|c| {
                    let cf = format_ident!("{}", c.name);
                    Self::index_key_expr(Self::index_value_expr(
                        &c.field_type,
                        quote! { __rec.#cf },
                    ))
                })
                .collect();
            let key_from_arg = Self::composite_key_build(&arg_keys);
            let key_from_rec = Self::composite_key_build(&rec_keys);
            let subject = format!(
                "`{}` composite ({})",
                model.name,
                comps.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            );
            methods.push(Self::emit_probe(
                &model_name,
                &ident,
                &find_fn,
                &find_at_fn,
                &params,
                &key_from_arg,
                &key_from_rec,
                include_live,
                None,
                &subject,
            ));
        }

        quote! { #(#methods)* }
    }

    /// Emit one index probe family: a `Vec` `find_by`/`find_by_at`, plus (for
    /// `&unique` single-field indexes) an `Option` `get_by`/`get_by_at`.  Live
    /// (`get`-based) forms are emitted only when `include_live` is set (writer);
    /// the `_at` (`get_at`-based) forms are always emitted (writer + reader).
    #[allow(clippy::too_many_arguments)]
    fn emit_probe(
        model_name: &proc_macro2::Ident,
        index_ident: &proc_macro2::Ident,
        find_fn: &proc_macro2::Ident,
        find_at_fn: &proc_macro2::Ident,
        params: &TokenStream,
        key_from_arg: &TokenStream,
        key_from_rec: &TokenStream,
        include_live: bool,
        unique: Option<&(proc_macro2::Ident, proc_macro2::Ident)>,
        subject: &str,
    ) -> TokenStream {
        let find_at_doc = format!(
            "Snapshot-scoped index probe for {subject} (#90/#56): O(1) candidate \
             lookup, each resolved through `get_at` (the version live as of `snap`) \
             and post-filtered so a candidate whose value changed after the snapshot \
             is excluded.  Honest limit: a row whose value changed *away* between \
             the snapshot and now is not in the (unversioned) index."
        );
        let mut m = TokenStream::new();

        if include_live {
            let find_doc = format!(
                "Index probe for {subject} (#90): every live match for the argument(s). \
                 O(1) candidate lookup + newest-version resolution, not a scan."
            );
            m.extend(quote! {
                #[doc = #find_doc]
                pub fn #find_fn(&self, #params) -> Vec<#model_name> {
                    let __k: String = { #key_from_arg };
                    let __ids = match self.#index_ident.get(&__k) {
                        Some(__s) => __s,
                        None => return Vec::new(),
                    };
                    __ids.iter().filter_map(|&__id| self.get(__id)).collect()
                }
            });
        }

        m.extend(quote! {
            #[doc = #find_at_doc]
            pub fn #find_at_fn(
                &self,
                snap: &forgedb_storage::Snapshot,
                #params
            ) -> Vec<#model_name> {
                let __k: String = { #key_from_arg };
                let __ids = match self.#index_ident.get(&__k) {
                    Some(__s) => __s,
                    None => return Vec::new(),
                };
                let mut __out = Vec::new();
                for &__id in __ids {
                    if let Some(__rec) = self.get_at(snap, __id) {
                        let __rk: String = { #key_from_rec };
                        if __rk == __k {
                            __out.push(__rec);
                        }
                    }
                }
                __out
            }
        });

        if let Some((get_fn, get_at_fn)) = unique {
            let get_at_doc = format!(
                "Snapshot-scoped unique index probe for {subject} (#90/#56): the \
                 at-most-one match as of `snap`."
            );
            if include_live {
                let get_doc = format!(
                    "Unique index probe for {subject} (#90): the at-most-one live match."
                );
                m.extend(quote! {
                    #[doc = #get_doc]
                    pub fn #get_fn(&self, #params) -> Option<#model_name> {
                        let __k: String = { #key_from_arg };
                        let __ids = self.#index_ident.get(&__k)?;
                        __ids.iter().find_map(|&__id| self.get(__id))
                    }
                });
            }
            m.extend(quote! {
                #[doc = #get_at_doc]
                pub fn #get_at_fn(
                    &self,
                    snap: &forgedb_storage::Snapshot,
                    #params
                ) -> Option<#model_name> {
                    let __k: String = { #key_from_arg };
                    let __ids = match self.#index_ident.get(&__k) {
                        Some(__s) => __s,
                        None => return None,
                    };
                    for &__id in __ids {
                        if let Some(__rec) = self.get_at(snap, __id) {
                            let __rk: String = { #key_from_rec };
                            if __rk == __k {
                                return Some(__rec);
                            }
                        }
                    }
                    None
                }
            });
        }

        m
    }

    /// Generate the body of the shared `read_at(row_index)` accessor.
    ///
    /// This is the read path *below* identity resolution: given a physical row
    /// index, check the tombstone and materialize the record.  Both `get(id)`
    /// (live index lookup) and the snapshot accessors `get_at`/`all_at` (#56,
    /// watermark-clamped) funnel through it, so there is exactly one place that
    /// knows a model's column layout.
    /// Emit the decode of ONE stored field at `row_index` from `receiver`, binding
    /// a `<field>_value` local whose type is EXACTLY the model struct field's type.
    /// Returns `None` for virtual/unstored fields (relations/components have no
    /// column).
    ///
    /// This is the single per-field decode path shared by `read_at` (full record)
    /// and the reopen index rebuild (`generate_rehydrate_logic`, which reads only
    /// the indexed-field columns) — one body, so the two can never drift, and the
    /// reopen path can fault in just the columns an index needs instead of the
    /// whole record.
    fn field_read_stmt(
        field: &forgedb_parser::Field,
        receiver: &TokenStream,
        row_index: &TokenStream,
    ) -> Option<(proc_macro2::Ident, TokenStream)> {
        let field_col_name = format_ident!("{}_col", field.name);
        let field_value_name = format_ident!("{}_value", field.name);

        if Self::is_enum_type(&field.field_type) {
            // Enum (#enum): a 1-byte discriminant column (2 bytes for nullable —
            // `[present, disc]`).  Decode via the generated `__from_u8`, which
            // hard-fails on an out-of-range byte.
            let enum_ident = Self::enum_type_ident(&field.field_type)
                .expect("enum field has an enum type");
            let stmt = if field.is_nullable() {
                quote! {
                    let #field_value_name = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        if bytes.first() == Some(&1u8) {
                            Some(#enum_ident::__from_u8(bytes[1]))
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        #enum_ident::__from_u8(bytes[0])
                    };
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_json_type(&field.field_type) {
            let stmt = if field.is_nullable() {
                // Decode the 1-byte presence tag written by insert
                // (0x01 = Some, anything else = None), then parse the JSON tail
                // back into a `serde_json::Value`.
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        if raw.as_bytes().first() == Some(&1u8) {
                            Some(serde_json::from_str(&raw[1..])
                                .expect("Failed to deserialize json"))
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        serde_json::from_str(&raw)
                            .expect("Failed to deserialize json")
                    };
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_string_type(&field.field_type) {
            let stmt = if field.is_nullable() {
                // Decode the 1-byte presence tag written by insert
                // (0x01 = Some, anything else = None).
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        if raw.as_bytes().first() == Some(&1u8) {
                            Some(raw[1..].to_string())
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = #receiver.#field_col_name.read_string(#row_index)
                        .expect("Failed to read string");
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_fixed_size_type(&field.field_type) {
            // Check if this is a complex type that needs byte conversion.
            // OptionalReference is treated like Nullable(Uuid) — stored as Option<Uuid> bytes.
            let needs_byte_conversion = matches!(
                &field.field_type,
                forgedb_parser::FieldType::Char(_)
                    | forgedb_parser::FieldType::FixedArray(_, _)
                    | forgedb_parser::FieldType::StructType(_)
                    | forgedb_parser::FieldType::OptionalStructType(_)
                    | forgedb_parser::FieldType::Nullable(_)
                    | forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(_),
                    )
            );

            let stmt = if needs_byte_conversion {
                // C3: use the actual stored type (Option<T> for nullable/optional) for the
                // type annotation and use ptr::read_unaligned to avoid UB on unaligned reads
                let stored_type = Self::stored_type_tokens(&field.field_type, field.is_nullable());
                quote! {
                    let #field_value_name: #stored_type = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        unsafe {
                            std::ptr::read_unaligned(bytes.as_ptr() as *const #stored_type)
                        }
                    };
                }
            } else {
                // For primitives, use typed method
                let read_method = Self::get_read_method(&field.field_type);

                // UUID and RequiredReference (FK scalar) both read raw [u8; 16]
                let is_uuid_like = matches!(
                    &field.field_type,
                    forgedb_parser::FieldType::Uuid
                        | forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(_),
                        )
                );
                // read_timestamp returns i64; the struct field is Timestamp
                let is_timestamp =
                    matches!(&field.field_type, forgedb_parser::FieldType::Timestamp);
                // Non-null decimal reads raw [u8; 16] then Decimal::deserialize.
                let is_decimal =
                    matches!(&field.field_type, forgedb_parser::FieldType::Decimal);

                if is_decimal {
                    quote! {
                        let #field_value_name = {
                            let bytes = #receiver.#field_col_name.read_uuid(#row_index)
                                .expect("Failed to read from column");
                            rust_decimal::Decimal::deserialize(bytes)
                        };
                    }
                } else if is_uuid_like {
                    quote! {
                        let #field_value_name = {
                            let bytes = #receiver.#field_col_name.#read_method(#row_index)
                                .expect("Failed to read from column");
                            Uuid::from_bytes(bytes)
                        };
                    }
                } else if is_timestamp {
                    quote! {
                        let #field_value_name = Timestamp::from(
                            #receiver.#field_col_name.#read_method(#row_index)
                                .expect("Failed to read from column"),
                        );
                    }
                } else {
                    quote! {
                        let #field_value_name = #receiver.#field_col_name.#read_method(#row_index)
                            .expect("Failed to read from column");
                    }
                }
            };
            Some((field_value_name, stmt))
        } else {
            None
        }
    }

    /// Render one struct field (`pub name: Type`) exactly as the model struct
    /// does, with the `utoipa` Timestamp `value_type` annotation and nullable
    /// wrapping.  Shared by the model struct and every projection struct (#113)
    /// so a projected field's type never drifts from the model's.
    fn model_struct_field(field: &forgedb_parser::Field) -> TokenStream {
        let field_name = format_ident!("{}", field.name);
        let field_type = Self::map_field_type_ident(&field.field_type);

        // forgedb_types::Timestamp is a newtype over i64 and does not implement
        // utoipa::ToSchema.  Annotate those fields so utoipa uses i64 for the
        // schema while the struct field keeps the semantic Timestamp type.
        //
        // `decimal` (rust_decimal::Decimal) likewise does not implement ToSchema,
        // and is serialized as a JSON *string* (precision-preserving, matching the
        // TS SDK's `string` type) via rust_decimal's `serde::str` module — so it
        // gets both a `#[schema(value_type = String)]` and a `#[serde(with = ...)]`.
        let (schema_attr, serde_attr) = if Self::is_timestamp_type(&field.field_type) {
            if field.is_nullable() {
                (quote! { #[schema(value_type = Option<i64>)] }, quote! {})
            } else {
                (quote! { #[schema(value_type = i64)] }, quote! {})
            }
        } else if Self::is_decimal_type(&field.field_type) {
            if field.is_nullable() {
                (
                    quote! { #[schema(value_type = Option<String>)] },
                    quote! { #[serde(with = "rust_decimal::serde::str_option")] },
                )
            } else {
                (
                    quote! { #[schema(value_type = String)] },
                    quote! { #[serde(with = "rust_decimal::serde::str")] },
                )
            }
        } else {
            (quote! {}, quote! {})
        };

        if field.is_nullable() {
            quote! { #schema_attr #serde_attr pub #field_name: Option<#field_type> }
        } else {
            quote! { #schema_attr #serde_attr pub #field_name: #field_type }
        }
    }

    /// The model's identity field (`id` or an `+auto` field), if any.  A
    /// projection always materializes it (#113, PM constraint 3).
    pub(crate) fn identity_field(model: &forgedb_parser::Model) -> Option<&forgedb_parser::Field> {
        model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
    }

    /// PascalCase a snake_case projection name (`list_row` → `ListRow`) for the
    /// generated struct suffix.
    pub(crate) fn projection_pascal(name: &str) -> String {
        name.split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    /// The ordered field set a projection materializes: the identity field
    /// first (always, PM constraint 3), then each selected field in declared
    /// order, de-duplicated (a projection may list `id` explicitly).
    pub(crate) fn projected_field_set<'a>(
        model: &'a forgedb_parser::Model,
        proj: &forgedb_parser::Projection,
    ) -> Vec<&'a forgedb_parser::Field> {
        let mut out: Vec<&forgedb_parser::Field> = Vec::new();
        if let Some(id_field) = Self::identity_field(model) {
            out.push(id_field);
        }
        for fname in &proj.fields {
            if out.iter().any(|f| &f.name == fname) {
                continue;
            }
            if let Some(f) = model.fields.iter().find(|f| &f.name == fname) {
                out.push(f);
            }
        }
        out
    }

    /// Emit, for a model's `@projection` directives (#113), the projection
    /// structs (top-level, alongside the model struct) and the read methods
    /// (inside the `*Storage` impl).  Returns `(structs, methods)`.  Empty when
    /// the model declares no projections.  `id_read` is the id-at-row expression
    /// (`generate_id_read_expr`) used to resolve newest-version-within-watermark
    /// for the snapshot `_at` variants; `None` for a model with no identity field.
    fn generate_projections(
        model: &forgedb_parser::Model,
        id_type: &TokenStream,
        id_read: Option<&TokenStream>,
    ) -> (TokenStream, TokenStream) {
        if model.projections.is_empty() {
            return (quote! {}, quote! {});
        }

        // Per-model snapshot row-resolver helpers, emitted ONCE and reused by
        // every projection's `_at` variant, so the newest-version scan is not
        // duplicated per projection.  They mirror `get_at`/`all_at`'s resolution
        // but return row indices (leaving the final read to the projected
        // decoder).  Guarded on identity presence exactly like the accessors.
        let resolvers = match id_read {
            Some(id_read) => quote! {
                /// Resolve the newest committed row of `id` as of `snap`
                /// (#113 projection `_at` support; same logic as `get_at`).
                fn __proj_resolve_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<usize> {
                    let watermark = snap.watermark();
                    let mut newest: Option<usize> = None;
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        if row_id == id {
                            newest = Some(row_index);
                        }
                    }
                    newest
                }

                /// Row indices of every live record as of `snap` — the newest
                /// version per id in row order (#113; same logic as `all_at`).
                fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
                    let watermark = snap.watermark();
                    let mut newest: HashMap<#id_type, usize> = HashMap::new();
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        newest.insert(row_id, row_index);
                    }
                    let mut rows = Vec::new();
                    for row_index in 0..watermark {
                        let row_id = #id_read;
                        if newest.get(&row_id) == Some(&row_index) {
                            rows.push(row_index);
                        }
                    }
                    rows
                }
            },
            None => quote! {
                /// Resolve `id`'s row clamped to `snap` (no-mutation model).
                fn __proj_resolve_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<usize> {
                    let row_index = *self.id_to_row.get(&id)?;
                    if !snap.visible(row_index) { return None; }
                    Some(row_index)
                }

                /// Every visible row as of `snap` (no-mutation model).
                fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
                    (0..snap.watermark()).collect()
                }
            },
        };

        let mut structs = Vec::new();
        let mut methods = Vec::new();

        for proj in &model.projections {
            let proj_ident = format_ident!("{}{}", model.name, Self::projection_pascal(&proj.name));
            let fields = Self::projected_field_set(model, proj);
            let struct_field_defs: Vec<_> = fields.iter().map(|f| Self::model_struct_field(f)).collect();

            let read_at_name = format_ident!("read_{}_at", proj.name);
            let get_name = format_ident!("get_{}", proj.name);
            let all_name = format_ident!("all_{}", proj.name);
            let get_at_name = format_ident!("get_{}_at", proj.name);
            let all_at_name = format_ident!("all_{}_at", proj.name);

            let read_body = Self::generate_row_read_body(&proj_ident, &fields);
            let doc = format!(
                "Projection `{}` of `{}` (#113): materializes only PK + \
                 declared columns, leaving unselected columns unread.",
                proj.name, model.name
            );

            structs.push(quote! {
                #[doc = #doc]
                #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
                pub struct #proj_ident {
                    #(#struct_field_defs),*
                }
            });

            methods.push(quote! {
                /// Read the projection at a physical row index (subset decode —
                /// only the projected columns are touched).
                pub fn #read_at_name(&self, row_index: usize) -> Option<#proj_ident> {
                    #read_body
                }

                /// Fetch the projection for `id` from the live newest version.
                pub fn #get_name(&self, id: #id_type) -> Option<#proj_ident> {
                    let row_index = *self.id_to_row.get(&id)?;
                    self.#read_at_name(row_index)
                }

                /// Every live record as this projection (order follows the id map).
                pub fn #all_name(&self) -> Vec<#proj_ident> {
                    let mut records = Vec::new();
                    for id in self.id_to_row.keys() {
                        if let Some(record) = self.#get_name(*id) {
                            records.push(record);
                        }
                    }
                    records
                }

                /// Snapshot-scoped projection point read (#56 + #113).
                pub fn #get_at_name(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#proj_ident> {
                    let row_index = self.__proj_resolve_at(snap, id)?;
                    self.#read_at_name(row_index)
                }

                /// Snapshot-scoped projection scan (#56 + #113).
                pub fn #all_at_name(&self, snap: &forgedb_storage::Snapshot) -> Vec<#proj_ident> {
                    let mut records = Vec::new();
                    for row_index in self.__proj_live_rows_at(snap) {
                        if let Some(record) = self.#read_at_name(row_index) {
                            records.push(record);
                        }
                    }
                    records
                }
            });
        }

        let structs = quote! { #(#structs)* };
        let methods = quote! {
            #resolvers
            #(#methods)*
        };
        (structs, methods)
    }

    /// Emit the body of a read-by-`row_index` over a chosen `fields` subset,
    /// constructing `struct_ident`.  Shared by `read_at` (all model fields) and
    /// projection reads (PK + selected) so there is ONE decode path — every
    /// field decode is the same `field_read_stmt` branch (#113, PM constraint 1).
    /// Only the columns of `fields` are touched, which is what lets a projection
    /// skip the wasm lazy fault-in of unselected columns.
    fn generate_row_read_body(
        struct_ident: &proc_macro2::Ident,
        fields: &[&forgedb_parser::Field],
    ) -> TokenStream {
        let mut read_statements = Vec::new();
        let mut field_values = Vec::new();
        let recv = quote! { self };
        let row = quote! { row_index };

        for field in fields {
            let field_name = format_ident!("{}", field.name);
            match Self::field_read_stmt(field, &recv, &row) {
                Some((value_ident, stmt)) => {
                    read_statements.push(stmt);
                    // C1: only push to field_values when a binding was emitted.
                    field_values.push(quote! { #field_name: #value_ident });
                }
                None => {
                    // C1: Relation/component fields have no storage column.
                    // Provide a type-appropriate default so the struct literal compiles.
                    // (Projections never hit this branch — validation rejects
                    // non-projectable fields — so it only serves the full record.)
                    let default_val = Self::default_for_unstored_field(&field.field_type);
                    field_values.push(quote! { #field_name: #default_val });
                }
            }
        }

        quote! {
            // Read receives an already-resolved physical row index.
            // Check if deleted
            if self.tombstones.is_deleted(row_index).unwrap_or(true) {
                return None;
            }

            // Read the selected fields
            #(#read_statements)*

            // Construct the record
            Some(#struct_ident {
                #(#field_values),*
            })
        }
    }

    fn generate_read_at_logic(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let fields: Vec<&forgedb_parser::Field> = model.fields.iter().collect();
        Self::generate_row_read_body(&model_name, &fields)
    }

    /// Return the Rust type tokens used for raw byte storage read/write (C3).
    /// For `OptionalStructType` and `Nullable` the stored type is `Option<Inner>`.
    fn stored_type_tokens(
        field_type: &forgedb_parser::FieldType,
        is_nullable: bool,
    ) -> TokenStream {
        let base = Self::map_field_type_ident(field_type);
        if is_nullable {
            quote! { Option<#base> }
        } else {
            base
        }
    }

    /// Generate a sensible default value for fields that have no storage column
    /// (virtual relations, components).  Returned tokens are placed directly in the
    /// struct literal, so they must be valid expressions of the correct type.
    ///
    /// NOTE: `RequiredReference` and `OptionalReference` are FK scalars — they now
    /// have storage columns and never reach this function.  Only `OneToMany` /
    /// `ManyToMany` (virtual, no column) and `Component` (no column) are handled here.
    fn default_for_unstored_field(field_type: &forgedb_parser::FieldType) -> TokenStream {
        match field_type {
            // Virtual relation fields (no storage column) map to ()
            forgedb_parser::FieldType::Relation(_) => quote! { () },
            // Component fields map to String
            forgedb_parser::FieldType::Component(_) => quote! { Default::default() },
            _ => quote! { Default::default() },
        }
    }

    /// Check if a field type is fixed-size
    fn is_fixed_size_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::U32
            | forgedb_parser::FieldType::U64
            | forgedb_parser::FieldType::I32
            | forgedb_parser::FieldType::I64
            | forgedb_parser::FieldType::F64
            | forgedb_parser::FieldType::Bool
            | forgedb_parser::FieldType::Uuid
            | forgedb_parser::FieldType::Timestamp
            // `decimal` is a fixed 16-byte column (rust_decimal::Decimal::serialize),
            // stored exactly like Uuid.
            | forgedb_parser::FieldType::Decimal
            // `enum` is a fixed 1-byte discriminant column (2 bytes for nullable);
            // the enum-specific append/read branches (gated on `is_enum_type`) run
            // BEFORE the generic fixed path everywhere, so this only makes column
            // layout / iteration agree that an enum field HAS a column.
            | forgedb_parser::FieldType::Enum(_)
            | forgedb_parser::FieldType::Char(_)
            | forgedb_parser::FieldType::FixedArray(_, _)
            | forgedb_parser::FieldType::StructType(_)
            | forgedb_parser::FieldType::OptionalStructType(_) => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_fixed_size_type(inner),
            // FK scalar references are fixed-size UUID columns (16 bytes for required,
            // size_of::<Option<Uuid>>() for optional).  This is the load-bearing entry
            // point that causes generate_storage_fields, generate_storage_inits,
            // generate_insert_logic, and generate_get_logic to all agree that FK fields
            // have a column — keeping column_index consistent across all four helpers.
            forgedb_parser::FieldType::Relation(
                forgedb_parser::RelationType::RequiredReference(_)
                | forgedb_parser::RelationType::OptionalReference(_),
            ) => true,
            _ => false,
        }
    }

    /// Check if a field type is a string (variable-length).
    ///
    /// This is the *semantic* "String" predicate — it gates the string-specific
    /// encode/decode body (`append_string`/`read_string` with UTF-8 conversion)
    /// and the string validation directives (`@length`/`@email`/…).  For
    /// column-layout classification (does this field get a `VariableColumn`?) use
    /// `is_variable_column_type`, which also covers `json`.
    fn is_string_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::String => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_string_type(inner),
            _ => false,
        }
    }

    /// Check if a field type is `json` (variable-length, typed `serde_json::Value`).
    ///
    /// `json` rides the exact same variable-column storage path as `String`
    /// (a `VariableColumn`), but its encode/decode body serializes via
    /// `serde_json::to_string`/`from_str` (JSON is always valid UTF-8, so the
    /// serialized form is stored through the same `append_string`/`read_string`
    /// as a plain string — no new storage machinery, no new dependency).
    fn is_json_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Json => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_json_type(inner),
            _ => false,
        }
    }

    /// Check if a field type is (or wraps) `decimal` — an exact fixed-point value
    /// typed `rust_decimal::Decimal`.
    ///
    /// `decimal` rides the FIXED 16-byte column path (like `Uuid`), encoded via
    /// `Decimal::serialize() -> [u8; 16]` and decoded via `Decimal::deserialize`.
    /// It needs its own append/read body (raw 16-byte column, but a
    /// serialize/deserialize round-trip rather than uuid's `as_bytes`/`from_bytes`),
    /// so this predicate special-cases it out of the generic fixed path — the same
    /// way `is_uuid_like`/`is_timestamp` do.
    fn is_decimal_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Decimal => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_decimal_type(inner),
            _ => false,
        }
    }

    /// Check if a field type is (or wraps) a user-declared enum (#enum).
    ///
    /// An enum is stored as a fixed 1-byte `u8` discriminant column (2 bytes for
    /// nullable enum, `[present, disc]`), encoded via the generated `__to_u8`/
    /// `__from_u8` codec.  Like `is_decimal_type` it special-cases enum out of the
    /// generic fixed byte-conversion path (it needs an explicit variant match, not
    /// `read_unaligned`), so this predicate gates that branch.
    fn is_enum_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Enum(_) => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_enum_type(inner),
            _ => false,
        }
    }

    /// The generated enum type ident an enum field maps to (for `__from_u8`), if any.
    fn enum_type_ident(field_type: &forgedb_parser::FieldType) -> Option<proc_macro2::Ident> {
        field_type
            .enum_name()
            .map(|n| format_ident!("{}", n))
    }

    /// Whether a field occupies a variable-length column (`String` OR `json`).
    /// This is the classification predicate for storage layout / column
    /// iteration / projectability — everywhere the question is "does this field
    /// get a `VariableColumn`?" (as opposed to the string-semantic checks that
    /// stay on `is_string_type`).
    fn is_variable_column_type(field_type: &forgedb_parser::FieldType) -> bool {
        Self::is_string_type(field_type) || Self::is_json_type(field_type)
    }

    /// Check if a field type is (or wraps) a `Timestamp`.
    ///
    /// `forgedb_types::Timestamp` is a newtype over `i64` but does not implement
    /// `utoipa::ToSchema`.  The generator uses this to decide whether to emit a
    /// `#[schema(value_type = …)]` annotation and to use `i64::from` / `Timestamp::from`
    /// in the insert / get helpers respectively.
    fn is_timestamp_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Timestamp => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_timestamp_type(inner),
            _ => false,
        }
    }

    /// Get the type name for storage paths
    fn type_name(field_type: &forgedb_parser::FieldType) -> &'static str {
        match field_type {
            forgedb_parser::FieldType::U32 => "u32",
            forgedb_parser::FieldType::U64 => "u64",
            forgedb_parser::FieldType::I32 => "i32",
            forgedb_parser::FieldType::I64 => "i64",
            forgedb_parser::FieldType::F64 => "f64",
            forgedb_parser::FieldType::Bool => "bool",
            forgedb_parser::FieldType::Uuid => "uuid",
            forgedb_parser::FieldType::Timestamp => "timestamp",
            // decimal persists as a raw [u8; 16] (Decimal::serialize) — reuses the
            // uuid column file-path label, same 16-byte fixed column on disk.
            forgedb_parser::FieldType::Decimal => "decimal",
            // enum persists as a raw 1-byte discriminant (append_bytes/read_bytes).
            forgedb_parser::FieldType::Enum(_) => "enum",
            forgedb_parser::FieldType::Char(_) => "bytes",
            forgedb_parser::FieldType::FixedArray(_, _) => "bytes",
            forgedb_parser::FieldType::StructType(_) => "bytes",
            forgedb_parser::FieldType::OptionalStructType(_) => "bytes",
            forgedb_parser::FieldType::Nullable(inner) => Self::type_name(inner),
            // Both FK reference types store UUID bytes — required as 16 raw bytes,
            // optional as Option<Uuid> bytes; the path label is "uuid" for both.
            forgedb_parser::FieldType::Relation(
                forgedb_parser::RelationType::RequiredReference(_)
                | forgedb_parser::RelationType::OptionalReference(_),
            ) => "uuid",
            _ => "unknown",
        }
    }

    /// Get the append method name for a field type
    fn get_append_method(field_type: &forgedb_parser::FieldType) -> proc_macro2::Ident {
        format_ident!("append_{}", Self::type_name(field_type))
    }

    /// Get the read method name for a field type
    fn get_read_method(field_type: &forgedb_parser::FieldType) -> proc_macro2::Ident {
        format_ident!("read_{}", Self::type_name(field_type))
    }

    /// Generate reopen/rehydration logic for a model storage's `new()` (#65).
    ///
    /// Sets `row_count` from the tombstone-file length (the authoritative
    /// row-count anchor — 1 byte/row, appended last per insert) and rebuilds
    /// `id_to_row` by reading the id column for every committed row.  Operates on
    /// a local `db` binding (the just-constructed storage).  Emitted for the id
    /// field only — reconstructing the identity map is all reopen needs; the
    /// remaining columns are read on demand by `get`.
    fn generate_rehydrate_logic(model: &forgedb_parser::Model) -> TokenStream {
        Self::generate_rehydrate_body(model, &quote! { db })
    }

    /// The shared reopen-rehydration body (#65), parameterized over the receiver
    /// binding so it can rebuild the in-memory maps of either a just-constructed
    /// storage (`db`, used by `new_at`) or `self` (used by the transaction
    /// `__reindex_committed`, MVCC Tier 1).  Sets `<recv>.row_count` from the
    /// tombstone anchor and rebuilds `id_to_row` + the secondary indexes from the
    /// committed prefix via the SAME narrow per-field decode path everywhere.
    fn generate_rehydrate_body(
        model: &forgedb_parser::Model,
        recv: &TokenStream,
    ) -> TokenStream {
        // Mirror `get`'s per-type read of the id field, but at loop index `i`, via
        // the shared id-read helper.  The overwriting insert in ascending physical
        // order yields last-occurrence-wins, which is exactly the superseding-version
        // resolution the mutation surface (#66) needs on reopen: the newest version
        // is the highest physical index for an id, so the reopened index resolves it.
        let i = format_ident!("i");
        let id_scan = match Self::generate_id_read_expr(model, recv, &i) {
            Some(read_expr) => quote! {
                for i in 0..n {
                    let id = #read_expr;
                    #recv.id_to_row.insert(id, i);
                }
            },
            // No id/auto field: nothing to index (such a model cannot `insert`
            // either — `insert` reads `record.id`).  Row count still recovers.
            None => quote! {},
        };

        // Secondary-index rebuild (#90): fold into the SAME reopen scan that
        // rebuilds `id_to_row`.  Runs AFTER `id_scan` has resolved each id to its
        // newest physical row (last-write-wins), so the index keys only committed,
        // live (non-tombstoned) values.
        //
        // Reads ONLY the indexed-field columns at each id's newest physical row —
        // NOT the full record via `db.get()`.  `get()` would decode every column,
        // which on the wasm lazy-source backend faults every column in at reopen
        // (defeating partial hydrate); native pays the same over-read as file I/O.
        // We resolve the row from `id_to_row` (already populated by `id_scan`),
        // gate on the tombstone exactly as `read_at` does, then decode just the
        // union of {single-index fields} ∪ {composite components} via the shared
        // `field_read_stmt` decode path (same body `read_at` uses — no drift).
        // Collect ids first to avoid holding an immutable borrow of `id_to_row`
        // across the index mutation.
        let id_tok = quote! { __id };
        let index_rebuild = {
            let indexed = Self::indexed_fields(model);
            let composites = Self::composite_indexes(model);
            if indexed.is_empty() && composites.is_empty() {
                quote! {}
            } else {
                let id_type = Self::id_type_tokens(model);

                // The set of fields whose columns the rebuild must decode: every
                // single-index field plus every composite component, de-duplicated
                // by name (a field can be both).  Each becomes a `<field>_value`
                // local via the shared per-field decode path.
                let mut read_fields: Vec<&forgedb_parser::Field> = Vec::new();
                for f in indexed.iter().copied() {
                    read_fields.push(f);
                }
                for (_, comps) in &composites {
                    for c in comps.iter().copied() {
                        if !read_fields.iter().any(|g| g.name == c.name) {
                            read_fields.push(c);
                        }
                    }
                }
                let row_tok = quote! { __row };
                let field_reads: Vec<_> = read_fields
                    .iter()
                    .filter_map(|f| {
                        Self::field_read_stmt(f, recv, &row_tok).map(|(_, stmt)| stmt)
                    })
                    .collect();

                // Single-field index adds (#90 + #100/#102 via `indexed_fields`),
                // keyed off the just-decoded `<field>_value` locals.
                let mut adds: Vec<_> = indexed
                    .iter()
                    .map(|f| {
                        let ident = Self::index_field_ident(f);
                        let val = format_ident!("{}_value", f.name);
                        let val = Self::index_value_expr(&f.field_type, quote! { #val });
                        Self::index_add_block(recv, &ident, val, &id_tok)
                    })
                    .collect();
                // Composite index adds (#101), folded into the same scan.
                adds.extend(composites.iter().map(|(ident, comps)| {
                    let parts: Vec<_> = comps
                        .iter()
                        .map(|c| {
                            let val = format_ident!("{}_value", c.name);
                            Self::index_value_expr(&c.field_type, quote! { #val })
                        })
                        .collect();
                    Self::composite_add_block(recv, ident, &parts, &id_tok)
                }));
                quote! {
                    let __ids: Vec<#id_type> = #recv.id_to_row.keys().copied().collect();
                    for __id in __ids {
                        let __row = match #recv.id_to_row.get(&__id) {
                            Some(__r) => *__r,
                            None => continue,
                        };
                        // Skip deleted rows — the same tombstone gate `read_at`
                        // (and therefore `get`) applies, so the index keys only
                        // live values.
                        if #recv.tombstones.is_deleted(__row).unwrap_or(true) {
                            continue;
                        }
                        #(#field_reads)*
                        #(#adds)*
                    }
                }
            }
        };

        quote! {
            let n = #recv.tombstones.len();
            #recv.row_count = n;
            #id_scan
            #index_rebuild
        }
    }

    /// Generate the main database struct
    fn generate_database(schema: &Schema) -> Result<TokenStream> {
        // Generate field definitions for each model storage
        let db_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                let storage_type = format_ident!("{}Storage", model.name);
                quote! { pub #field_name: #storage_type }
            })
            .collect();

        // M2M junction storage: one persisted junction table per many-to-many
        // pair, added as a Database field + initializer.
        let m2m = Self::valid_m2m(schema);
        let junction_fields: Vec<_> = m2m
            .iter()
            .map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! { pub #field: #ty }
            })
            .collect();

        // `new()` builds each collection, then hands it a clone of the shared
        // change feed (#62 Direction A) before assembling `Self`.  Every collection
        // publishes into the same feed, so one WS subscriber sees all inserts/links.
        let attach_stmts: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let ty = format_ident!("{}Storage", model.name);
                quote! {
                    let mut #field = #ty::new();
                    #field.attach_changefeed(changefeed.clone());
                }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! {
                    let mut #field = #ty::new();
                    #field.attach_changefeed(changefeed.clone());
                }
            }))
            .collect();
        // Root-aware variant (#59): identical to `attach_stmts` but each
        // collection opens under `root` (the process's tenant data dir) via
        // `new_at(&root)` instead of the CWD-relative `new()`.  `Database::new()`
        // and `Database::open_at(root)` share everything else.
        let attach_stmts_at: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let ty = format_ident!("{}Storage", model.name);
                quote! {
                    let mut #field = #ty::new_at(&root);
                    #field.attach_changefeed(changefeed.clone());
                    #field.attach_broker(broker.clone());
                }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! {
                    let mut #field = #ty::new_at(&root);
                    #field.attach_changefeed(changefeed.clone());
                    #field.attach_broker(broker.clone());
                }
            }))
            .collect();
        let field_idents: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
            .chain(m2m.iter().map(Self::junction_field_ident))
            .collect();

        // Open-time format-version guard (#74 Phase 1).  For every collection that
        // has already been written (its `manifest.json` exists), load exactly the
        // opaque `format_version` integer and refuse to open the dir when it does
        // not match this binary's codegen-baked `EXPECTED_FORMAT_VERSION`.  This is
        // the fail-fast that turns a stale data dir (written under an older schema,
        // now opened by regenerated code) from a SILENT byte mis-decode into a
        // clear panic pointing at the migration bin.  Red line DV-6: it reads one
        // integer and refuses — it NEVER inspects column names/types to self-heal.
        // A collection with no manifest yet (a fresh, never-written dir) is skipped
        // — there is nothing stale to guard against.
        let version_guard_stmts: Vec<_> = schema
            .models
            .iter()
            .map(|model| format!("{}/manifest.json", Self::to_snake_case(&model.name)))
            .chain(m2m.iter().map(|m| {
                format!(
                    "{}_{}_link/manifest.json",
                    Self::to_snake_case(&m.model1),
                    Self::to_snake_case(&m.model2)
                )
            }))
            .map(|manifest_rel| {
                quote! {
                    {
                        let __mf = root.join(#manifest_rel);
                        if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                            if __m.format_version != EXPECTED_FORMAT_VERSION {
                                panic!(
                                    "ForgeDB: data dir at {} is on-disk format v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                                    __mf.display(),
                                    __m.format_version,
                                    EXPECTED_FORMAT_VERSION,
                                );
                            }
                        }
                    }
                }
            })
            .collect();

        // Model-only field idents (#92 Phase 4): `Database::compact()` compacts
        // model storages, not junctions — a junction is an append-only link table
        // with no update/delete, so it accumulates no dead versions to reclaim.
        let model_field_idents: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
            .collect();

        // DatabaseSnapshot (#56, Direction A): one row-count watermark per model
        // and per junction, captured together by `Database::snapshot()`.  Because
        // storage is append-only, this set of watermarks defines a database-wide
        // consistent read view — every `*_at` accessor clamps to it.
        let snapshot_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { pub #field_name: forgedb_storage::Snapshot }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                quote! { pub #field: forgedb_storage::Snapshot }
            }))
            .collect();
        let snapshot_inits: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { #field_name: self.#field_name.snapshot() }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                quote! { #field: self.#field.snapshot() }
            }))
            .collect();

        // DatabaseReader (#56, Direction B): a read-only bundle of every model's
        // and junction's shared-fd reader handle.  A reader thread holds one of
        // these plus a `DatabaseSnapshot` captured on the single writer, and reads
        // lock-free — never blocking, and never blocked by, the writer's appends.
        // The `DatabaseSnapshot` capture is routed through the writer (see
        // `Database::snapshot()`), so it is cross-model atomic by construction.
        let reader_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                let reader_type = format_ident!("{}StorageReader", model.name);
                quote! { pub #field_name: #reader_type }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = format_ident!("{}Reader", Self::junction_struct_ident(m));
                quote! { pub #field: #ty }
            }))
            .collect();
        let reader_inits: Vec<_> = field_idents
            .iter()
            .map(|field| quote! { #field: self.#field.reader() })
            .collect();

        // Replication apply dispatch (#110 Milestone C): map a frame's OPAQUE
        // model tag to the matching collection's follower `apply`.  A closed,
        // generated match — the follower reads no `.forge` at runtime; it only
        // routes by the string tag the server stamped and lets generated code
        // materialize the typed record.  Id-bearing models dispatch to their
        // per-model `apply`; a junction tag decodes the 32-byte opaque pair and
        // re-links.  (Guard: this method must never `match` on a *decoded field*.)
        let apply_model_arms: Vec<_> = schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let name = model.name.as_str();
                quote! { #name => self.#field.apply(ev.kind, &ev.bytes), }
            })
            .collect();
        let apply_junction_arms: Vec<_> = m2m
            .iter()
            .map(|m| {
                let field = Self::junction_field_ident(m);
                let base = format!(
                    "{}_{}_link",
                    Self::to_snake_case(&m.model1),
                    Self::to_snake_case(&m.model2)
                );
                quote! {
                    #base => {
                        // Opaque 32-byte pair: left uuid ++ right uuid, exactly as
                        // the junction broker recorded it.  The follower re-links
                        // through the same generated `link` (broker `None`, inert).
                        if ev.bytes.len() == 32 {
                            let mut __l = [0u8; 16];
                            __l.copy_from_slice(&ev.bytes[0..16]);
                            let mut __r = [0u8; 16];
                            __r.copy_from_slice(&ev.bytes[16..32]);
                            self.#field.link(Uuid::from_bytes(__l), Uuid::from_bytes(__r));
                        }
                        Ok(())
                    }
                }
            })
            .collect();
        // Junctions flush via `checkpoint` (they have no WAL to truncate — a #89
        // boundary — so it is just an fsync of both id columns) in `commit`.
        let junction_field_idents: Vec<_> = m2m.iter().map(Self::junction_field_ident).collect();

        // Journal-driven recovery arms (MVCC Tier 1, #83, M1b): map a committed-
        // txn journal record's OPAQUE model tag to that collection's
        // `__recover_to_committed`, truncating any aborted ahead-rows down to the
        // journalled committed length.  Only id-bearing models are transactable
        // and so appear in a journal record.  A closed generated match on the
        // opaque tag — no `.forge` read at runtime.
        let journal_recover_arms: Vec<_> = schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let name = model.name.as_str();
                quote! {
                    #name => __db.#field.__recover_to_committed(__len as usize),
                }
            })
            .collect();

        let tokens = quote! {
            /// Main database struct
            pub struct Database {
                #(#db_fields,)*
                #(#junction_fields,)*
                /// Shared change feed (#62 Direction A): every collection emits a
                /// field-blind `(model, row_index)` signal here on insert/link.
                /// The generated WS endpoint subscribes to it; standalone callers
                /// can ignore it.
                pub changefeed: forgedb_changefeed::ChangeFeed,
                /// Durable replication broker (#82 Direction C): `Some` on the
                /// per-tenant `open_at` path (durable log under the data root),
                /// `None` on the lock-free `new()` convenience path.  Shared across
                /// every collection behind an `Arc<Mutex<..>>` so one global offset
                /// sequences all changes; the generated `/replicate` WS endpoint
                /// streams from it to a resumable follower (#110).
                pub broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                /// Single-writer guard (#89): `open_at` holds an exclusive advisory
                /// lock on the data dir, so a second writer process refuses rather
                /// than corrupting (v1 single-writer-per-process contract — this
                /// only acquires-or-refuses; it is not a lease broker).  `None` on
                /// the lock-free `new()` convenience path (standalone / tests).
                _lock: Option<forgedb_storage::DirLock>,
                /// Atomic multi-model commit journal (MVCC Tier 1, #83, M1b).  A
                /// shared, CRC-framed, append-only log at the data-dir root holding
                /// one opaque record per committed transaction: the touched
                /// collections' post-commit row-counts `[(model_tag, post_len)]`.
                /// Framed + fsynced by `forgedb-wal`'s published opaque `Raw` path
                /// (no new substrate) — the single fsync that flips a transaction
                /// from "aborted" to "committed" atomically across all its models.
                /// `Some` on the `open_at` path (durable dir); `None` on the
                /// lock-free `new()` convenience path, whose transactions are
                /// in-process-atomic but not crash-atomic across models.
                _txn_journal: Option<forgedb_wal::WalManager>,
                /// Commit sequencer (MVCC Tier 2, #83): monotonic LSN source +
                /// first-committer-wins conflict map.  In-memory, ephemeral;
                /// rebuilds empty on restart (no in-flight txns survive a process
                /// restart).  Shared behind `Arc<Mutex>` so prepare can be
                /// concurrent and commit is serialized.  Seeded from the broker
                /// watermark on `open_at` so the commit-LSN and broker offset form
                /// one unified monotonic sequence.
                seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
            }

            /// A read-only, lock-free view of the whole database (#56 Direction B).
            /// One per reader thread; each field shares the writer's column files
            /// via independent descriptors.  Read through a `DatabaseSnapshot`
            /// captured on the writer (`Database::snapshot()`) for a cross-model
            /// consistent, torn-row-free view while the single writer keeps
            /// appending.  Exposes only `&self` reads — it cannot mutate.
            pub struct DatabaseReader {
                #(#reader_fields,)*
            }

            /// A database-wide read snapshot (#56, Direction A): a row-count
            /// watermark for every model and junction, captured atomically by
            /// `Database::snapshot()`.  Pass a field to that collection's
            /// `*_at` accessors for reads consistent as of the capture instant.
            /// Rows appended after capture are invisible; concurrent writers
            /// cannot make a reader observe a torn row.
            pub struct DatabaseSnapshot {
                #(#snapshot_fields,)*
            }

            impl Database {
                pub fn new() -> Self {
                    let changefeed = forgedb_changefeed::ChangeFeed::new(1024);
                    #(#attach_stmts)*
                    Self {
                        #(#field_idents,)*
                        changefeed,
                        // Standalone / test path: no durable replication broker.
                        broker: None,
                        _lock: None,
                        // No durable dir → no crash-atomic multi-model commit
                        // journal (transactions here are in-process-atomic only).
                        _txn_journal: None,
                        // Sequencer seeded at 0 on the standalone path (no broker
                        // watermark to seed from; commit LSNs start at Lsn(1)).
                        seq: std::sync::Arc::new(std::sync::Mutex::new(
                            forgedb_txn::CommitSequencer::new(0),
                        )),
                    }
                }

                /// Open the whole database rooted at `root` (#59): every model and
                /// junction collection is opened under `root` via `new_at`, so a
                /// single generated binary serves whichever data dir it is handed.
                /// A per-tenant process passes that tenant's dir (e.g.
                /// `<data>/<tenant>`), making tenant isolation the filesystem
                /// boundary — no query logic, no `.forge` read at runtime.  `new()`
                /// is exactly `open_at(".")`.
                pub fn open_at(root: std::path::PathBuf) -> Self {
                    // Single-writer guard (#89): acquire the data-dir lock BEFORE
                    // opening any column/WAL file, so a second writer refuses up
                    // front instead of racing.  Held for the life of the Database.
                    let _lock = Some(
                        forgedb_storage::DirLock::acquire(&root).expect(
                            "another writer already holds this data dir \
                             (ForgeDB is single-writer-per-process, #89)",
                        ),
                    );
                    Self::__open_with_lock(root, _lock)
                }

                /// Lock-free-capable open core shared by the standalone path
                /// (`open_at`, `_lock = Some`) and the Tier-3 coordinated path
                /// (`connect`, `_lock = None`).  MVCC spec T3-5: a coordinated
                /// client does NOT take the #89 `DirLock` — the coordinator holds
                /// it on every client's behalf, and write mutual-exclusion among
                /// coordinated clients is the coordinator's serialized turn-grant.
                /// There is deliberately NO public lock-free open: `None` is
                /// reachable only through `connect`, which first establishes a live
                /// coordinator turn-channel, so a lock-free open never occurs
                /// without a coordinator actually serializing writers (the PM
                /// re-gate's binding constraint — no unserialized lock-free write).
                fn __open_with_lock(
                    root: std::path::PathBuf,
                    _lock: Option<forgedb_storage::DirLock>,
                ) -> Self {
                    // Format-version guard (#74 Phase 1): refuse a data dir whose
                    // stamped `format_version` differs from this binary's baked
                    // `EXPECTED_FORMAT_VERSION`, BEFORE opening any column/WAL file.
                    // Reads one opaque integer per existing manifest and fails fast
                    // on mismatch — never reshapes (DV-6).  Skipped for a fresh dir.
                    #(#version_guard_stmts)*
                    let changefeed = forgedb_changefeed::ChangeFeed::new(1024);
                    // Durable replication broker (#82 Direction C): one offset-addressed
                    // log per tenant, under the data root, shared across all collections
                    // so a single global offset sequences every change.  If it cannot be
                    // opened the server still runs — replication is simply unavailable
                    // (the `/replicate` endpoint reports it), never blocking writes.
                    let broker = match forgedb_changefeed::durable::DurableBroker::open(
                        root.join("_replication.log"),
                        forgedb_changefeed::durable::FsyncPolicy::Always,
                        1024,
                    ) {
                        Ok(b) => Some(std::sync::Arc::new(std::sync::Mutex::new(b))),
                        Err(_) => None,
                    };
                    // MVCC Tier 2 (#83): seed the commit sequencer from the broker
                    // watermark so the commit-LSN and the broker's global offset form
                    // one unified monotonic sequence.  The broker watermark is the
                    // highest offset recorded so far; seeding there means the first
                    // Tier-2 commit LSN is broker_watermark + 1, guaranteeing the
                    // two sequences never collide.  Falls back to 0 when there is no
                    // broker (the `Err(_)` branch above), which is correct: the
                    // standalone/test path has no broker to unify with.
                    let __seq_start = match &broker {
                        Some(b) => b.lock().map(|b| b.watermark()).unwrap_or(0),
                        None => 0,
                    };
                    let seq = std::sync::Arc::new(std::sync::Mutex::new(
                        forgedb_txn::CommitSequencer::new(__seq_start),
                    ));
                    // Atomic multi-model commit journal (MVCC Tier 1, #83, M1b):
                    // open the CRC-framed append-only log at the data-dir root
                    // (reusing `forgedb-wal`'s published opaque `Raw` path — no new
                    // substrate).  Read its LAST CRC-valid record BEFORE building
                    // the collections: it is the authoritative durable
                    // { model_tag -> committed_len } map.  (An absent/empty journal
                    // ⇒ the collections' own #89 per-model recovery is authoritative
                    // and this loop is a no-op — backward compatible with a data dir
                    // written before transactions existed.)
                    let mut _txn_journal = forgedb_wal::WalManager::open(
                        root.join("_txn_journal.log"),
                        forgedb_wal::FsyncPolicy::Always,
                    ).expect("Failed to open transaction journal");
                    // The last valid record's decoded length vector (empty if none).
                    let __journal_committed: Vec<(String, u64)> = {
                        let __entries = _txn_journal
                            .replay(|_| -> std::io::Result<()> { Ok(()) })
                            .unwrap_or_default();
                        match __entries.last() {
                            Some(__e) => {
                                if let forgedb_wal::WalOperation::Raw { payload } = &__e.operation {
                                    serde_json::from_slice(payload).unwrap_or_default()
                                } else {
                                    Vec::new()
                                }
                            }
                            None => Vec::new(),
                        }
                    };
                    #(#attach_stmts_at)*
                    let mut __db = Self {
                        #(#field_idents,)*
                        changefeed,
                        broker,
                        _lock,
                        _txn_journal: Some(_txn_journal),
                        seq,
                    };
                    // Journal-driven recovery (M1b): a transaction that staged rows
                    // but whose journal commit record never landed left ahead-rows
                    // in some columns; truncate every touched collection back to its
                    // journalled committed length so the txn is all-or-nothing.  A
                    // committed txn's columns were fsynced BEFORE its journal record,
                    // so a present record ⟺ all its columns are durable ⟹ this is a
                    // no-op for it (its recovered length already matches).
                    for (__model, __len) in &__journal_committed {
                        let __len = *__len;
                        match __model.as_str() {
                            #(#journal_recover_arms)*
                            // Unknown tag (a model removed since the record) → skip.
                            _ => {}
                        }
                    }
                    __db
                }

                /// Capture a consistent read snapshot across all collections.
                /// Called on the single writer, so — because the writer is never
                /// mid-mutation between calls — the per-collection watermarks are
                /// captured atomically as of one commit boundary (#56 Direction B).
                /// Hand the returned snapshot to `DatabaseReader` accessors for a
                /// cross-model-consistent read view; rows appended after this call
                /// are invisible and no reader can observe a torn row.
                pub fn snapshot(&self) -> DatabaseSnapshot {
                    DatabaseSnapshot {
                        #(#snapshot_inits,)*
                    }
                }

                /// Force a WAL checkpoint across every collection (#89 step 2):
                /// fsync all columns and truncate every model WAL now, bounding
                /// the WALs and shortening the next reopen.  Models fsync-then-
                /// truncate their WAL; junctions (no WAL — a #89 boundary) only
                /// fsync their id columns.  Runs automatically every
                /// `WAL_CHECKPOINT_INTERVAL` mutations per collection; this is the
                /// explicit, force-now entry point.  Single-writer only.
                pub fn checkpoint(&mut self) {
                    #(self.#field_idents.checkpoint();)*
                }

                /// Force compaction across every model (#92 Phase 4): reclaim the
                /// dead (superseded/tombstoned) row versions the mutation surface
                /// (#66) leaves behind, in-process under the single-writer lock.
                /// Each model checkpoints, rewrites its files dropping dead rows,
                /// then reopens to rebuild its in-memory maps.  Runs automatically
                /// every `COMPACTION_DEAD_THRESHOLD` dead versions per model; this
                /// is the explicit, force-now entry point (parity with the manual
                /// `forgedb compact` CLI path).  Junctions are skipped — an
                /// append-only link table accumulates no dead versions.  Single-
                /// writer only.
                ///
                /// MVCC Tier 2 (#83, PM constraint 3): no row still visible as of
                /// the oldest live snapshot may be GC'd.  With genuine concurrent
                /// prepare (`SharedDatabase`/`Arc<RwLock<Database>>`), a
                /// `ConcurrentTxHandle` registers a live snapshot on the sequencer
                /// while another thread may take the write lock and call `compact()`,
                /// so this guard is **load-bearing in Tier 2**: if any snapshot is
                /// live, compaction is DEFERRED (skipped this pass) rather than
                /// augmenting the keep-set — a conservative correctness guarantee
                /// (never reclaims a pinned version) at the cost of compaction
                /// liveness under sustained concurrent load.  `forgedb-compaction`
                /// stays snapshot-unaware (opaque indices only).
                pub fn compact(&mut self) {
                    // MVCC Tier 2 (#83): if any snapshot is live in the sequencer,
                    // defer compaction — a live prepare snapshot may still need the
                    // versions a reclaim would drop.  Load-bearing under concurrent
                    // prepare (SharedDatabase), where a snapshot can coexist with a
                    // compaction call on another thread.
                    if let Ok(__seq) = self.seq.lock() {
                        if __seq.oldest_live_snapshot().as_u64() > 0 {
                            // At least one snapshot is live — skip compaction this pass.
                            // The per-model `in_transaction` flag separately defers
                            // auto-compaction inside storage.
                            return;
                        }
                    }
                    #(self.#model_field_idents.compact();)*
                }

                /// Open a read-only handle over the whole database (#56 Direction
                /// B).  The returned `DatabaseReader` shares every collection's
                /// files via independent descriptors, so it (and any clones handed
                /// to reader threads) reads the committed prefix lock-free while
                /// this writer keeps mutating.
                pub fn reader(&self) -> DatabaseReader {
                    DatabaseReader {
                        #(#reader_inits,)*
                    }
                }

                /// Apply one replicated change frame on the browser read-replica
                /// follower (#110 Milestone C).
                ///
                /// Dispatches on the frame's **opaque model tag** (a generated
                /// closed set — the follower never reads a `.forge` schema at
                /// runtime) to the matching generated collection, which decodes
                /// the opaque row bytes and replays through the SAME generated
                /// mutation surface the server used.  An unknown tag is ignored
                /// (forward-compatible with a server that gained a model).  The
                /// change feed / broker are `None` on a follower, so applying
                /// neither re-emits nor re-journals — it is a pure local replay.
                ///
                /// Idempotent by absolute position for an in-order, from-zero
                /// replay: the follower reproduces the server's exact physical
                /// layout row-for-row.
                pub fn apply_frame(
                    &mut self,
                    ev: &forgedb_changefeed::durable::PersistedEvent,
                ) -> Result<(), ApplyError> {
                    match ev.model.as_str() {
                        #(#apply_model_arms)*
                        #(#apply_junction_arms)*
                        // Unknown model tag: ignore (a newer server model the
                        // replica's generated code does not know).  Forward-compat.
                        _ => Ok(()),
                    }
                }

                /// Point-in-time recovery (#77): replay durable-broker frames in
                /// `(base_offset .. target_offset]` through `apply_frame`, then
                /// commit, leaving this database at exactly the state as of broker
                /// offset `target_offset`.
                ///
                /// The recovery recipe: restore a full base backup (taken at broker
                /// offset `base_offset`, recorded in the archive header) into this
                /// data dir, place the retained `_replication.log` alongside it, then
                /// `open_at` and call `recover_to(base_offset, target_offset)`. The
                /// base's committed columns already contain every change through
                /// `base_offset` (the generated mutation records to the broker only
                /// *after* the column commit), so replay begins strictly *after* it —
                /// exactly like #110's reload-resume skips frames `<= watermark`.
                ///
                /// Recovery reuses the SAME opaque-model-tag `apply_frame` the
                /// browser follower (#110) uses — there is no second decode/apply
                /// path, and the broker offset is the only ordering key (never a
                /// decoded field). Reads the log via `DurableBroker::read_from` in
                /// bounded batches so a long log does not materialize at once.
                ///
                /// Idempotent by id: even if a boundary frame `<= base_offset` were
                /// re-applied (it is not, given the strict `>` filter), the
                /// superseding-version append means `get`/`all` still resolve one
                /// record per id — double-apply is read-safe.
                ///
                /// Returns the highest offset actually applied (`base_offset` if the
                /// log holds nothing after it, capped at the log's watermark if
                /// `target_offset` runs past the retained tail). Errors if there is
                /// no broker (the standalone `new()` path has no `_replication.log`
                /// to replay) or a frame fails to apply.
                pub fn recover_to(
                    &mut self,
                    base_offset: u64,
                    target_offset: u64,
                ) -> Result<u64, ApplyError> {
                    // Read frames from the broker's durable log, but DETACH the
                    // broker for the duration of replay so `apply_frame`'s
                    // insert/update/delete do NOT re-record the frames we are
                    // reading back into the same `_replication.log` (which would
                    // duplicate the tail and corrupt the offset stream).  This is
                    // exactly why the #110 follower runs with `broker: None`; here
                    // we take it out, replay, then put the original handle back so
                    // the recovered database stays usable (its log is unchanged,
                    // still holding the pre-recovery frames).
                    let broker = self
                        .broker
                        .take()
                        .ok_or_else(|| ApplyError::Decode(
                            "no replication broker: recover_to needs a data dir \
                             opened via open_at with a _replication.log present"
                                .to_string(),
                        ))?;
                    // Detach the shared handle from every collection too, so their
                    // generated mutations stay inert during replay.
                    #(self.#field_idents.attach_broker(None);)*

                    const BATCH: usize = 512;
                    let mut after = base_offset;
                    let mut applied = base_offset;
                    let result: Result<u64, ApplyError> = (|| {
                        loop {
                            // Read the next batch of frames strictly after `after`.
                            let frames = {
                                let guard = broker.lock().unwrap();
                                guard
                                    .read_from(after, BATCH)
                                    .map_err(|e| ApplyError::Decode(e.to_string()))?
                            };
                            if frames.is_empty() {
                                break;
                            }
                            for ev in &frames {
                                if ev.offset > target_offset {
                                    // Reached the recovery point — stop, don't apply.
                                    return Ok(applied);
                                }
                                self.apply_frame(ev)?;
                                applied = ev.offset;
                                after = ev.offset;
                            }
                        }
                        Ok(applied)
                    })();

                    // Re-attach the broker regardless of outcome, then flush.
                    #(self.#field_idents.attach_broker(Some(broker.clone()));)*
                    self.broker = Some(broker);
                    let applied = result?;
                    self.commit().map_err(|e| ApplyError::Decode(e.to_string()))?;
                    Ok(applied)
                }

                /// Commit every collection (#110 Milestone C additive commit):
                /// flush all model columns + tombstones and fsync junction id
                /// columns.  Native: an fsync durability boundary.  Wasm arena
                /// backend: per-column `flush` is a no-op — durability instead
                /// comes from the transport writing the arena blobs to
                /// IndexedDB/OPFS in one transaction after this returns.  So on
                /// the follower this is the "arenas are consistent, safe to
                /// persist" barrier the transport's async commit waits on.
                pub fn commit(&mut self) -> std::io::Result<()> {
                    #(
                        self.#model_field_idents.commit()?;
                    )*
                    #(
                        self.#junction_field_idents.checkpoint();
                    )*
                    Ok(())
                }
            }
        };

        Ok(tokens)
    }

    /// Generate the `Database`-level validated write wrappers (#91 Phase 3):
    /// `create_<model>` / `update_<model>`.  These add the one integrity check the
    /// storage-scoped `insert`/`update` cannot make — **foreign-key existence** —
    /// because it needs sibling-collection access (`self.<target>.get(fk)`), then
    /// delegate to the storage method (which enforces field constraints + `&unique`).
    /// Required FKs must resolve; optional FKs must resolve *when set*.  Only
    /// UUID-keyed targets are checked (an FK is always a `Uuid`, so an integer-PK
    /// target cannot be resolved by it — the same restriction as relation
    /// traversal).  The generated REST boundary routes creates/updates through
    /// these, so both the Rust API and REST get full integrity.
    fn generate_validated_writes(schema: &Schema) -> TokenStream {
        let mut methods = Vec::new();
        for model in &schema.models {
            if !model.fields.iter().any(|f| f.name == "id" || f.auto_generate) {
                continue;
            }
            let snake = Self::to_snake_case(&model.name);
            let storage_field = format_ident!("{}", snake);
            let model_ident = format_ident!("{}", model.name);
            let id_type = Self::id_type_tokens(model);
            let create_fn = format_ident!("create_{}", snake);
            let update_fn = format_ident!("update_{}", snake);

            let fk_checks: Vec<_> = model
                .fields
                .iter()
                .filter_map(|field| {
                    let (target_name, optional) = match &field.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => return None,
                    };
                    let target = schema.find_model(target_name)?;
                    if !Self::is_uuid_pk(target) {
                        return None;
                    }
                    let target_storage = format_ident!("{}", Self::to_snake_case(&target.name));
                    let fk_field = format_ident!("{}", field.name);
                    let fname = field.name.as_str();
                    let tname = target.name.as_str();
                    Some(if optional {
                        quote! {
                            if let Some(__fk) = record.#fk_field {
                                if self.#target_storage.get(__fk).is_none() {
                                    return Err(ValidationError::DanglingReference {
                                        field: #fname, target: #tname,
                                    });
                                }
                            }
                        }
                    } else {
                        quote! {
                            if self.#target_storage.get(record.#fk_field).is_none() {
                                return Err(ValidationError::DanglingReference {
                                    field: #fname, target: #tname,
                                });
                            }
                        }
                    })
                })
                .collect();

            let create_doc = format!(
                "Create a {} with full integrity (#91): foreign keys must resolve, \
                 then field constraints + `&unique` are enforced by `insert`.",
                model.name
            );
            let update_doc = format!(
                "Update a {} with full integrity (#91): foreign keys must resolve, \
                 then field constraints + `&unique` are enforced by `update`. \
                 `Ok(false)` if the id is absent.",
                model.name
            );
            methods.push(quote! {
                #[doc = #create_doc]
                pub fn #create_fn(&mut self, record: #model_ident) -> Result<#id_type, ValidationError> {
                    #(#fk_checks)*
                    self.#storage_field.insert(record)
                }

                #[doc = #update_doc]
                pub fn #update_fn(&mut self, id: #id_type, record: #model_ident) -> Result<bool, ValidationError> {
                    #(#fk_checks)*
                    self.#storage_field.update(id, record)
                }
            });
        }

        // --- delete_<model> wrappers (delete semantics: referential integrity +
        // cascade + set_null + M2M cascade-unlink).  Mirrors the create/update
        // wrapper pattern (#91): the referential logic needs sibling-collection
        // access `Storage` lacks, so it lives on `Database`.  The direct
        // `db.<model>.delete` storage path SKIPS these checks (same caveat #91
        // documents for `insert`) — REST + `Database::delete_<model>` get full
        // integrity.
        let delete_methods = Self::generate_delete_wrappers(schema);
        for m in delete_methods {
            methods.push(m);
        }

        if methods.is_empty() {
            return quote! {};
        }
        quote! {
            /// Maximum on-delete cascade recursion depth (delete semantics).  A
            /// cascade delete recurses into referencing children (which may cascade
            /// in turn); this bounds a pathological schema FK cycle so it fails
            /// loudly with `ReferencedByChildren`-class refusal rather than
            /// looping.  Fixed for v1 (same posture as the WAL/compaction consts).
            const MAX_CASCADE_DEPTH: u32 = 64;

            impl Database {
                #(#methods)*
            }
        }
    }

    /// Generate the `delete_<model>` referential-integrity wrappers on `Database`
    /// (delete semantics).  For each UUID-keyed parent model, emits:
    ///   * a public `delete_<model>(id) -> Result<bool, ValidationError>` that
    ///     evaluates every child FK's `@on_delete` policy before delegating to
    ///     `<model>Storage::delete`, and unlinks the model's M2M junction rows;
    ///   * an internal depth-bounded `delete_<model>_cascade(id, depth)` that the
    ///     public method and any cascading parent call (cascade recurses through
    ///     the child's OWN wrapper so ITS rules fire — multi-level chains work).
    ///
    /// Policies (default `restrict` when `@on_delete` is absent):
    ///   * `restrict`  → refuse if any live child references the parent (409).
    ///   * `cascade`   → recursively delete every referencing child.
    ///   * `set_null`  → null each referencing (optional) child FK via update.
    ///
    /// All FK-graph walking is done here at compile time; the emitted code reads
    /// no schema at runtime (identity red line held).
    fn generate_delete_wrappers(schema: &Schema) -> Vec<TokenStream> {
        let mut out = Vec::new();

        for parent in &schema.models {
            // A wrapper is generated for every identity model (matching the
            // create/update wrappers), so the REST DELETE route always has one.
            if !parent.fields.iter().any(|f| f.name == "id" || f.auto_generate) {
                continue;
            }
            let parent_snake = Self::to_snake_case(&parent.name);
            let parent_storage = format_ident!("{}", parent_snake);
            let public_fn = format_ident!("delete_{}", parent_snake);
            let cascade_fn = format_ident!("delete_{}_cascade", parent_snake);
            let parent_name_str = parent.name.as_str();
            let id_type = Self::id_type_tokens(parent);

            // Integer-PK models are never FK targets (FKs are always UUID) and
            // never in an M2M (junctions require UUID PKs on both sides), so their
            // wrapper is a trivial delegate — no referential-integrity work.
            if !Self::is_uuid_pk(parent) {
                let doc = format!(
                    "Delete a {} (integer-keyed): no model references it via a \
                     foreign key, so this delegates to storage `delete`.  Returns \
                     `Ok(false)` if the id is absent.",
                    parent.name
                );
                out.push(quote! {
                    #[doc = #doc]
                    pub fn #public_fn(&mut self, id: #id_type) -> Result<bool, ValidationError> {
                        Ok(self.#parent_storage.delete(id))
                    }
                });
                continue;
            }

            // Every child FK field that references THIS parent, with its policy.
            // A child may have multiple FKs to the same parent (disambiguated by
            // field), each with its own `@on_delete`.  Restrict CHECKS run before
            // any cascade/set_null mutation, so a later `restrict` sibling refuses
            // the whole delete without a partial cascade having already fired.
            let mut restrict_checks: Vec<TokenStream> = Vec::new();
            let mut mutations: Vec<TokenStream> = Vec::new();
            for child in &schema.models {
                if !Self::is_uuid_pk(child) {
                    continue;
                }
                let child_snake = Self::to_snake_case(&child.name);
                let child_storage = format_ident!("{}", child_snake);
                let child_delete = format_ident!("delete_{}_cascade", child_snake);
                let child_update = format_ident!("update_{}", child_snake);

                for field in &child.fields {
                    let (target_name, optional) = match &field.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => continue,
                    };
                    if target_name != &parent.name {
                        continue;
                    }
                    // The reverse FK must be indexed to probe children in O(1).
                    // FK fields are always indexed (#100), so this holds; guard
                    // anyway so a non-indexed edge falls back to a scan.
                    let fk_field = format_ident!("{}", field.name);
                    let fk_probe = format_ident!("find_by_{}", field.name);
                    let child_field_str = field.name.as_str();
                    let child_name_str = child.name.as_str();
                    let probe_arg = if optional {
                        quote! { Some(id) }
                    } else {
                        quote! { id }
                    };
                    let child_indexed = child
                        .fields
                        .iter()
                        .any(|f| f.name == "id" || f.auto_generate);
                    let find_children = if child_indexed {
                        quote! { self.#child_storage.#fk_probe(#probe_arg) }
                    } else {
                        let pred = if optional {
                            quote! { __c.#fk_field == Some(id) }
                        } else {
                            quote! { __c.#fk_field == id }
                        };
                        quote! {
                            self.#child_storage.all().into_iter().filter(|__c| #pred).collect::<Vec<_>>()
                        }
                    };

                    match Self::on_delete_policy(field) {
                        OnDeletePolicy::Restrict => {
                            restrict_checks.push(quote! {
                                {
                                    let __children = #find_children;
                                    if !__children.is_empty() {
                                        return Err(ValidationError::ReferencedByChildren {
                                            model: #child_name_str,
                                            field: #child_field_str,
                                        });
                                    }
                                }
                            });
                        }
                        OnDeletePolicy::Cascade => {
                            mutations.push(quote! {
                                {
                                    let __children = #find_children;
                                    for __c in __children {
                                        self.#child_delete(__c.id, __depth + 1)?;
                                    }
                                }
                            });
                        }
                        OnDeletePolicy::SetNull => {
                            // Only reachable for OPTIONAL FKs (validated above),
                            // so `#fk_field = None` type-checks.
                            mutations.push(quote! {
                                {
                                    let __children = #find_children;
                                    for mut __c in __children {
                                        let __cid = __c.id;
                                        __c.#fk_field = None;
                                        self.#child_update(__cid, __c)?;
                                    }
                                }
                            });
                        }
                    }
                }
            }

            // M2M cascade-unlink: any junction this parent participates in has its
            // pairs referencing the deleted id retracted, so traversal no longer
            // resolves the gone record.  Emitted for both sides of each M2M.
            let mut m2m_unlinks: Vec<TokenStream> = Vec::new();
            for m in Self::valid_m2m(schema) {
                let junction_field = Self::junction_field_ident(&m);
                if m.model1 == parent.name {
                    m2m_unlinks.push(quote! {
                        self.#junction_field.unlink_all_left(id);
                    });
                }
                if m.model2 == parent.name {
                    let junction_field2 = Self::junction_field_ident(&m);
                    m2m_unlinks.push(quote! {
                        self.#junction_field2.unlink_all_right(id);
                    });
                }
            }

            let doc = format!(
                "Delete a {} with referential integrity (delete semantics): each child \
                 FK's `@on_delete` policy (restrict/cascade/set_null) is applied, this \
                 model's M2M junction rows are unlinked, then the row is tombstoned. \
                 Returns `Ok(false)` if the id is absent, `Err(ReferencedByChildren)` \
                 (409) if a `restrict` child blocks the delete.  The REST DELETE route \
                 goes through this wrapper; the direct `db.{}.delete` storage path does \
                 NOT (it skips these checks).",
                parent.name, parent_snake
            );

            out.push(quote! {
                #[doc = #doc]
                pub fn #public_fn(&mut self, id: Uuid) -> Result<bool, ValidationError> {
                    self.#cascade_fn(id, 0)
                }

                /// Internal depth-bounded cascade worker.  `depth` guards a
                /// pathological FK cycle (bounded by `MAX_CASCADE_DEPTH`).
                fn #cascade_fn(&mut self, id: Uuid, __depth: u32) -> Result<bool, ValidationError> {
                    if __depth > MAX_CASCADE_DEPTH {
                        return Err(ValidationError::ReferencedByChildren {
                            model: #parent_name_str,
                            field: "on_delete cascade depth exceeded",
                        });
                    }
                    // Nothing to delete → no-op (matches storage `delete` semantics),
                    // and skip child work so a cascade cycle terminates.
                    if self.#parent_storage.get(id).is_none() {
                        return Ok(false);
                    }
                    // Restrict checks run FIRST so a violation refuses before any
                    // cascade/set_null side effect fires.
                    #(#restrict_checks)*
                    // Then cascade-delete / set-null the referencing children.
                    #(#mutations)*
                    // Retract M2M junction rows for this id (both sides).
                    #(#m2m_unlinks)*
                    Ok(self.#parent_storage.delete(id))
                }
            });
        }

        out
    }

    /// Generate the transaction surface (MVCC Tier 1, #83): the `TxHandle` struct,
    /// its scoped per-model write (`create_/update_/delete_`) + read (`get_/all_`)
    /// methods, `Database::transaction`, and the commit/rollback machinery.
    ///
    /// Identity: this is entirely GENERATED per schema (a method on the tailored
    /// `Database`) — not a shipped generic `db.begin()` over an arbitrary schema.
    /// `TxHandle` exposes exactly the per-model methods codegen already emits,
    /// scoped to a transaction; no predicate-as-data, no runtime schema.  The only
    /// substrate used is the already-published `forgedb-wal` `Raw` path (the commit
    /// journal), `truncate_to_rows`, and the watermark — no new crate, no format
    /// change.  Skipped entirely for a schema with no id-bearing (transactable)
    /// model.
    fn generate_transaction_impl(schema: &Schema) -> TokenStream {
        // Only id-bearing models are transactable (they can be inserted / mutated).
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        // Rollback arms: truncate every touched collection's columns + WAL tail
        // back to its recorded pre-txn mark, and clear its txn guard.
        let rollback_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let __mark = *__mark;
                        // Erase staged rows: truncate every column + tombstone.
                        let _ = self.db.#field.__truncate_all_to(__mark);
                        self.db.#field.row_count = __mark;
                        // Drop the staged WAL tail back to the recorded byte mark.
                        if let Some(__wm) = self.wal_marks.get(#tag) {
                            let _ = self.db.#field.wal.truncate_to(*__wm);
                        }
                        // Leave the txn guard; run any deferred maintenance now.
                        self.db.#field.in_transaction = false;
                        self.db.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        // Commit reindex arms: after the watermark is (already) advanced by the
        // staged appends, rebuild each touched collection's id_to_row + indexes
        // from the now-visible committed prefix, clear the guard, run deferred
        // maintenance.
        let commit_reindex_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        self.db.#field.__reindex_committed();
                        self.db.#field.in_transaction = false;
                        self.db.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        // Commit fsync arms: make each touched collection's columns + WAL durable
        // BEFORE the journal record (the ordering that makes the journal fsync the
        // atomic commit point).
        let commit_fsync_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let _ = self.db.#field.commit();
                        let _ = self.db.#field.wal.flush();
                        // Record this collection's post-commit committed length for
                        // the journal (the atomic multi-model commit marker).
                        __journal.push((#tag.to_string(), self.db.#field.row_count as u64));
                    }
                }
            })
            .collect();

        // Per-model scoped write + read methods on TxHandle.
        let mut tx_methods: Vec<TokenStream> = Vec::new();
        for model in &tx_models {
            tx_methods.push(Self::generate_txn_model_methods(model, schema));
        }

        // The `mark_<model>` helpers: record the pre-txn physical row count + WAL
        // byte offset the first time a collection is written, and set its txn guard.
        let mark_methods: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                let mark_fn = format_ident!("__mark_{}", Self::to_snake_case(&m.name));
                quote! {
                    /// Record `#tag`'s pre-txn rollback marks on first touch and set
                    /// its transaction guard (so auto-checkpoint/compaction defer).
                    fn #mark_fn(&mut self) {
                        if !self.marks.contains_key(#tag) {
                            let __rc = self.db.#field.row_count;
                            let __wb = self.db.#field.wal.size().unwrap_or(0);
                            self.marks.insert(#tag, __rc);
                            self.wal_marks.insert(#tag, __wb);
                            self.db.#field.in_transaction = true;
                        }
                    }
                }
            })
            .collect();

        quote! {
            /// A scoped transaction handle (MVCC Tier 1, #83).  Exposes exactly the
            /// per-model write surface codegen already emits (`create_/update_/
            /// delete_`), scoped to the transaction, plus a read surface
            /// (`get_/all_`) with **read-your-writes**: reads resolve against the
            /// staged physical length (the watermark raised past the public one),
            /// so the transaction sees its own uncommitted appends over the
            /// committed prefix — the same `get_at` decode path, just a raised
            /// watermark (no forked decoder).  Outside readers still clamp at the
            /// public watermark, so staged rows are invisible until commit.
            pub struct TxHandle<'db> {
                db: &'db mut Database,
                /// Pre-txn physical row-count per touched collection (lazy, on first
                /// write) — the rollback truncation target.
                marks: std::collections::BTreeMap<&'static str, usize>,
                /// Pre-txn per-model WAL byte offset per touched collection — the
                /// rollback WAL-tail truncation target.
                wal_marks: std::collections::BTreeMap<&'static str, u64>,
                /// Buffered change-feed + durable-broker records, drained (offsets
                /// consumed) only on commit — so a rolled-back txn consumes NO
                /// broker offset and emits NO changefeed event (no offset gap).
                pending_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, usize, Vec<u8>)>,
                /// Unique keys claimed by staged writes in this transaction (M1a fix).
                ///
                /// Each entry is `(field_name, encoded_key)`.  Checked ALONGSIDE the
                /// committed index so that two `create_<model>` calls with the SAME
                /// `&unique` value in one transaction both see the conflict — the
                /// committed index only reflects rows visible before the txn started.
                /// Rollback discards the set automatically (it is owned by `TxHandle`
                /// and never touches the real index maps, so no undo is needed).
                staged_unique_keys: std::collections::BTreeSet<(&'static str, String)>,
                /// Set once `commit` runs; the `Drop` backstop rolls back otherwise.
                committed: bool,
            }

            impl<'db> TxHandle<'db> {
                fn begin(db: &'db mut Database) -> Self {
                    TxHandle {
                        db,
                        marks: std::collections::BTreeMap::new(),
                        wal_marks: std::collections::BTreeMap::new(),
                        pending_events: Vec::new(),
                        staged_unique_keys: std::collections::BTreeSet::new(),
                        committed: false,
                    }
                }

                #(#mark_methods)*

                #(#tx_methods)*

                /// Commit the transaction (MVCC Tier 1, #83).  Ordering is
                /// load-bearing (mirrors #96's checkpoint order): (1) fsync every
                /// touched collection's columns + per-model WAL, (2) append + fsync
                /// the atomic multi-model commit record to `_txn_journal.log` (THE
                /// commit point — a present record ⟺ all its columns durable), (3)
                /// advance visibility by rebuilding each touched collection's
                /// id_to_row + indexes from the now-committed prefix, (4) drain the
                /// buffered changefeed + broker events (offsets consumed HERE and
                /// only here).  A crash before step 2 leaves no journal record → the
                /// staged rows are truncated on reopen (all-or-nothing).
                pub fn commit(mut self) -> Result<(), TxError> {
                    // Step 1 + build the journal length vector.
                    let mut __journal: Vec<(String, u64)> = Vec::new();
                    // Iterate marks in a stable order (BTreeMap) so the journal is
                    // deterministic; only touched collections appear.
                    let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
                    for __tag in &__touched {
                        match *__tag {
                            #(#commit_fsync_arms)*
                            _ => {}
                        }
                    }
                    // Step 2: the atomic commit point.  Encode the length vector as
                    // opaque bytes and write it via the WAL `Raw` path (fsync-always).
                    if let Some(__jrnl) = self.db._txn_journal.as_mut() {
                        let __payload = serde_json::to_vec(&__journal)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl
                            .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
                    }
                    // Step 3: advance visibility (rebuild maps) + close the guard +
                    // run any deferred maintenance, per touched collection.
                    for __tag in &__touched {
                        match *__tag {
                            #(#commit_reindex_arms)*
                            _ => {}
                        }
                    }
                    // Step 4: drain the buffered events to the shared broker +
                    // change feed, consuming the contiguous offset run HERE (one
                    // burst per commit).  A rolled-back txn never reaches this, so it
                    // leaves no offset gap.
                    for (__model, __kind, _id_bytes, __row, __bytes) in std::mem::take(&mut self.pending_events) {
                        // Best-effort in-process change feed (owned by `Database`).
                        self.db.changefeed.emit(__model, __row, __kind);
                        // Durable replication broker (#82): consume the contiguous
                        // offset run HERE — one burst per commit, no gap on abort.
                        if let Some(__broker) = &self.db.broker {
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(__model, __row as u64, __kind, __bytes);
                            }
                        }
                    }
                    self.committed = true;
                    Ok(())
                }

                /// Roll back the transaction: truncate every touched collection's
                /// staged rows + WAL tail back to its pre-txn mark, discard the
                /// buffered events (no broker offset consumed, no changefeed emit),
                /// and clear each txn guard.  No journal record is written, no
                /// watermark advanced — nothing outside the transaction ever
                /// observed the staged rows.
                pub fn rollback(mut self) {
                    self.rollback_internal();
                    self.committed = true; // prevent the Drop backstop double-running
                }

                fn rollback_internal(&mut self) {
                    let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
                    for __tag in &__touched {
                        if let Some(__mark) = self.marks.get(*__tag) {
                            match *__tag {
                                #(#rollback_arms)*
                                _ => {}
                            }
                        }
                    }
                    self.pending_events.clear();
                }

                /// Compute the write-set for the commit sequencer (MVCC Tier 2, #83).
                ///
                /// Returns all opaque keys touched by this transaction: one per staged
                /// row (`model_tag_bytes ++ logical_id_bytes`) and one per claimed
                /// unique key (`field_name_bytes ++ encoded_key_bytes`).  The
                /// sequencer sees only opaque bytes — no model name, no field, no
                /// schema — preserving the identity red line.
                pub fn __write_set(&self, snapshot_lsn: forgedb_txn::Lsn) -> forgedb_txn::WriteSet {
                    let mut keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
                    // Row-level opaque keys: model_tag_bytes ++ logical_id_bytes.
                    // Using the logical entity id (not the physical row index) so two concurrent
                    // transactions that both write the same logical entity conflict,
                    // regardless of which physical rows they stage.
                    for (__model, _kind, __id_bytes, _row, _bytes) in &self.pending_events {
                        let mut k = Vec::with_capacity(__model.len() + __id_bytes.len());
                        k.extend_from_slice(__model.as_bytes());
                        k.extend_from_slice(__id_bytes);
                        keys.push(k.into_boxed_slice());
                    }
                    // Unique-key opaque keys: field_name_bytes ++ encoded_key_bytes.
                    // Capturing unique-key claims prevents two concurrent txns from
                    // committing the same unique value even when they staged different
                    // physical rows (which have different row-level keys above).
                    for (__fname, __ekey) in &self.staged_unique_keys {
                        let mut k = Vec::with_capacity(__fname.len() + __ekey.len());
                        k.extend_from_slice(__fname.as_bytes());
                        k.extend_from_slice(__ekey.as_bytes());
                        keys.push(k.into_boxed_slice());
                    }
                    forgedb_txn::WriteSet { keys, snapshot_lsn }
                }
            }

            impl<'db> Drop for TxHandle<'db> {
                /// Panic-safety backstop: an un-committed handle (dropped via an
                /// early `?`, a panic, or an explicit non-commit) rolls back, so a
                /// half-staged transaction never becomes visible.
                fn drop(&mut self) {
                    if !self.committed {
                        self.rollback_internal();
                    }
                }
            }

            /// Default retry count for `transaction_optimistic` (#83, Tier 2).
            ///
            /// Override per-call via `transaction_retrying(retries, f)`.
            /// (`forgedb.toml` `[transaction] max_retries` wiring is a residual —
            /// the knob is exposed via `transaction_retrying` for now.)
            const DEFAULT_TXN_RETRIES: u32 = 3;

            impl Database {
                /// Run a transaction (MVCC Tier 1, #83).  The closure receives a
                /// scoped `TxHandle` exposing the per-model `create_/update_/delete_`
                /// writes + `get_/all_` reads; its writes are staged (physically
                /// appended past the public watermark, durable in the per-model WAL,
                /// but invisible outside) and made visible ATOMICALLY on `Ok`
                /// (commit) or discarded on `Err`/panic (rollback-by-truncate).
                /// Multi-model writes commit all-or-nothing across a crash via the
                /// `_txn_journal.log` (M1b).
                ///
                /// ```ignore
                /// let id = db.transaction(|tx| {
                ///     let uid = tx.create_user(user)?;   // staged
                ///     tx.create_post(post)?;             // atomic with the user
                ///     Ok(uid)
                /// })?;                                   // Ok → commit; Err → rollback
                /// ```
                pub fn transaction<T>(
                    &mut self,
                    f: impl FnOnce(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    let mut tx = TxHandle::begin(self);
                    match f(&mut tx) {
                        Ok(v) => {
                            tx.commit()?;
                            Ok(v)
                        }
                        Err(e) => {
                            tx.rollback();
                            Err(e)
                        }
                    }
                }

                /// Run an optimistic transaction with auto-retry (MVCC Tier 2, #83).
                ///
                /// The closure `f` is `Fn` (re-runnable): it may be invoked up to
                /// `retries + 1` times.  Each attempt: (1) registers a snapshot LSN
                /// via the `CommitSequencer`, (2) runs the prepare closure staging
                /// writes into private buffers via `TxHandle`, (3) at the serialized
                /// commit point calls `CommitSequencer::try_commit` — on
                /// `Committed`, applies via the Tier-1 commit; on `Conflict`, drops
                /// the buffers and retries (snapshot re-registered).
                ///
                /// **Isolation:** snapshot isolation, first-committer-wins.  The
                /// disclosed anomaly is write-skew (two txns read overlapping key
                /// sets, each writes a disjoint subset; both may commit in SI).
                /// Serializable snapshot isolation (SSI, read-set tracking) is Tier 3.
                ///
                /// **"Concurrent writers"** = concurrent *prepare*, serialized
                /// *commit*.  In Tier 2, `transaction_retrying` takes `&mut self`,
                /// so prepare is also serialized (only one `&mut` holder at a time).
                /// The sequencer and conflict-map are structurally ready for
                /// concurrent prepare; `&mut Database` makes it a no-op here.
                /// Genuine concurrent prepare lands in Tier 3
                /// (`Arc<RwLock<Database>>`).
                pub fn transaction_retrying<T>(
                    &mut self,
                    retries: u32,
                    f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    // Clone the Arc<Mutex<CommitSequencer>> BEFORE the loop so we
                    // can call the sequencer while `tx` (which holds &mut Database)
                    // is live — cloning the Arc does not require borrowing `self`.
                    let __seq_arc = std::sync::Arc::clone(&self.seq);
                    for __attempt in 0..=retries {
                        // Step 1: register the read snapshot (current committed LSN).
                        // This can still use `self.seq` because `tx` is not yet alive.
                        let __snap_lsn = {
                            let mut __seq = self.seq.lock().unwrap();
                            __seq.register_snapshot()
                        };
                        // TxHandle::begin takes &mut Database (self), so no further
                        // `self.seq` borrows are possible while `tx` lives.  All
                        // sequencer calls below use the cloned Arc.
                        let mut tx = TxHandle::begin(self);
                        let __out = f(&mut tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                tx.rollback();
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };
                        // Step 2: build the write-set from the staged transaction,
                        // then hand it to the sequencer for first-committer-wins check.
                        let __ws = tx.__write_set(__snap_lsn);
                        let __outcome = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.try_commit(&__ws)
                        };
                        match __outcome {
                            forgedb_txn::CommitOutcome::Committed(_l) => {
                                // Apply the staged writes via Tier-1 commit.
                                tx.commit()?;
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                let _ = __attempt; // suppress unused variable warning
                                return Ok(__val);
                            }
                            forgedb_txn::CommitOutcome::Conflict { .. } => {
                                // Drop the staged writes and retry.
                                tx.rollback();
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                // Loop continues to next attempt.
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }

                /// Run an optimistic transaction with the default retry count (#83).
                ///
                /// Uses `DEFAULT_TXN_RETRIES` (= 3 retries, 4 total attempts).  For
                /// per-call control use `transaction_retrying(retries, f)`.
                pub fn transaction_optimistic<T>(
                    &mut self,
                    f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    self.transaction_retrying(DEFAULT_TXN_RETRIES, f)
                }
            }
        }
    }

    /// Generate the Tier 2 concurrent-prepare surface (#83): `SharedDatabase` wrapping
    /// `Arc<RwLock<Database>>`, and `ConcurrentTxHandle` which stages into private
    /// in-memory buffers (touching NO shared state during prepare).  The commit
    /// critical section takes the exclusive write lock briefly to apply the buffer
    /// via the Tier-1 apply path and advance visibility.
    ///
    /// Identity red line: the conflict-detection path (sequencer) sees only opaque
    /// keys; model-tag dispatch only appears in the apply path (after conflict
    /// resolution passes) — the same pattern as `apply_frame` for the follower.
    fn generate_shared_database_impl(schema: &Schema) -> TokenStream {
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        // Per-model apply arms for __apply_and_commit_concurrent_buffer.
        let concurrent_apply_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let model_name = format_ident!("{}", m.name);
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let __record: #model_name = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.#field.__stage_append(__record, *__deleted);
                        __staged_events.push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                }
            })
            .collect();

        // Per-model mark arms: record pre-txn row_count + wal byte offset, set in_transaction.
        let concurrent_mark_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        if !__marks.contains_key(#tag) {
                            let __rc = self.#field.row_count;
                            let __wb = self.#field.wal.size().unwrap_or(0);
                            __marks.insert(#tag, __rc);
                            __wal_marks.insert(#tag, __wb);
                            self.#field.in_transaction = true;
                        }
                    }
                }
            })
            .collect();

        // Per-model fsync arms.
        let concurrent_fsync_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let _ = self.#field.commit();
                        let _ = self.#field.wal.flush();
                        __journal.push((#tag.to_string(), self.#field.row_count as u64));
                    }
                }
            })
            .collect();

        // Per-model reindex arms.
        let concurrent_reindex_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        self.#field.__reindex_committed();
                        self.#field.in_transaction = false;
                        self.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        // Per-model rollback arms for concurrent apply.
        let concurrent_rollback_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        if let Some(&__mark) = __marks.get(#tag) {
                            let _ = self.#field.__truncate_all_to(__mark);
                            self.#field.row_count = __mark;
                            if let Some(&__wm) = __wal_marks.get(#tag) {
                                let _ = self.#field.wal.truncate_to(__wm);
                            }
                            self.#field.in_transaction = false;
                            self.#field.run_deferred_maintenance();
                        }
                    }
                }
            })
            .collect();

        // Per-model concurrent methods on ConcurrentTxHandle.
        let mut concurrent_model_methods: Vec<TokenStream> = Vec::new();
        for model in &tx_models {
            let model_name = format_ident!("{}", model.name);
            let model_tag = model.name.as_str();
            let snake = Self::to_snake_case(&model.name);
            let field = format_ident!("{}", snake);
            let id_type = Self::id_type_tokens(model);
            let id_field = Self::id_field_ident(model);
            let validate_fn = format_ident!("validate_{}", snake);
            let create_fn = format_ident!("create_{}", snake);
            let update_fn = format_ident!("update_{}", snake);
            let delete_fn = format_ident!("delete_{}", snake);
            let get_fn = format_ident!("get_{}", snake);
            let all_fn = format_ident!("all_{}", snake);
            let snap_field = format_ident!("{}", snake);

            // Unique checks (insert) for ConcurrentTxHandle.
            let unique_checks_insert: Vec<_> = Self::indexed_fields(model)
                .iter()
                .filter(|f| f.unique)
                .map(|f| {
                    let ident = Self::index_field_ident(f);
                    let fident = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let key = Self::index_key_expr(Self::index_value_expr(
                        &f.field_type,
                        quote! { record.#fident },
                    ));
                    quote! {
                        {
                            let __uk: String = { #key };
                            if self.staged_unique_keys.contains(&(#fname, __uk.clone())) {
                                return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                            }
                            {
                                let __db = self.inner.read().unwrap();
                                if __db.#field.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                                    return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                                }
                            }
                            self.staged_unique_keys.insert((#fname, __uk));
                        }
                    }
                })
                .collect();

            // Unique checks (update) for ConcurrentTxHandle.
            let unique_checks_update: Vec<_> = Self::indexed_fields(model)
                .iter()
                .filter(|f| f.unique)
                .map(|f| {
                    let ident = Self::index_field_ident(f);
                    let fident = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let key = Self::index_key_expr(Self::index_value_expr(
                        &f.field_type,
                        quote! { record.#fident },
                    ));
                    quote! {
                        {
                            let __uk: String = { #key };
                            {
                                let __db = self.inner.read().unwrap();
                                if let Some(__ids) = __db.#field.#ident.get(&__uk) {
                                    if __ids.iter().any(|__i| *__i != id) {
                                        return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                                    }
                                }
                            }
                            let __committed_owns = {
                                let __db = self.inner.read().unwrap();
                                __db.#field.#ident.get(&__uk).is_some_and(|__ids| __ids.contains(&id))
                            };
                            if !__committed_owns && self.staged_unique_keys.contains(&(#fname, __uk.clone())) {
                                return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                            }
                            self.staged_unique_keys.insert((#fname, __uk));
                        }
                    }
                })
                .collect();

            // FK checks for ConcurrentTxHandle.
            let fk_checks: Vec<_> = model
                .fields
                .iter()
                .filter_map(|f| {
                    let (target_name, optional) = match &f.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => return None,
                    };
                    let target = schema.find_model(target_name)?;
                    if !Self::is_uuid_pk(target) {
                        return None;
                    }
                    let target_get_fn = format_ident!("get_{}", Self::to_snake_case(&target.name));
                    let fk_field = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let tname = target.name.as_str();
                    Some(if optional {
                        quote! {
                            if let Some(__fk) = record.#fk_field {
                                if self.#target_get_fn(__fk).is_none() {
                                    return Err(TxError::Validation(ValidationError::DanglingReference {
                                        field: #fname, target: #tname,
                                    }));
                                }
                            }
                        }
                    } else {
                        quote! {
                            if self.#target_get_fn(record.#fk_field).is_none() {
                                return Err(TxError::Validation(ValidationError::DanglingReference {
                                    field: #fname, target: #tname,
                                }));
                            }
                        }
                    })
                })
                .collect();

            concurrent_model_methods.push(quote! {
                /// Stage the creation of a #model_name in a concurrent transaction.
                pub fn #create_fn(&mut self, record: #model_name) -> Result<#id_type, TxError> {
                    #validate_fn(&record)?;
                    #(#unique_checks_insert)*
                    #(#fk_checks)*
                    let id = record.#id_field;
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes.clone(), __bytes.clone(), false));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes, __bytes));
                    Ok(id)
                }

                /// Stage an update of a #model_name in a concurrent transaction.
                pub fn #update_fn(&mut self, id: #id_type, record: #model_name) -> Result<bool, TxError> {
                    if self.#get_fn(id).is_none() {
                        return Ok(false);
                    }
                    #validate_fn(&record)?;
                    #(#unique_checks_update)*
                    #(#fk_checks)*
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes.clone(), __bytes.clone(), false));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes, __bytes));
                    Ok(true)
                }

                /// Stage a delete of a #model_name in a concurrent transaction.
                pub fn #delete_fn(&mut self, id: #id_type) -> bool {
                    let record = match self.#get_fn(id) {
                        Some(r) => r,
                        None => return false,
                    };
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes.clone(), __bytes.clone(), true));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __bytes));
                    true
                }

                /// Read a #model_name within the concurrent transaction (read-your-writes).
                pub fn #get_fn(&self, id: #id_type) -> Option<#model_name> {
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
                        if __ib == &__id_bytes {
                            if *__del { return None; }
                            return serde_json::from_slice(__bytes).ok();
                        }
                    }
                    let __db = self.inner.read().unwrap();
                    __db.#field.get_at(&self.snap.#snap_field, id)
                }

                /// Read all live #model_name records visible to this concurrent transaction.
                pub fn #all_fn(&self) -> Vec<#model_name> {
                    let __db = self.inner.read().unwrap();
                    let mut __rows = __db.#field.all_at(&self.snap.#snap_field);
                    drop(__db);
                    for (_, _kind, __ib, __bytes, __del) in &self.buffer {
                        let __id_bytes_ref: &Vec<u8> = __ib;
                        if *__del {
                            __rows.retain(|r| {
                                serde_json::to_vec(&r.#id_field).unwrap_or_default().as_slice() != __id_bytes_ref.as_slice()
                            });
                        } else if let Ok(__rec) = serde_json::from_slice::<#model_name>(__bytes) {
                            let __staged_id = __rec.#id_field;
                            if let Some(__pos) = __rows.iter().position(|r| r.#id_field == __staged_id) {
                                __rows[__pos] = __rec;
                            } else {
                                __rows.push(__rec);
                            }
                        }
                    }
                    __rows
                }
            });
        }

        quote! {
            /// A cloneable, thread-safe handle for concurrent transaction preparation
            /// (#83 Tier 2).
            ///
            /// Multiple threads may prepare transactions concurrently — each calls
            /// `transaction_concurrent` which holds the inner `RwLock` in **read mode**
            /// only during snapshot-capture, then releases it entirely during prepare.
            /// The serialized commit section takes the **exclusive write lock** to apply
            /// the buffer atomically.
            ///
            /// Create via `Database::shared()`.
            #[derive(Clone)]
            pub struct SharedDatabase {
                inner: std::sync::Arc<std::sync::RwLock<Database>>,
                /// Sequencer Arc cloned from Database.seq at construction.
                seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
            }

            impl SharedDatabase {
                /// Run a transaction with genuine concurrent prepare (#83 Tier 2).
                ///
                /// The closure receives a `ConcurrentTxHandle` that stages writes into
                /// **private in-memory buffers** — NO shared state is modified during
                /// prepare.  The commit critical section takes the exclusive write lock
                /// briefly to apply the buffer and advance visibility.
                pub fn transaction_concurrent<T>(
                    &self,
                    retries: u32,
                    f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    let __seq_arc = std::sync::Arc::clone(&self.seq);
                    for __attempt in 0..=retries {
                        // Step 1: register read snapshot (sequencer only, no write lock).
                        let __snap_lsn = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.register_snapshot()
                        };
                        // Step 2: capture committed watermarks (brief read lock).
                        let __snap = {
                            let __db = self.inner.read().unwrap();
                            __db.snapshot()
                        };
                        // Step 3: run the prepare closure with private-buffer staging.
                        // NO lock held during prepare.
                        let mut __tx = ConcurrentTxHandle {
                            inner: std::sync::Arc::clone(&self.inner),
                            snap: __snap,
                            buffer: Vec::new(),
                            staged_unique_keys: std::collections::BTreeSet::new(),
                            pending_events: Vec::new(),
                        };
                        let __out = f(&mut __tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };
                        // Step 4: build the opaque write-set (logical id keys + unique claims).
                        // Identity red line: the sequencer sees only opaque bytes.
                        let mut __ws_keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
                        for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                            let mut k = Vec::with_capacity(__model.len() + __id_bytes.len());
                            k.extend_from_slice(__model.as_bytes());
                            k.extend_from_slice(__id_bytes);
                            __ws_keys.push(k.into_boxed_slice());
                        }
                        for (__fname, __ekey) in &__tx.staged_unique_keys {
                            let mut k = Vec::with_capacity(__fname.len() + __ekey.len());
                            k.extend_from_slice(__fname.as_bytes());
                            k.extend_from_slice(__ekey.as_bytes());
                            __ws_keys.push(k.into_boxed_slice());
                        }
                        let __ws = forgedb_txn::WriteSet { keys: __ws_keys, snapshot_lsn: __snap_lsn };
                        // Step 5: serialized conflict check (sequencer mutex only, no RwLock).
                        let __outcome = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.try_commit(&__ws)
                        };
                        match __outcome {
                            forgedb_txn::CommitOutcome::Committed(_l) => {
                                // Step 6: apply the private buffer under exclusive write lock.
                                {
                                    let mut __db = self.inner.write().unwrap();
                                    let __buf = __tx.buffer;
                                    let __evts = __tx.pending_events;
                                    __db.__apply_and_commit_concurrent_buffer(__buf, __evts)?;
                                }
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                let _ = __attempt;
                                return Ok(__val);
                            }
                            forgedb_txn::CommitOutcome::Conflict { .. } => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                // Loop continues to next attempt.
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }
            }

            /// Private-buffer transaction handle for concurrent preparation (#83 Tier 2).
            ///
            /// Unlike `TxHandle` (which stages into shared columns via `__stage_append`),
            /// `ConcurrentTxHandle` stages into in-memory buffers that touch NO shared
            /// state during prepare.
            pub struct ConcurrentTxHandle {
                inner: std::sync::Arc<std::sync::RwLock<Database>>,
                /// Snapshot watermarks for reads (captured at transaction start).
                snap: DatabaseSnapshot,
                /// Staged rows: (model_tag, kind, id_bytes, record_bytes, deleted).
                buffer: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool)>,
                /// Claimed unique keys (field_name, encoded_key).
                staged_unique_keys: std::collections::BTreeSet<(&'static str, String)>,
                /// Pending events: (model_tag, kind, id_bytes, record_bytes).
                pending_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>)>,
            }

            impl ConcurrentTxHandle {
                #(#concurrent_model_methods)*
            }

            impl Database {
                /// Create a cloneable shared handle for concurrent transactions (#83 Tier 2).
                pub fn shared(self) -> SharedDatabase {
                    let seq = std::sync::Arc::clone(&self.seq);
                    SharedDatabase {
                        inner: std::sync::Arc::new(std::sync::RwLock::new(self)),
                        seq,
                    }
                }

                /// Apply a concurrent-prepare private buffer under the exclusive write lock
                /// (#83 Tier 2 apply path).
                ///
                /// The model-tag dispatch here is in the APPLY path (after conflict
                /// resolution passed) — same pattern as `apply_frame` for the follower.
                ///
                /// Returns the physical `(model_tag_bytes, row_index)` for each row appended
                /// so the Tier 3 caller can forward correct positions to the coordinator's
                /// `_replication.log` (T3-3/T3-8 contract).  Tier 2 callers discard the
                /// `Ok` value via `?;` — the call-site text is byte-identical (T3-5).
                pub(crate) fn __apply_and_commit_concurrent_buffer(
                    &mut self,
                    buffer: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool)>,
                    events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>)>,
                ) -> Result<Vec<(Vec<u8>, u64)>, TxError> {
                    let mut __marks: std::collections::BTreeMap<&'static str, usize> =
                        std::collections::BTreeMap::new();
                    let mut __wal_marks: std::collections::BTreeMap<&'static str, u64> =
                        std::collections::BTreeMap::new();

                    // Mark all touched models in_transaction and record pre-apply row counts.
                    for (__model_tag, _kind, _id_bytes, _bytes, _deleted) in &buffer {
                        match *__model_tag {
                            #(#concurrent_mark_arms)*
                            _ => {}
                        }
                    }

                    // Apply all staged rows via __stage_append.
                    let mut __staged_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, usize, Vec<u8>)> = Vec::new();
                    for (__model_tag, __kind, _id_bytes, __bytes, __deleted) in &buffer {
                        let __result: Result<(), TxError> = (|| {
                            match *__model_tag {
                                #(#concurrent_apply_arms)*
                                _ => {}
                            }
                            Ok(())
                        })();
                        if let Err(e) = __result {
                            // Rollback all touched models on partial apply failure.
                            let __touched: Vec<&'static str> = __marks.keys().copied().collect();
                            for __tag in &__touched {
                                match *__tag {
                                    #(#concurrent_rollback_arms)*
                                    _ => {}
                                }
                            }
                            return Err(e);
                        }
                    }

                    // Commit: fsync + journal + reindex + events.
                    let mut __journal: Vec<(String, u64)> = Vec::new();
                    let __touched: Vec<&'static str> = __marks.keys().copied().collect();
                    for __tag in &__touched {
                        match *__tag {
                            #(#concurrent_fsync_arms)*
                            _ => {}
                        }
                    }
                    if let Some(__jrnl) = self._txn_journal.as_mut() {
                        let __payload = serde_json::to_vec(&__journal)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl
                            .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
                    }
                    for __tag in &__touched {
                        match *__tag {
                            #(#concurrent_reindex_arms)*
                            _ => {}
                        }
                    }
                    // Drain events to changefeed + durable broker; collect (tag, row) pairs
                    // for the Tier 3 coordinator `Committed` payload (T3-3/T3-8).
                    let mut __row_index_pairs: Vec<(Vec<u8>, u64)> = Vec::with_capacity(__staged_events.len());
                    for (__model, __kind, __row, __bytes) in __staged_events {
                        __row_index_pairs.push((__model.as_bytes().to_vec(), __row as u64));
                        self.changefeed.emit(__model, __row, __kind);
                        if let Some(__broker) = &self.broker {
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(__model, __row as u64, __kind, __bytes);
                            }
                        }
                    }
                    let _ = events;
                    Ok(__row_index_pairs)
                }
            }
        }
    }

    /// Generate one model's scoped write + read methods on `TxHandle` (MVCC
    /// Tier 1, #83).  Each write records the collection's rollback mark on first
    /// touch, runs the SAME #91 validators (field constraints, `&unique` against
    /// the committed index, FK existence against txn-visible target rows), stages
    /// the row via `__stage_append`, and buffers the changefeed/broker event.
    /// Each read resolves against the raised (staged) watermark for read-your-writes.
    fn generate_txn_model_methods(
        model: &forgedb_parser::Model,
        schema: &Schema,
    ) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let model_tag = model.name.as_str();
        let snake = Self::to_snake_case(&model.name);
        let field = format_ident!("{}", snake);
        let id_type = Self::id_type_tokens(model);
        let id_field = Self::id_field_ident(model);
        let mark_fn = format_ident!("__mark_{}", snake);
        let create_fn = format_ident!("create_{}", snake);
        let update_fn = format_ident!("update_{}", snake);
        let delete_fn = format_ident!("delete_{}", snake);
        let get_fn = format_ident!("get_{}", snake);
        let all_fn = format_ident!("all_{}", snake);
        let validate_fn = format_ident!("validate_{}", snake);

        // `&unique` checks: (1) against the committed index (same as the non-txn
        // path) and (2) against the staged-unique buffer (`staged_unique_keys`) so
        // two `create_<model>` calls with the SAME `&unique` value in ONE transaction
        // both fail — the committed index only reflects pre-txn rows.
        // On success, insert the key into `staged_unique_keys` so subsequent staged
        // writes can see it.  Rollback discards the set (it never touches real maps).
        let unique_checks_insert: Vec<_> = Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let key = Self::index_key_expr(Self::index_value_expr(
                    &f.field_type,
                    quote! { record.#fident },
                ));
                quote! {
                    {
                        let __uk: String = { #key };
                        // Check committed index.
                        if self.db.#field.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                            return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                        }
                        // Check staged-key buffer (intra-txn duplicate guard, M1a fix).
                        if self.staged_unique_keys.contains(&(#fname, __uk.clone())) {
                            return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                        }
                        // Claim the key for this transaction.
                        self.staged_unique_keys.insert((#fname, __uk));
                    }
                }
            })
            .collect();
        let unique_checks_update: Vec<_> = Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let key = Self::index_key_expr(Self::index_value_expr(
                    &f.field_type,
                    quote! { record.#fident },
                ));
                quote! {
                    {
                        let __uk: String = { #key };
                        // Check committed index (exclude self-id).
                        if let Some(__ids) = self.db.#field.#ident.get(&__uk) {
                            if __ids.iter().any(|__i| *__i != id) {
                                return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                            }
                        }
                        // Check staged-key buffer (intra-txn duplicate guard, M1a fix).
                        // An update that keeps its own unique value is fine: the
                        // staged_unique_keys set does NOT track the id owning each key,
                        // so we can only block a staged update whose NEW value was
                        // already claimed by a DIFFERENT staged write.  A prior
                        // staged insert of the same id already put this key in the
                        // buffer; an update by the same txn to a different value
                        // should be blocked only if the new value was claimed by
                        // someone else.  We use a conservative check: if the key is
                        // in the staged buffer and the committed index does NOT
                        // already hold it under this id, it was claimed by another
                        // staged write.
                        let __committed_owns = self.db.#field.#ident
                            .get(&__uk)
                            .is_some_and(|__ids| __ids.contains(&id));
                        if !__committed_owns
                            && self.staged_unique_keys.contains(&(#fname, __uk.clone()))
                        {
                            return Err(TxError::Validation(ValidationError::Unique { field: #fname }));
                        }
                        // Claim the key for this transaction.
                        self.staged_unique_keys.insert((#fname, __uk));
                    }
                }
            })
            .collect();

        // FK-existence checks (txn-aware): each FK must resolve to a row VISIBLE to
        // the transaction (committed OR staged in this txn), so a create-parent +
        // create-child in one transaction is valid.  Reads through the scoped
        // `get_<target>` so it sees the target's staged rows.
        let fk_checks: Vec<_> = model
            .fields
            .iter()
            .filter_map(|f| {
                let (target_name, optional) = match &f.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => (t, false),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => (t, true),
                    _ => return None,
                };
                let target = schema.find_model(target_name)?;
                if !Self::is_uuid_pk(target) {
                    return None;
                }
                let target_get = format_ident!("get_{}", Self::to_snake_case(&target.name));
                let fk_field = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let tname = target.name.as_str();
                Some(if optional {
                    quote! {
                        if let Some(__fk) = record.#fk_field {
                            if self.#target_get(__fk).is_none() {
                                return Err(TxError::Validation(ValidationError::DanglingReference {
                                    field: #fname, target: #tname,
                                }));
                            }
                        }
                    }
                } else {
                    quote! {
                        if self.#target_get(record.#fk_field).is_none() {
                            return Err(TxError::Validation(ValidationError::DanglingReference {
                                field: #fname, target: #tname,
                            }));
                        }
                    }
                })
            })
            .collect();

        let create_doc = format!(
            "Stage the creation of a {} in this transaction (#83).  Validates field \
             constraints + `&unique` (committed index) + FK existence (txn-visible \
             targets), then stages the row; visible only after `commit`.",
            model.name
        );
        let update_doc = format!(
            "Stage an update of a {} in this transaction (#83).  `Ok(false)` if the \
             id is not visible to the transaction.",
            model.name
        );
        let delete_doc = format!(
            "Stage a delete of a {} in this transaction (#83).  Appends a tombstoned \
             version; `false` if the id is not visible to the transaction.",
            model.name
        );
        let get_doc = format!(
            "Read a {} within the transaction (#83) with read-your-writes: resolves \
             the newest version over the committed prefix ∪ this txn's staged rows.",
            model.name
        );
        let all_doc = format!(
            "Read every live {} within the transaction (#83), including this txn's \
             own staged rows (read-your-writes).",
            model.name
        );

        quote! {
            #[doc = #create_doc]
            pub fn #create_fn(&mut self, record: #model_name) -> Result<#id_type, TxError> {
                self.#mark_fn();
                // #91 field constraints.
                #validate_fn(&record)?;
                // #91 `&unique` (committed index; within-txn duplicate = honest limit).
                #(#unique_checks_insert)*
                // #91 FK existence, txn-visible.
                #(#fk_checks)*
                let id = record.#id_field;
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                // Opaque row bytes for the buffered broker record (same encoding the
                // WAL / #82 broker journal — never a decoded field).
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                // Stage the row (WAL + columns + tombstone; NO index/feed/broker).
                let __row = self.db.#field.__stage_append(record, false);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes, __row, __bytes));
                Ok(id)
            }

            #[doc = #update_doc]
            pub fn #update_fn(&mut self, id: #id_type, record: #model_name) -> Result<bool, TxError> {
                // Existence within the txn (committed ∪ staged) via read-your-writes.
                if self.#get_fn(id).is_none() {
                    return Ok(false);
                }
                self.#mark_fn();
                #validate_fn(&record)?;
                #(#unique_checks_update)*
                #(#fk_checks)*
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                let __row = self.db.#field.__stage_append(record, false);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes, __row, __bytes));
                Ok(true)
            }

            #[doc = #delete_doc]
            pub fn #delete_fn(&mut self, id: #id_type) -> bool {
                // Resolve the current (txn-visible) record to re-append under the
                // tombstone and to supply the broker bytes.
                let record = match self.#get_fn(id) {
                    Some(r) => r,
                    None => return false,
                };
                self.#mark_fn();
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                let __row = self.db.#field.__stage_append(record, true);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __row, __bytes));
                true
            }

            #[doc = #get_doc]
            pub fn #get_fn(&self, id: #id_type) -> Option<#model_name> {
                // Read-your-writes: `get_at` with the watermark raised to the current
                // physical (staged) length resolves the newest version per id over
                // the committed prefix ∪ this txn's staged rows — one decode path.
                let __snap = forgedb_storage::Snapshot::new(self.db.#field.row_count);
                self.db.#field.get_at(&__snap, id)
            }

            #[doc = #all_doc]
            pub fn #all_fn(&self) -> Vec<#model_name> {
                let __snap = forgedb_storage::Snapshot::new(self.db.#field.row_count);
                self.db.#field.all_at(&__snap)
            }
        }
    }

    /// Generate the persisted junction storage struct for each M2M relation.
    ///
    /// Each junction stores two `Uuid` columns (`left` = model1 id, `right` =
    /// model2 id) plus a per-pair `Tombstones` column (1 byte/row) for
    /// `unlink` (delete semantics): `link` appends a pair (`false` tombstone),
    /// `unlink` appends a NEW retracted pair (`true` tombstone) — append-only, the
    /// same retraction primitive models use for `delete` (#66).  `pairs()` filters
    /// out any pair that is retracted or superseded by a later retraction, so
    /// traversal never resolves an unlinked edge.  Uses only the already-published
    /// `Tombstones` type — no substrate change.
    fn generate_junction_structs(schema: &Schema) -> TokenStream {
        let mut structs = Vec::new();

        for m in Self::valid_m2m(schema) {
            let struct_ident = Self::junction_struct_ident(&m);
            let reader_ident = format_ident!("{}Reader", struct_ident);
            let reader_doc = format!(
                "Read-only, lock-free reader handle for the {}<->{} junction (#56 Direction B)",
                m.model1, m.model2
            );
            let field_snake = Self::to_snake_case(&m.model1);
            let field_snake2 = Self::to_snake_case(&m.model2);
            let struct_doc = format!(
                "Junction table for the {}.{} <-> {}.{} many-to-many relation",
                m.model1, m.field1, m.model2, m.field2
            );
            let base = format!("{}_{}_link", field_snake, field_snake2);
            let left_path = format!("{}/fixed/left.bin", base);
            let right_path = format!("{}/fixed/right.bin", base);
            let tombstones_path = format!("{}/tombstones.bin", base);
            let manifest_path = format!("{}/manifest.json", base);

            structs.push(quote! {
                #[doc = #struct_doc]
                pub struct #struct_ident {
                    left_col: FixedColumn,
                    right_col: FixedColumn,
                    /// Per-pair retraction (delete semantics): `true` = unlinked.
                    /// Appended in lockstep with `left`/`right` (append-only).
                    tombstones: Tombstones,
                    row_count: usize,
                    changefeed: Option<forgedb_changefeed::ChangeFeed>,
                    broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                }

                impl #struct_ident {
                    /// Open the junction storage, rehydrating `row_count` from the
                    /// persisted columns so M2M links survive a process restart
                    /// (#65).  `right_col` is appended last in `link`, so its length
                    /// is the count of fully-committed pairs; a torn link (left
                    /// written, right not) is excluded and overwritten by the next.
                    pub fn new() -> Self {
                        Self::new_at(std::path::Path::new("."))
                    }

                    /// Open the junction storage rooted at `root` (#59): both uuid
                    /// columns and the manifest are joined under it, so a per-tenant
                    /// process's M2M links live under that tenant's dir.
                    pub fn new_at(root: &std::path::Path) -> Self {
                        let left_col = FixedColumn::new(
                            root.join(#left_path),
                            16usize,
                        ).expect("Failed to create junction column");
                        let right_col = FixedColumn::new(
                            root.join(#right_path),
                            16usize,
                        ).expect("Failed to create junction column");
                        let mut tombstones = Tombstones::new(
                            root.join(#tombstones_path),
                        ).expect("Failed to create junction tombstones");
                        // `right_col` is appended last per `link`, so its length is
                        // the count of fully-committed pairs.  Reconcile the
                        // tombstone column to that count: back-fill `false` for any
                        // pre-tombstone-format rows or a torn tail (tombstone not
                        // written for the last link) so `is_deleted(i)` is defined
                        // for every committed pair.
                        let row_count = right_col.len();
                        while tombstones.len() < row_count {
                            tombstones
                                .append(false)
                                .expect("Failed to back-fill junction tombstone");
                        }
                        let db = Self {
                            left_col,
                            right_col,
                            tombstones,
                            row_count,
                            changefeed: None,
                            broker: None,
                        };
                        db.write_manifest(root);
                        db
                    }

                    /// Attach a shared change feed (#62): `link` then emits a
                    /// field-blind `(junction_name, row_index)` signal.
                    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
                        self.changefeed = Some(feed);
                    }

                    /// Attach the shared durable replication broker (#82): `link`
                    /// then durably records the pair (opaque bytes) at a global offset.
                    pub fn attach_broker(
                        &mut self,
                        broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                    ) {
                        self.broker = broker;
                    }

                    /// Write the junction's physical-layout manifest (#57): two
                    /// 16-byte uuid columns and a row-count anchor on `right.bin`
                    /// (appended last per link, 16 bytes/row).  Layout only —
                    /// carries no relation semantics.  Best-effort; reopen anchors
                    /// on the file length regardless.
                    fn write_manifest(&self, root: &std::path::Path) {
                        let columns = vec![
                            forgedb_storage::ColumnMetadata {
                                name: "left".to_string(),
                                column_type: forgedb_storage::ColumnType::Uuid,
                                column_index: 0usize,
                                value_size: 16usize,
                                kind: forgedb_storage::ColumnKind::Fixed,
                                relative_path: "fixed/left.bin".to_string(),
                            },
                            forgedb_storage::ColumnMetadata {
                                name: "right".to_string(),
                                column_type: forgedb_storage::ColumnType::Uuid,
                                column_index: 1usize,
                                value_size: 16usize,
                                kind: forgedb_storage::ColumnKind::Fixed,
                                relative_path: "fixed/right.bin".to_string(),
                            },
                        ];
                        let __manifest_abs = root.join(#manifest_path);
                        // Preserve the incremental-backup chain token (#76) across
                        // reopen, same as the model path (junctions are not compacted
                        // today, so this stays 0 — but the invariant is uniform).
                        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
                            .map(|m| m.compaction_epoch)
                            .unwrap_or(0);
                        // Preserve the on-disk format version across reopen (#74
                        // Phase 1), same as the model path — a reopen must never
                        // clobber a migration-bumped version.  Fresh dir → this
                        // binary's `EXPECTED_FORMAT_VERSION` baseline.
                        let __format_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
                            .map(|m| m.format_version)
                            .unwrap_or(EXPECTED_FORMAT_VERSION);
                        let manifest = forgedb_storage::Manifest {
                            schema_version: 1,
                            row_count: self.row_count,
                            columns,
                            wal_enabled: false,
                            // Observability only (#89 step 2): rows durable on disk at open
                    // (== row_count — recovery already realigned the columns).  NOT
                    // load-bearing for recovery, which reads the column lengths.
                    last_checkpoint: self.row_count as u64,
                            compaction_epoch: __compaction_epoch,
                            format_version: __format_version,
                            row_anchor: Some(forgedb_storage::RowAnchor {
                                relative_path: "fixed/right.bin".to_string(),
                                bytes_per_row: 16usize,
                            }),
                        };
                        let _ = manifest.save_to(&__manifest_abs);
                    }

                    /// Record a link between a `left` (model1) id and a `right`
                    /// (model2) id.
                    pub fn link(&mut self, left: Uuid, right: Uuid) {
                        let row_index = self.row_count;
                        self.left_col.append_uuid(*left.as_bytes())
                            .expect("Failed to append link");
                        self.right_col.append_uuid(*right.as_bytes())
                            .expect("Failed to append link");
                        // A live link — tombstone `false`, appended in lockstep.
                        self.tombstones.append(false)
                            .expect("Failed to append junction tombstone");
                        self.row_count += 1;
                        // Change-feed emit (#62): a link is an M2M junction append.
                        if let Some(feed) = &self.changefeed {
                            feed.emit(#base, row_index, forgedb_changefeed::ChangeKind::Linked);
                        }
                        // Durable replication record (#82): the link pair as opaque
                        // bytes (left ++ right, 32 bytes) at a global offset.
                        if let Some(__broker) = &self.broker {
                            let mut __row_bytes = Vec::with_capacity(32);
                            __row_bytes.extend_from_slice(left.as_bytes());
                            __row_bytes.extend_from_slice(right.as_bytes());
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(
                                    #base,
                                    row_index as u64,
                                    forgedb_changefeed::ChangeKind::Linked,
                                    __row_bytes,
                                );
                            }
                        }
                    }

                    /// Remove the link between `left` and `right` (delete
                    /// semantics — the reverse of `link`).  Append-only retraction:
                    /// a new row for the pair with tombstone `true`, so `pairs()`
                    /// (latest-wins per pair) stops resolving it.  Returns `true` if
                    /// a live link existed, `false` if the pair was not linked.
                    /// Re-`link` after `unlink` restores the edge (a later live row
                    /// supersedes the retraction).
                    pub fn unlink(&mut self, left: Uuid, right: Uuid) -> bool {
                        // Is the pair currently live?  Latest-wins scan.
                        let mut __live = false;
                        for i in 0..self.row_count {
                            let l = Uuid::from_bytes(
                                self.left_col.read_uuid(i).expect("Failed to read link"),
                            );
                            let r = Uuid::from_bytes(
                                self.right_col.read_uuid(i).expect("Failed to read link"),
                            );
                            if l == left && r == right {
                                __live = !self.tombstones.is_deleted(i).unwrap_or(false);
                            }
                        }
                        if !__live {
                            return false;
                        }
                        let row_index = self.row_count;
                        self.left_col.append_uuid(*left.as_bytes())
                            .expect("Failed to append unlink");
                        self.right_col.append_uuid(*right.as_bytes())
                            .expect("Failed to append unlink");
                        self.tombstones.append(true)
                            .expect("Failed to append junction tombstone");
                        self.row_count += 1;
                        if let Some(feed) = &self.changefeed {
                            feed.emit(#base, row_index, forgedb_changefeed::ChangeKind::Deleted);
                        }
                        if let Some(__broker) = &self.broker {
                            let mut __row_bytes = Vec::with_capacity(32);
                            __row_bytes.extend_from_slice(left.as_bytes());
                            __row_bytes.extend_from_slice(right.as_bytes());
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(
                                    #base,
                                    row_index as u64,
                                    forgedb_changefeed::ChangeKind::Deleted,
                                    __row_bytes,
                                );
                            }
                        }
                        true
                    }

                    /// Retract every live pair whose `left` id equals `id` (M2M
                    /// cascade-unlink when the left model's row is deleted).
                    pub fn unlink_all_left(&mut self, id: Uuid) {
                        let __targets: Vec<Uuid> = self
                            .pairs()
                            .into_iter()
                            .filter(|(l, _)| *l == id)
                            .map(|(_, r)| r)
                            .collect();
                        for r in __targets {
                            self.unlink(id, r);
                        }
                    }

                    /// Retract every live pair whose `right` id equals `id` (M2M
                    /// cascade-unlink when the right model's row is deleted).
                    pub fn unlink_all_right(&mut self, id: Uuid) {
                        let __targets: Vec<Uuid> = self
                            .pairs()
                            .into_iter()
                            .filter(|(_, r)| *r == id)
                            .map(|(l, _)| l)
                            .collect();
                        for l in __targets {
                            self.unlink(l, id);
                        }
                    }

                    /// Make this junction's id columns durable (#89 step 2).  M2M
                    /// junctions have no WAL (a #89 boundary — link appends are not
                    /// yet crash-recovered), so there is nothing to truncate; this
                    /// only fsyncs the columns so a `Database::checkpoint()`
                    /// leaves link rows as durable as model rows.
                    pub fn checkpoint(&mut self) {
                        self.left_col
                            .flush()
                            .expect("Failed to fsync junction left column on checkpoint");
                        self.right_col
                            .flush()
                            .expect("Failed to fsync junction right column on checkpoint");
                        self.tombstones
                            .flush()
                            .expect("Failed to fsync junction tombstones on checkpoint");
                    }

                    /// Every LIVE (left, right) id pair (latest-wins per pair):
                    /// a pair is live iff its most-recent row is not retracted, so
                    /// `unlink`ed edges are excluded and a re-`link` restores it.
                    pub fn pairs(&self) -> Vec<(Uuid, Uuid)> {
                        self.pairs_prefix(self.row_count)
                    }

                    /// Shared latest-wins resolution over the row prefix `0..end`.
                    fn pairs_prefix(&self, end: usize) -> Vec<(Uuid, Uuid)> {
                        // Preserve first-link order while applying latest-wins on
                        // the retraction flag.
                        let mut order: Vec<(Uuid, Uuid)> = Vec::new();
                        let mut state: std::collections::HashMap<(Uuid, Uuid), bool> =
                            std::collections::HashMap::new();
                        for i in 0..end {
                            let left = Uuid::from_bytes(
                                self.left_col.read_uuid(i).expect("Failed to read link"),
                            );
                            let right = Uuid::from_bytes(
                                self.right_col.read_uuid(i).expect("Failed to read link"),
                            );
                            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
                            if !state.contains_key(&(left, right)) {
                                order.push((left, right));
                            }
                            state.insert((left, right), deleted);
                        }
                        order
                            .into_iter()
                            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
                            .collect()
                    }

                    /// The number of committed link rows (the snapshot watermark).
                    pub fn row_count(&self) -> usize {
                        self.row_count
                    }

                    /// Capture a read snapshot at the current link count (#56).
                    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
                        forgedb_storage::Snapshot::new(self.row_count)
                    }

                    /// Live link pairs committed as of `snap` (#56): latest-wins
                    /// over the prefix `0..snap.watermark()`, excluding links
                    /// appended after the snapshot AND pairs retracted within it.
                    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)> {
                        self.pairs_prefix(snap.watermark())
                    }

                    /// Open a read-only handle over this junction (#56 Direction B):
                    /// shares both id columns + tombstones via independent fds for
                    /// lock-free snapshot reads concurrent with the writer's `link`.
                    pub fn reader(&self) -> #reader_ident {
                        #reader_ident {
                            left_col: self.left_col.reader()
                                .expect("Failed to open junction left reader"),
                            right_col: self.right_col.reader()
                                .expect("Failed to open junction right reader"),
                            tombstones: self.tombstones.reader()
                                .expect("Failed to open junction tombstones reader"),
                        }
                    }
                }

                #[doc = #reader_doc]
                pub struct #reader_ident {
                    left_col: forgedb_storage::FixedColumnReader,
                    right_col: forgedb_storage::FixedColumnReader,
                    tombstones: forgedb_storage::TombstonesReader,
                }

                impl #reader_ident {
                    /// Live link pairs committed as of `snap` (#56 Direction B): the
                    /// same latest-wins prefix resolution as the writer junction,
                    /// reading through the shared-fd reader columns.
                    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(Uuid, Uuid)> {
                        let end = snap.watermark();
                        let mut order: Vec<(Uuid, Uuid)> = Vec::new();
                        let mut state: std::collections::HashMap<(Uuid, Uuid), bool> =
                            std::collections::HashMap::new();
                        for i in 0..end {
                            let left = Uuid::from_bytes(
                                self.left_col.read_uuid(i).expect("Failed to read link"),
                            );
                            let right = Uuid::from_bytes(
                                self.right_col.read_uuid(i).expect("Failed to read link"),
                            );
                            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
                            if !state.contains_key(&(left, right)) {
                                order.push((left, right));
                            }
                            state.insert((left, right), deleted);
                        }
                        order
                            .into_iter()
                            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
                            .collect()
                    }
                }
            });
        }

        quote! { #(#structs)* }
    }

    /// Generate the relation-traversal `impl Database` block: forward FK getters,
    /// reverse one-to-many collection getters, and M2M link/query helpers.
    ///
    /// All generated method names are pushed through a single `seen` set so a
    /// pathological schema can never produce two methods with the same name
    /// (which would fail to compile); a collision is skipped rather than emitted.
    fn generate_traversal_impl(schema: &Schema) -> TokenStream {
        use std::collections::{HashMap, HashSet};

        let mut methods = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // --- A. Forward FK getters (`*Target` / `?Target`) --------------------
        for model in &schema.models {
            let model_snake = Self::to_snake_case(&model.name);
            for field in &model.fields {
                let (target_name, optional) = match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => (t, false),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => (t, true),
                    _ => continue,
                };
                // Only when the target exists and is UUID-keyed.
                let Some(target) = schema.find_model(target_name) else {
                    continue;
                };
                if !Self::is_uuid_pk(target) {
                    continue;
                }

                let method_name = format!("{}_{}", model_snake, field.name);
                if !seen.insert(method_name.clone()) {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let model_ident = format_ident!("{}", model.name);
                let target_ident = format_ident!("{}", target.name);
                let target_field = format_ident!("{}", Self::to_snake_case(&target.name));
                let fk_field = format_ident!("{}", field.name);

                if optional {
                    methods.push(quote! {
                        #[doc = "Resolve the optional foreign key to its record."]
                        pub fn #method_ident(&self, record: &#model_ident) -> Option<#target_ident> {
                            record.#fk_field.and_then(|fk| self.#target_field.get(fk))
                        }
                    });
                } else {
                    methods.push(quote! {
                        #[doc = "Resolve the foreign key to its record."]
                        pub fn #method_ident(&self, record: &#model_ident) -> Option<#target_ident> {
                            self.#target_field.get(record.#fk_field)
                        }
                    });
                }
            }
        }

        // --- B. Reverse one-to-many collection getters ------------------------
        // Group by (parent_model, parent_field) so that a parent whose child has
        // multiple FKs back to it disambiguates by child field name.
        let pairs = schema.detect_relations();
        let mut group_counts: HashMap<(String, String), usize> = HashMap::new();
        for p in &pairs {
            *group_counts
                .entry((p.parent_model.clone(), p.parent_field.clone()))
                .or_default() += 1;
        }
        for p in &pairs {
            let Some(parent) = schema.find_model(&p.parent_model) else {
                continue;
            };
            if !Self::is_uuid_pk(parent) {
                continue;
            }

            let ambiguous = group_counts
                .get(&(p.parent_model.clone(), p.parent_field.clone()))
                .is_some_and(|&c| c > 1);
            let method_name = if ambiguous {
                format!(
                    "{}_{}_by_{}",
                    Self::to_snake_case(&p.parent_model),
                    p.parent_field,
                    p.child_field
                )
            } else {
                format!("{}_{}", Self::to_snake_case(&p.parent_model), p.parent_field)
            };
            if !seen.insert(method_name.clone()) {
                continue;
            }
            let method_ident = format_ident!("{}", method_name);
            let child_ident = format_ident!("{}", p.child_model);
            let child_field = format_ident!("{}", Self::to_snake_case(&p.child_model));
            let fk_probe = format_ident!("find_by_{}", p.child_field);

            // The FK column is now indexed (#100): probe the child's FK index in
            // O(1) instead of scanning `all()` — but only when the child model has
            // an identity (id/auto) field, since `indexed_fields` (and thus the
            // probe) is empty otherwise; fall back to the scan in that case.
            let child_indexed = schema
                .find_model(&p.child_model)
                .is_some_and(|c| c.fields.iter().any(|f| f.name == "id" || f.auto_generate));

            let doc = if child_indexed {
                format!(
                    "All {} whose {} references the given id (#100: O(1) FK-index probe, not a scan).",
                    p.child_model, p.child_field
                )
            } else {
                format!("All {} whose {} references the given id.", p.child_model, p.child_field)
            };

            let body = if child_indexed {
                // Required FK probe takes `Uuid`; optional FK probe takes `Option<Uuid>`.
                let arg = if p.is_required {
                    quote! { id }
                } else {
                    quote! { Some(id) }
                };
                quote! { self.#child_field.#fk_probe(#arg) }
            } else {
                let fk_field = format_ident!("{}", p.child_field);
                let predicate = if p.is_required {
                    quote! { record.#fk_field == id }
                } else {
                    quote! { record.#fk_field == Some(id) }
                };
                quote! {
                    self.#child_field
                        .all()
                        .into_iter()
                        .filter(|record| #predicate)
                        .collect()
                }
            };

            methods.push(quote! {
                #[doc = #doc]
                pub fn #method_ident(&self, id: Uuid) -> Vec<#child_ident> {
                    #body
                }
            });
        }

        // --- C. Many-to-many link + query helpers -----------------------------
        for m in Self::valid_m2m(schema) {
            let junction_field = Self::junction_field_ident(&m);
            let model1_ident = format_ident!("{}", m.model1);
            let model2_ident = format_ident!("{}", m.model2);
            let model1_storage = format_ident!("{}", Self::to_snake_case(&m.model1));
            let model2_storage = format_ident!("{}", Self::to_snake_case(&m.model2));

            // link_<a>_<b>
            let link_name = format!(
                "link_{}_{}",
                Self::to_snake_case(&m.model1),
                Self::to_snake_case(&m.model2)
            );
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                let doc = format!(
                    "Link a {} (left) and a {} (right) in the junction table.",
                    m.model1, m.model2
                );
                // Fixed `left`/`right` param names — model-derived names would
                // collide for a self-referential M2M (model1 == model2).
                methods.push(quote! {
                    #[doc = #doc]
                    pub fn #link_ident(&mut self, left: Uuid, right: Uuid) {
                        self.#junction_field.link(left, right);
                    }
                });
            }

            // unlink_<a>_<b> (delete semantics — reverse of link_<a>_<b>)
            let unlink_name = format!(
                "unlink_{}_{}",
                Self::to_snake_case(&m.model1),
                Self::to_snake_case(&m.model2)
            );
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                let junction_field_u = Self::junction_field_ident(&m);
                let doc = format!(
                    "Remove the link between a {} (left) and a {} (right).  Returns \
                     `true` if a live link existed.  Traversal (`{}_{}` / `{}_{}`) \
                     stops resolving the pair afterwards.",
                    m.model1, m.model2,
                    Self::to_snake_case(&m.model1), m.field1,
                    Self::to_snake_case(&m.model2), m.field2
                );
                methods.push(quote! {
                    #[doc = #doc]
                    pub fn #unlink_ident(&mut self, left: Uuid, right: Uuid) -> bool {
                        self.#junction_field_u.unlink(left, right)
                    }
                });
            }

            // model1.field1 -> Vec<model2>
            let fwd_name = format!("{}_{}", Self::to_snake_case(&m.model1), m.field1);
            if seen.insert(fwd_name.clone()) {
                let fwd_ident = format_ident!("{}", fwd_name);
                let doc = format!("All linked {} for the given {} id.", m.model2, m.model1);
                methods.push(quote! {
                    #[doc = #doc]
                    pub fn #fwd_ident(&self, id: Uuid) -> Vec<#model2_ident> {
                        self.#junction_field
                            .pairs()
                            .into_iter()
                            .filter(|(left, _)| *left == id)
                            .filter_map(|(_, right)| self.#model2_storage.get(right))
                            .collect()
                    }
                });

                // Snapshot-scoped variant of the same forward M2M query (#56,
                // Direction A milestone).  Demonstrates cross-table snapshot
                // consistency: the junction is clamped to *its* watermark
                // (`pairs_at`) and each resolved target to *its* watermark
                // (`get_at`), so links or target rows appended after the snapshot
                // was captured are excluded on both sides of the join.
                let fwd_at_name = format!(
                    "{}_{}_at",
                    Self::to_snake_case(&m.model1),
                    m.field1
                );
                if seen.insert(fwd_at_name.clone()) {
                    let fwd_at_ident = format_ident!("{}", fwd_at_name);
                    let junction_field2 = junction_field.clone();
                    let doc = format!(
                        "All linked {} for the given {} id, consistent as of `snap`.",
                        m.model2, m.model1
                    );
                    methods.push(quote! {
                        #[doc = #doc]
                        pub fn #fwd_at_ident(
                            &self,
                            snap: &DatabaseSnapshot,
                            id: Uuid,
                        ) -> Vec<#model2_ident> {
                            self.#junction_field2
                                .pairs_at(&snap.#junction_field2)
                                .into_iter()
                                .filter(|(left, _)| *left == id)
                                .filter_map(|(_, right)| {
                                    self.#model2_storage.get_at(&snap.#model2_storage, right)
                                })
                                .collect()
                        }
                    });
                }
            }

            // model2.field2 -> Vec<model1>
            let rev_name = format!("{}_{}", Self::to_snake_case(&m.model2), m.field2);
            if seen.insert(rev_name.clone()) {
                let rev_ident = format_ident!("{}", rev_name);
                let doc = format!("All linked {} for the given {} id.", m.model1, m.model2);
                methods.push(quote! {
                    #[doc = #doc]
                    pub fn #rev_ident(&self, id: Uuid) -> Vec<#model1_ident> {
                        self.#junction_field
                            .pairs()
                            .into_iter()
                            .filter(|(_, right)| *right == id)
                            .filter_map(|(left, _)| self.#model1_storage.get(left))
                            .collect()
                    }
                });
            }
        }

        if methods.is_empty() {
            return quote! {};
        }

        quote! {
            impl Database {
                #(#methods)*
            }
        }
    }

    /// Generate the snapshot-scoped forward M2M traversal on `DatabaseReader`
    /// (#56 Direction B).  Mirrors the `<model1>_<field1>_at` method emitted on
    /// `Database`, but bound to the read-only reader bundle: `pairs_at` on the
    /// junction reader + `get_at` on the target model reader, both clamped to the
    /// same writer-captured `DatabaseSnapshot` for a cross-table-consistent join.
    /// Only the snapshot-scoped `_at` variant is emitted — a `DatabaseReader`
    /// never exposes the non-snapshot getters (they consult `get`/`pairs`, which
    /// a read-only handle intentionally lacks).
    fn generate_reader_traversal_impl(schema: &Schema) -> TokenStream {
        let mut methods = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for m in Self::valid_m2m(schema) {
            let junction_field = Self::junction_field_ident(&m);
            let model2_ident = format_ident!("{}", m.model2);
            let model2_storage = format_ident!("{}", Self::to_snake_case(&m.model2));

            let fwd_at_name = format!("{}_{}_at", Self::to_snake_case(&m.model1), m.field1);
            if seen.insert(fwd_at_name.clone()) {
                let fwd_at_ident = format_ident!("{}", fwd_at_name);
                let doc = format!(
                    "All linked {} for the given {} id, consistent as of `snap` (reader).",
                    m.model2, m.model1
                );
                methods.push(quote! {
                    #[doc = #doc]
                    pub fn #fwd_at_ident(
                        &self,
                        snap: &DatabaseSnapshot,
                        id: Uuid,
                    ) -> Vec<#model2_ident> {
                        self.#junction_field
                            .pairs_at(&snap.#junction_field)
                            .into_iter()
                            .filter(|(left, _)| *left == id)
                            .filter_map(|(_, right)| {
                                self.#model2_storage.get_at(&snap.#model2_storage, right)
                            })
                            .collect()
                    }
                });
            }
        }

        if methods.is_empty() {
            return quote! {};
        }

        quote! {
            impl DatabaseReader {
                #(#methods)*
            }
        }
    }

    /// Generate `<Model>WithRelations` structs and their eager-load getters.
    ///
    /// For every model with at least one forward foreign key to a UUID-keyed
    /// target, emit a struct bundling the base record with each resolved
    /// reference (`Option<Target>`, since the referent may be missing), and a
    /// `<model>_with_relations(id)` getter that populates them in one call.  The
    /// getter's `id` parameter uses the model's own PK type, so integer-PK models
    /// are supported as long as their FK *targets* are UUID-keyed.
    fn generate_eager_load(schema: &Schema) -> TokenStream {
        let mut items = Vec::new();

        for model in &schema.models {
            // Collect forward FKs to UUID-keyed targets, preserving field order.
            let fks: Vec<(&forgedb_parser::Field, &str, bool)> = model
                .fields
                .iter()
                .filter_map(|field| match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => Some((field, t.as_str(), false)),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => Some((field, t.as_str(), true)),
                    _ => None,
                })
                .filter(|(_, target, _)| {
                    schema.find_model(target).is_some_and(Self::is_uuid_pk)
                })
                .collect();

            if fks.is_empty() {
                continue;
            }

            let model_ident = format_ident!("{}", model.name);
            let base_field = Self::to_snake_case(&model.name);
            let base_ident = format_ident!("{}", base_field);
            let struct_ident = format_ident!("{}WithRelations", model.name);
            let getter_ident = format_ident!("{}_with_relations", base_field);
            let id_type = Self::id_type_tokens(model);
            let struct_doc = format!("{} with its forward references resolved", model.name);

            // Choose a non-colliding field name for each resolved reference:
            // strip a trailing `_id`, falling back to the raw FK name on clash.
            let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
            used.insert(base_field.clone());

            let mut struct_fields = Vec::new();
            let mut resolvers = Vec::new();
            let mut resolved_idents = Vec::new();

            for (field, target, optional) in fks {
                let stripped = field
                    .name
                    .strip_suffix("_id")
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&field.name)
                    .to_string();
                let resolved_name = if used.contains(&stripped) {
                    field.name.clone()
                } else {
                    stripped
                };
                // If even the raw name collides, skip this reference rather than
                // emit a duplicate struct field.
                if !used.insert(resolved_name.clone()) {
                    continue;
                }

                let resolved_ident = format_ident!("{}", resolved_name);
                let target_ident = format_ident!("{}", target);
                let target_storage = format_ident!("{}", Self::to_snake_case(target));
                let fk_ident = format_ident!("{}", field.name);

                struct_fields.push(quote! {
                    pub #resolved_ident: Option<#target_ident>
                });

                if optional {
                    resolvers.push(quote! {
                        let #resolved_ident = #base_ident.#fk_ident
                            .and_then(|fk| self.#target_storage.get(fk));
                    });
                } else {
                    resolvers.push(quote! {
                        let #resolved_ident = self.#target_storage.get(#base_ident.#fk_ident);
                    });
                }
                resolved_idents.push(resolved_ident);
            }

            items.push(quote! {
                #[doc = #struct_doc]
                #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
                pub struct #struct_ident {
                    pub #base_ident: #model_ident,
                    #(#struct_fields,)*
                }

                impl Database {
                    #[doc = "Load a record together with its forward references."]
                    pub fn #getter_ident(&self, id: #id_type) -> Option<#struct_ident> {
                        let #base_ident = self.#base_ident.get(id)?;
                        #(#resolvers)*
                        Some(#struct_ident { #base_ident, #(#resolved_idents),* })
                    }
                }
            });
        }

        quote! { #(#items)* }
    }

    /// Map ForgeDB field type to Rust type identifier for use with quote!
    ///
    /// Note: for `OptionalStructType` and `Nullable`, this returns the *inner* type
    /// token.  The `Option<>` wrapper is applied by callers that check `field.is_nullable()`.
    fn map_field_type_ident(field_type: &forgedb_parser::FieldType) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::U32 => quote! { u32 },
            forgedb_parser::FieldType::U64 => quote! { u64 },
            forgedb_parser::FieldType::I32 => quote! { i32 },
            forgedb_parser::FieldType::I64 => quote! { i64 },
            forgedb_parser::FieldType::F64 => quote! { f64 },
            forgedb_parser::FieldType::Bool => quote! { bool },
            forgedb_parser::FieldType::String => quote! { String },
            forgedb_parser::FieldType::Json => quote! { serde_json::Value },
            forgedb_parser::FieldType::Decimal => quote! { rust_decimal::Decimal },
            forgedb_parser::FieldType::Enum(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::Uuid => quote! { Uuid },
            forgedb_parser::FieldType::Timestamp => quote! { Timestamp },
            forgedb_parser::FieldType::StructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::OptionalStructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::Nullable(inner) => {
                // Return the inner type; is_nullable() causes the Option<> wrapper to be added
                Self::map_field_type_ident(inner)
            }
            forgedb_parser::FieldType::Char(size) => {
                quote! { [u8; #size] }
            }
            forgedb_parser::FieldType::FixedArray(inner, count) => {
                let inner_type = Self::map_field_type_ident(inner);
                quote! { [#inner_type; #count] }
            }
            forgedb_parser::FieldType::Relation(rel) => {
                match rel {
                    forgedb_parser::RelationType::RequiredReference(_) => quote! { Uuid },
                    forgedb_parser::RelationType::OptionalReference(_) => quote! { Uuid },
                    _ => quote! { () }, // Virtual fields
                }
            }
            _ => quote! { String }, // Default fallback
        }
    }

    /// Convert PascalCase to snake_case
    pub(crate) fn to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }

    /// Generate the Tier 3 coordinator client surface (#84, MVCC multi-process).
    ///
    /// Emits `CoordinatedDatabase` and `Database::connect` — additive to the
    /// existing method bodies (the only non-additive edit is factoring `open_at`
    /// into the lock-free-capable `__open_with_lock` core the coordinated open
    /// reuses; the single-writer commit/transaction bodies stay byte-identical —
    /// T3-5).
    ///
    /// ## What is generated
    ///
    /// ```rust,ignore
    /// pub struct CoordinatedDatabase {
    ///     shared: SharedDatabase,
    ///     coordinator: std::sync::Arc<forgedb_coordinator::client::CoordinatorClient>,
    /// }
    ///
    /// impl CoordinatedDatabase {
    ///     pub fn transaction_coordinated<T>(
    ///         &self,
    ///         retries: u32,
    ///         f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
    ///     ) -> Result<T, TxError> { ... }
    /// }
    ///
    /// impl Database {
    ///     pub fn connect(
    ///         root: std::path::PathBuf,
    ///         socket_path: std::path::PathBuf,
    ///     ) -> Result<CoordinatedDatabase, TxError> { ... }
    /// }
    /// ```
    ///
    /// ## Identity red line
    ///
    /// `CoordinatedDatabase` sends only opaque byte keys to the coordinator and
    /// calls the SAME `__apply_and_commit_concurrent_buffer` as `SharedDatabase`
    /// for the data-plane write — no second apply path, no drift.  The
    /// coordinator itself never receives a decoded field.
    fn generate_coordinated_client(schema: &Schema) -> TokenStream {
        // Only generate if there are transactable models.
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        // Per-model peer-refresh arms: re-derive EVERY column's row_count from the
        // shared on-disk files, then rebuild id_to_row + secondary indexes.
        // `__sync_columns_from_disk` syncs all data columns + the tombstone (via
        // the additive `*::sync_from_disk` substrate) — NOT just the tombstone: a
        // peer's row is only readable once every column's bound is refreshed, so a
        // tombstone-only sync would leave `get`/`read_at` reading out of bounds.
        // T3-8: reads shared columns via the existing storage fds; writes no column
        // (no append/flush). The coordinator never calls this — it is generated
        // data-plane code, satisfying T3-8.
        let peer_refresh_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    self.#field.__sync_columns_from_disk();
                    self.#field.__reindex_committed();
                }
            })
            .collect();

        quote! {
            /// Tier 3 MVCC multi-process coordinator client (#84).
            ///
            /// Wraps a [`SharedDatabase`] (Tier 2 in-process concurrency) and
            /// routes the serialized commit section through a running
            /// `forgedb coordinate <root>` coordinator process over a Unix socket,
            /// enabling multiple OS processes to share a single data directory
            /// without corruption.
            ///
            /// Create via [`Database::connect`] (connects to the coordinator, then
            /// opens the data dir lock-free — the coordinator holds the #89 lock).
            ///
            /// Native-only: `forgedb-coordinator` uses Unix sockets + advisory
            /// file locks, so the whole Tier-3 coordinated surface is gated off
            /// the `wasm32` build (the browser read-replica is a read-only
            /// follower — it never coordinates multi-process writes).
            #[cfg(not(target_arch = "wasm32"))]
            pub struct CoordinatedDatabase {
                shared: SharedDatabase,
                coordinator: std::sync::Arc<forgedb_coordinator::client::CoordinatorClient>,
                /// The coordinator LSN this writer has refreshed peer commits up to.
                /// When `coordinator.last_known_lsn()` exceeds this, a peer refresh
                /// is performed before the next prepare (peer read-currency, #84).
                last_refreshed_lsn: std::sync::atomic::AtomicU64,
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl CoordinatedDatabase {
                /// Run a transaction through the Tier 3 commit coordinator.
                ///
                /// ## Commit flow
                ///
                /// 0. **Peer refresh** (if coordinator LSN advanced): sync tombstone
                ///    counts from disk + rebuild id_to_row/indexes under write lock.
                ///    This is how a writer sees commits made by other processes while
                ///    it was prepare-ing — peer read-currency via #56-B shared column
                ///    files, driven by the coordinator log offset as the signal.
                ///    T3-8: the coordinator never writes a column; this refresh is
                ///    generated data-plane code reading the shared files.
                /// 1. Capture read snapshot (brief `RwLock` read, sequencer snapshot).
                /// 2. Run prepare closure with `ConcurrentTxHandle` — no lock held.
                /// 3. Build opaque write-set keys (same as Tier 2, identity red line).
                /// 4. **`RequestTurn`** → coordinator: conflict-check + LSN assignment.
                ///    - On `Grant`: proceed to data-plane write.
                ///    - On `Nack` (conflict): release snapshot, retry from step 0.
                ///    - On `Busy`: brief back-off then retry step 4.
                /// 5. Data-plane write: `__apply_and_commit_concurrent_buffer` under
                ///    the exclusive in-process `RwLock`.  Same path as Tier 2.
                ///    Returns the physical `(model_tag_bytes, row_index)` pairs for
                ///    each row appended (T3-3/T3-8: correct positions in the log).
                /// 6. **`Committed`** → coordinator: hand opaque row bytes + REAL
                ///    row indices for the durable replication log, receive `Ack { lsn }`.
                /// 7. Release snapshot; return the prepared value.
                pub fn transaction_coordinated<T>(
                    &self,
                    retries: u32,
                    f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    use forgedb_changefeed::ChangeKind as __CK;
                    use std::sync::atomic::Ordering;
                    let __coord = std::sync::Arc::clone(&self.coordinator);
                    let __seq_arc = std::sync::Arc::clone(&self.shared.seq);
                    let __inner = std::sync::Arc::clone(&self.shared.inner);

                    let mut __last_lsn = __coord.last_known_lsn();

                    for __attempt in 0..=retries {
                        // Step 0 – peer read-currency: if the coordinator has advanced
                        // past our last-refreshed offset, a peer committed rows we have
                        // not yet reflected in our in-memory id_to_row / indexes.
                        // Refresh them now by re-reading the shared column files (T3-8:
                        // generated data-plane code; coordinator has no column dep).
                        {
                            let __coord_lsn = __coord.last_known_lsn();
                            let __refreshed = self.last_refreshed_lsn.load(Ordering::Acquire);
                            if __coord_lsn > __refreshed {
                                let mut __db = __inner.write().unwrap();
                                __db.__peer_refresh();
                                self.last_refreshed_lsn.store(__coord_lsn, Ordering::Release);
                            }
                        }

                        // Step 1 – register snapshot LSN (local; coordinator is the authority).
                        let __snap_lsn = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.register_snapshot()
                        };

                        // Step 2 – capture committed watermarks (brief read lock).
                        let __snap = {
                            let __db = __inner.read().unwrap();
                            __db.snapshot()
                        };

                        // Step 3 – prepare closure with private-buffer staging (no lock held).
                        let mut __tx = ConcurrentTxHandle {
                            inner: std::sync::Arc::clone(&__inner),
                            snap: __snap,
                            buffer: Vec::new(),
                            staged_unique_keys: std::collections::BTreeSet::new(),
                            pending_events: Vec::new(),
                        };
                        let __out = f(&mut __tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };

                        // Step 4 – build opaque write-set (identity red line: coordinator sees bytes only).
                        let mut __ws_keys: Vec<Vec<u8>> = Vec::new();
                        for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                            let mut k = Vec::with_capacity(__model.len() + __id_bytes.len());
                            k.extend_from_slice(__model.as_bytes());
                            k.extend_from_slice(__id_bytes);
                            __ws_keys.push(k);
                        }
                        for (__fname, __ekey) in &__tx.staged_unique_keys {
                            let mut k = Vec::with_capacity(__fname.len() + __ekey.len());
                            k.extend_from_slice(__fname.as_bytes());
                            k.extend_from_slice(__ekey.as_bytes());
                            __ws_keys.push(k);
                        }

                        // Step 5 – RequestTurn: coordinator conflict-check + LSN assignment.
                        let __turn = {
                            let mut __busy_retries = 0u32;
                            loop {
                                match __coord.request_turn(__ws_keys.clone(), __last_lsn) {
                                    Ok(grant) => break Ok(grant),
                                    Err(forgedb_coordinator::client::ClientError::Conflict { conflict_key }) => {
                                        break Err(forgedb_txn::CommitOutcome::Conflict {
                                            key: conflict_key.into_boxed_slice(),
                                        });
                                    }
                                    Err(forgedb_coordinator::client::ClientError::Busy) => {
                                        if __busy_retries >= 5 {
                                            break Err(forgedb_txn::CommitOutcome::Conflict {
                                                key: b"__busy__".to_vec().into_boxed_slice(),
                                            });
                                        }
                                        __busy_retries += 1;
                                        std::thread::sleep(std::time::Duration::from_millis(20 * __busy_retries as u64));
                                    }
                                    Err(e) => {
                                        let mut __seq = __seq_arc.lock().unwrap();
                                        __seq.release_snapshot(__snap_lsn);
                                        return Err(TxError::Io(e.to_string()));
                                    }
                                }
                            }
                        };

                        match __turn {
                            Err(_) => {
                                // Conflict or busy exhaustion — retry.
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                __last_lsn = __coord.last_known_lsn();
                                continue;
                            }
                            Ok((__turn_id, _reserved_lsn)) => {
                                // Save buffer metadata before moving buffer into apply.
                                let __kinds_copy: Vec<u8> = __tx.buffer
                                    .iter()
                                    .map(|(_, k, _, _, _)| k.to_byte())
                                    .collect();
                                let __bytes_copy: Vec<Vec<u8>> = __tx.buffer
                                    .iter()
                                    .map(|(_, _, _, b, _)| b.clone())
                                    .collect();

                                // Step 6 – data-plane write (same path as Tier 2).
                                // __apply_and_commit_concurrent_buffer now returns
                                // Vec<(model_tag_bytes, row_index)> — the real physical
                                // positions for the coordinator's replication log (T3-3/T3-8).
                                let __apply_result = {
                                    let mut __db = __inner.write().unwrap();
                                    __db.__apply_and_commit_concurrent_buffer(
                                        __tx.buffer,
                                        __tx.pending_events,
                                    )
                                };

                                // Step 7 – Committed: hand opaque bytes + REAL row indices.
                                // Build coordinator payload (opaque; coordinator never decodes).
                                let (__model_tags, __row_indices) = match &__apply_result {
                                    Ok(__pairs) => {
                                        let tags: Vec<Vec<u8>> = __pairs.iter().map(|(t, _)| t.clone()).collect();
                                        let rows: Vec<u64> = __pairs.iter().map(|(_, r)| *r).collect();
                                        (tags, rows)
                                    }
                                    Err(_) => {
                                        // Apply failed — still send Committed with empty payload
                                        // so the coordinator releases the turn (T3-5: data-plane
                                        // error must not wedge the coordinator turn slot).
                                        (Vec::new(), Vec::new())
                                    }
                                };
                                let __ack = __coord.committed(
                                    __turn_id,
                                    __model_tags,
                                    __row_indices,
                                    __kinds_copy,
                                    __bytes_copy,
                                );
                                match __ack {
                                    Ok(__lsn) => { __last_lsn = __lsn; }
                                    Err(e) => {
                                        // Best-effort: the commit is already durable (columns
                                        // + WAL fsynced before Committed); a missed ack only
                                        // leaves `__last_lsn` briefly stale.  Dependency-free
                                        // diagnostic so the generated crate needs no `log` dep.
                                        eprintln!("coordinator: Committed ack error: {e}");
                                    }
                                }

                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);

                                return __apply_result.map(|_| __val);
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl Database {
                /// Peer read-currency refresh for Tier 3 multi-process writers (#84).
                ///
                /// Re-reads the on-disk tombstone byte counts (the row-count anchor)
                /// for each model, updates the in-memory `row_count`, then rebuilds
                /// `id_to_row` + secondary indexes from the shared column files.
                ///
                /// Called under the exclusive write lock (`RwLock`) before each
                /// `transaction_coordinated` prepare step whenever the coordinator's
                /// committed LSN has advanced past `last_refreshed_lsn` — meaning a
                /// peer process committed rows that this writer has not yet seen.
                ///
                /// T3-8 contract: this method reads shared column files via the
                /// #56-B substrate (`sync_from_disk` / `__reindex_committed`); it
                /// NEVER writes a column.  The coordinator has no `forgedb-storage*`
                /// dependency and never calls this.
                pub(crate) fn __peer_refresh(&mut self) {
                    #(#peer_refresh_arms)*
                }

                /// Open a data dir as a **coordinated Tier-3 client** of a running
                /// `forgedb coordinate <root>` coordinator, returning a
                /// [`CoordinatedDatabase`] that lets multiple OS processes share one
                /// data directory safely (#84).
                ///
                /// ## Lock discipline (MVCC spec T3-5 / PM re-gate #84 — load-bearing)
                ///
                /// This connects to the coordinator **FIRST** and only then opens the
                /// data dir **lock-free** (`_lock: None`): the coordinator holds the
                /// #89 `DirLock` on `<root>/.forgedb.lock` on behalf of every client,
                /// and write mutual-exclusion among coordinated clients is the
                /// coordinator's serialized turn-grant — NOT the file lock.  If the
                /// coordinator is unreachable, this returns
                /// [`TxError::CoordinatorUnavailable`] and **no lock-free open
                /// happens**, so there is never an unserialized lock-free writer
                /// (the binding PM constraint).  Contrast `open_at`, which takes the
                /// `DirLock` itself for the standalone single-writer path — the two
                /// modes are mutually exclusive because both contend for the same
                /// lock file.
                pub fn connect(
                    root: std::path::PathBuf,
                    socket_path: std::path::PathBuf,
                ) -> Result<CoordinatedDatabase, TxError> {
                    // Establish the live coordinator turn-channel BEFORE any
                    // lock-free open, so a socket-configured-but-coordinator-down
                    // misconfiguration surfaces as CoordinatorUnavailable rather
                    // than silently opening a second unserialized writer.
                    let coordinator = forgedb_coordinator::client::CoordinatorClient::connect(&socket_path)
                        .map_err(|e| TxError::CoordinatorUnavailable(e.to_string()))?;
                    // Coordinator is live and holds the #89 DirLock for us → open
                    // WITHOUT taking the lock ourselves (T3-5).
                    let shared = Self::__open_with_lock(root, None).shared();
                    Ok(CoordinatedDatabase {
                        shared,
                        coordinator: std::sync::Arc::new(coordinator),
                        last_refreshed_lsn: std::sync::atomic::AtomicU64::new(0),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::ast::IndexType;
    use forgedb_parser::{Field, FieldType, Model, Schema};

    #[test]
    fn test_rust_generation_with_quote() {
        // Create a simple schema
        let schema = Schema {
            models: vec![Model {
                name: "User".to_string(),
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "email".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: true,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "age".to_string(),
                        field_type: FieldType::OptionalStructType("Age".to_string()),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            }],
            structs: vec![],
            enums: vec![],
        };

        let result = RustGenerator::generate(&schema).unwrap();
        let code = result.code;

        // Verify the generated code contains expected elements (formatted by prettyplease)
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub id: Uuid"));
        assert!(code.contains("pub email: String"));
        assert!(code.contains("pub age: Option<Age>"));
        assert!(code.contains("pub struct UserStorage"));
        assert!(code.contains("pub struct Database"));
        assert!(code.contains("pub user: UserStorage"));
        assert!(code.contains("impl UserStorage"));
        assert!(code.contains("pub fn new"));
        assert!(code.contains("pub fn insert"));
        assert!(code.contains("pub fn get"));

        // Verify it compiles as valid Rust (basic syntax check)
        assert!(code.contains("use std::collections::HashMap"));
        assert!(code.contains("use forgedb_types::{Uuid, Timestamp, Value}"));
    }

    #[test]
    fn test_multiple_models() {
        let schema = Schema {
            models: vec![
                Model {
                    name: "User".to_string(),
                    fields: vec![Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    }],
                    composite_indexes: vec![],
                    projections: Vec::new(),
                    soft_delete: false,
                },
                Model {
                    name: "Post".to_string(),
                    fields: vec![Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    }],
                    composite_indexes: vec![],
                    projections: Vec::new(),
                    soft_delete: false,
                },
            ],
            structs: vec![],
            enums: vec![],
        };

        let result = RustGenerator::generate(&schema).unwrap();
        let code = result.code;

        // Verify formatted output
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub struct UserStorage"));
        assert!(code.contains("pub struct PostStorage"));
        assert!(code.contains("pub user: UserStorage"));
        assert!(code.contains("pub post: PostStorage"));
    }
}
