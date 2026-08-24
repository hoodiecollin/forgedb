//! Golang binding generator — a class-2 transport target (RFC #203, sibling of
//! the PyO3/NAPI/WASM bindings under the #122 taxonomy).
//!
//! Emits, per schema, an idiomatic Go package (`forgedb.go` + a `forgedb.h` C
//! header) that calls the SAME generated native FFI C-ABI (`crates/codegen/src/
//! ffi.rs`) over cgo. It adds **no new substrate dep**, which keeps it off the
//! publish-gap critical path.
//!
//! # The C symbols this file names carry the app's prefix (#335 §2, decision 9)
//!
//! Every `forgedb_…` C symbol here became `<symbol_prefix><stem>`, from
//! `forgedb::naming::symbol_prefix` — the ONE definition, threaded in by the
//! caller and never re-derived. `ffi.rs` DEFINES those symbols and this file
//! DECLARES (`forgedb.h`) and CALLS (`forgedb.go`) them, so the three move
//! together or the Go binding fails to link.
//!
//! The one symbol emitted by *this* side rather than by `ffi.rs` is the cgo
//! `//export`ed completion callback in `forgedb_async.go`; it is prefixed for
//! the same reason, because `//export` makes it an external C symbol too.
//!
//! **Identity (class-2 transport glue).** Like the PyO3/NAPI wrappers, this file
//! is *generated per schema*: schema-tailored Go structs + typed methods that
//! reference the generated per-model C symbols **by name** (that IS the
//! tailoring). It invents **no** generic query surface — rows and ids cross cgo
//! as OPAQUE JSON bytes (the same opaque-bytes discipline as every other binding)
//! and are decoded into the generated Go struct via `encoding/json` at a
//! compile-time-known type. There is no runtime schema read, no `forgedb_query`,
//! no `switch model` dispatch (the removed-`QueryBuilder` red line).
//!
//! **Surface (at NAPI-RS parity).** Per-model CRUD (`Insert`/`Get`/`Count`/
//! `All`/`Update`/`Delete`), point-in-time `_at` reads over a `Snapshot`,
//! relation traversal (forward FK, reverse 1:M, M2M link/unlink + query getters,
//! incl. the M2M `_at` snapshot getter), **async CRUD** over the cgo completion
//! bridge (`forgedb_async.go`), and **Arrow columnar export** (`forgedb_arrow.go`,
//! zero-copy via `github.com/apache/arrow-go/v18`).
//!
//! Field-shape fidelity: scalars map to their Go equivalents; enums map to a
//! generated Go enum type + consts and inline structs to generated nested Go
//! structs; `json`, `char(N)`, fixed arrays, and virtual relation-collection
//! fields map to `json.RawMessage` (a lossless JSON passthrough that round-trips
//! whatever serde emits), so the Go struct matches the generated `database.rs`
//! serde shape exactly.

use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, RelationType, Schema};
use std::collections::{HashMap, HashSet};

/// Generates the Go cgo binding for a schema.
pub struct GoGenerator;

/// One id-bearing model exposed over the Go binding (CRUD + snapshot reads).
pub(crate) struct GoModel {
    /// PascalCase model name (also the Go struct name).
    pub(crate) name: String,
    /// snake_case name — the `<prefix><snake>_<op>` C-symbol infix.
    pub(crate) snake: String,
    /// Go type of the model's identity field (`string` for uuid PKs).
    pub(crate) id_go: String,
}

/// A relation-traversal C entry point mirrored one-for-one from the FFI
/// generator (`generate_relation_ops`). `sym` is the C-symbol suffix (without
/// the app's prefix); `method` is its PascalCase Go method name.
pub(crate) enum GoRelOp {
    /// Forward FK: resolve `*Target`/`?Target` by the source id → `*Target`.
    ForwardFk {
        sym: String,
        method: String,
        source_id_go: String,
        target: String,
    },
    /// Reverse 1:M query getter: all records for a parent id → `[]Target`. The
    /// id is typed as the PARENT's own identity type (#266).
    Vec {
        sym: String,
        method: String,
        id_go: String,
        target: String,
    },
    /// Snapshot-scoped M2M query getter: all linked records for an id as of
    /// a `*Snapshot` → `[]Target`.
    VecAt {
        sym: String,
        method: String,
        id_go: String,
        target: String,
    },
    /// M2M link (each endpoint's own id type) → `error`.
    Link {
        sym: String,
        method: String,
        left_go: String,
        right_go: String,
    },
    /// M2M unlink (each endpoint's own id type) → `(bool, error)`.
    Unlink {
        sym: String,
        method: String,
        left_go: String,
        right_go: String,
    },
}

/// One Arrow-exportable column: the `Export<Model><Field>Arrow` Go method and its
/// `<snake>_<field>_export_arrow` C symbol suffix (below the app's prefix).
pub(crate) struct ArrowCol {
    pub(crate) sym: String,
    pub(crate) method: String,
}

impl GoGenerator {
    /// Generate the `forgedb.go` package for a schema.
    pub fn generate(
        schema: &Schema,
        symbol_prefix: &str,
        fingerprint: &str,
    ) -> Result<GeneratedCode> {
        let models = Self::crud_models(schema);
        let rel_ops = Self::relation_ops(schema);

        let mut code = String::new();
        code.push_str(&subst(FILE_HEADER, symbol_prefix));
        code.push_str(&Self::fingerprint_check(symbol_prefix, fingerprint));
        code.push_str(&subst(SPINE, symbol_prefix));
        code.push_str(&subst(ASYNC_SPINE, symbol_prefix));

        // --- Generated enum + inline-struct types ---
        code.push_str(&Self::enum_types(schema));
        code.push_str(&Self::struct_types(schema));

        // --- Per-model row structs ---
        for model in schema.models.iter() {
            code.push_str(&Self::model_struct(schema, model));
        }

        // --- Per-model CRUD + snapshot reads ---
        for m in &models {
            code.push_str(&Self::crud_methods(m, symbol_prefix));
        }

        // --- Per-model async CRUD (over the FFI completion bridge) ---
        for m in &models {
            code.push_str(&Self::async_methods(m, symbol_prefix));
        }

        // --- Relation traversal ---
        for op in &rel_ops {
            code.push_str(&Self::relation_method(op, symbol_prefix));
        }

        Ok(GeneratedCode {
            description: format!(
                "Go cgo binding ({} models, {} relation ops)",
                models.len(),
                rel_ops.len()
            ),
            code,
        })
    }

    /// The Arrow-exportable columns (same filter as the FFI Arrow ops): each
    /// id-bearing model's non-null fixed-width primitive / uuid / required-FK
    /// columns.
    pub(crate) fn arrow_columns(schema: &Schema) -> Vec<ArrowCol> {
        let mut cols = Vec::new();
        for model in schema
            .models
            .iter()
            .filter(|m| m.has_identity())
        {
            let snake = RustGenerator::to_snake_case(&model.name);
            for field in &model.fields {
                if RustGenerator::arrow_export_format(schema, &field.field_type).is_some() {
                    cols.push(ArrowCol {
                        sym: format!("{snake}_{}_export_arrow", field.name),
                        method: format!(
                            "Export{}{}Arrow",
                            model.name,
                            to_pascal_case(&field.name)
                        ),
                    });
                }
            }
        }
        cols
    }

    /// The package-`init()` fingerprint check.
    ///
    /// **The fingerprint compared here is the `ffi` package's, not this
    /// directory's.** `<output>/go/` is not a cache package and is not hashed;
    /// the archive this links IS the `ffi` package's artifact, so the `ffi`
    /// package's value is the only one that can agree with what the archive
    /// exports. A later reader will want to "fix" this into a per-directory
    /// hash; that breaks the comparison.
    ///
    /// What this buys over the linker is narrower than "load-time verification"
    /// suggests, and the narrow version is the honest one: a schema change that
    /// removes an exported symbol ALREADY fails to link. This adds (a) a readable
    /// message where the linker's names a mangled C symbol, and (b) the cases the
    /// linker cannot see at all — a `[storage]`/`[runtime]` knob that changes
    /// durability semantics without changing one symbol name.
    fn fingerprint_check(symbol_prefix: &str, fingerprint: &str) -> String {
        format!(
            r#"
// forgedbFingerprint is the fingerprint of the generated source THIS file was
// written from. The linked archive exposes the same value; `init` below refuses
// to let the package load when they disagree.
//
// It is NOT a version: a ForgeDB upgrade that changes nothing about the emitted
// source leaves it untouched.
const forgedbFingerprint = "{fingerprint}"

func init() {{
	built := C.GoString(C.{pfx}fingerprint())
	if built != forgedbFingerprint {{
		panic("forgedb: libforgedb.a was built from a different schema than the Go package beside it" +
			"\n  this package expects: " + forgedbFingerprint +
			"\n  the archive reports:  " + built +
			"\nRun `forgedb build` to recompile it.")
	}}
}}
"#,
            pfx = symbol_prefix,
            fingerprint = fingerprint,
        )
    }

    /// Generate the `forgedb_arrow.go` companion (only when the schema has
    /// Arrow-exportable columns): per-column `Export<Model><Field>Arrow` methods
    /// that import the FFI's Arrow C-Data-Interface export into an `arrow.Array`
    /// via `arrow-go`'s `cdata`. Returns `None` (no file, no dep) when there are
    /// no exportable columns.
    ///
    /// NOTE: this is the ONE place the Go binding pulls an external module
    /// (`github.com/apache/arrow-go/v18`); the caller surfaces that to the user.
    pub fn generate_arrow(schema: &Schema, symbol_prefix: &str) -> Option<GeneratedCode> {
        let pfx = symbol_prefix;
        let cols = Self::arrow_columns(schema);
        if cols.is_empty() {
            return None;
        }
        let mut body = String::new();
        for c in &cols {
            body.push_str(&format!(
                r#"
// {method} exports the live column as an Arrow array (zero-copy mmap alias when
// the live rows are a dense prefix, a gathered copy otherwise). The caller owns
// the result and MUST call Release() on it.
func (db *DB) {method}() (arrow.Array, error) {{
	var sch C.ArrowSchema
	var arr C.ArrowArray
	var e *C.ForgeError
	if !bool(C.{pfx}{sym}(db.ptr, &sch, &arr, &e)) {{
		return nil, takeError(e)
	}}
	_, a, err := cdata.ImportCArray(
		(*cdata.CArrowArray)(unsafe.Pointer(&arr)),
		(*cdata.CArrowSchema)(unsafe.Pointer(&sch)),
	)
	return a, err
}}
"#,
                method = c.method,
                sym = c.sym,
            ));
        }
        Some(GeneratedCode {
            description: format!("Go Arrow export ({} columns)", cols.len()),
            code: format!("{}{}", subst(ARROW_FILE_HEADER, pfx), body),
        })
    }

    /// The `forgedb_async.go` companion file: the exported cgo completion
    /// callback. It MUST be separate from `forgedb.go` because cgo forbids C
    /// definitions in the preamble of any file that uses `//export`, and
    /// `forgedb.go` carries the registration shim (a definition). Schema-invariant.
    pub fn generate_async_bridge(symbol_prefix: &str) -> GeneratedCode {
        GeneratedCode {
            description: "Go async completion bridge (//export callback)".to_string(),
            code: subst(ASYNC_BRIDGE_FILE, symbol_prefix),
        }
    }

    /// A `go.mod` for the generated Go package. User-editable, so the CLI writes
    /// it ONLY when absent (like every other binding scaffold). When the schema
    /// has Arrow-exportable columns, the arrow-go require is included (run
    /// `go mod tidy` to populate `go.sum`).
    pub fn go_mod_scaffold(module: &str, needs_arrow: bool) -> String {
        let mut m = format!("module {module}\n\ngo 1.21\n");
        if needs_arrow {
            m.push_str("\nrequire github.com/apache/arrow-go/v18 v18.7.0\n");
        }
        m
    }

    /// Whether the schema exposes any Arrow-exportable column (drives the
    /// `forgedb_arrow.go` file + the arrow-go `go.mod` require).
    pub fn needs_arrow(schema: &Schema) -> bool {
        !Self::arrow_columns(schema).is_empty()
    }

    /// A `README.md` documenting the build order and cgo caveats. Written only
    /// when absent (a scaffold, like `go.mod`).
    pub fn readme_scaffold() -> &'static str {
        README_SCAFFOLD
    }

    // --- Derivation (shared by `generate` and `generate_header`) --------------

    /// The id-bearing models (same filter as the FFI per-model ops).
    pub(crate) fn crud_models(schema: &Schema) -> Vec<GoModel> {
        schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .map(|m| GoModel {
                name: m.name.clone(),
                snake: RustGenerator::to_snake_case(&m.name),
                id_go: Self::go_id_type(schema, m),
            })
            .collect()
    }

    /// Mirror `FfiGenerator::generate_relation_ops` faithfully — the SAME public
    /// schema helpers, the SAME family order and shared `seen` dedup — so every
    /// symbol we emit corresponds to a symbol the FFI generator emits. (The
    /// `test_go_calls_match_ffi_symbols` guard proves the correspondence.) The
    /// M2M snapshot `_at` getter is intentionally not surfaced in this first cut,
    /// but its name is still reserved in `seen` so later families collide
    /// identically to the FFI derivation.
    pub(crate) fn relation_ops(schema: &Schema) -> Vec<GoRelOp> {
        let mut ops = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // --- A. Forward FK getters (`*Target` / `?Target`) ---
        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_has_id = model.has_identity();
            let id_go = Self::go_id_type(schema, model);
            for field in &model.fields {
                let target_name = match &field.field_type {
                    FieldType::Relation(RelationType::RequiredReference(t))
                    | FieldType::Relation(RelationType::OptionalReference(t)) => t,
                    _ => continue,
                };
                let Some(target) = schema.find_model(target_name) else {
                    continue;
                };
                let method_name = format!("{model_snake}_{}", field.name);
                if !seen.insert(method_name.clone()) {
                    continue;
                }
                if !model_has_id {
                    continue;
                }
                ops.push(GoRelOp::ForwardFk {
                    method: to_pascal_case(&method_name),
                    sym: method_name,
                    source_id_go: id_go.clone(),
                    target: target.name.clone(),
                });
            }
        }

        // --- B. Reverse one-to-many collection getters ---
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
            let ambiguous = group_counts
                .get(&(p.parent_model.clone(), p.parent_field.clone()))
                .is_some_and(|&c| c > 1);
            let method_name = if ambiguous {
                format!(
                    "{}_{}_by_{}",
                    RustGenerator::to_snake_case(&p.parent_model),
                    p.parent_field,
                    p.child_field
                )
            } else {
                format!(
                    "{}_{}",
                    RustGenerator::to_snake_case(&p.parent_model),
                    p.parent_field
                )
            };
            if !seen.insert(method_name.clone()) {
                continue;
            }
            ops.push(GoRelOp::Vec {
                method: to_pascal_case(&method_name),
                sym: method_name,
                id_go: Self::go_id_type(schema, parent),
                target: p.child_model.clone(),
            });
        }

        // --- C. Many-to-many link / unlink + query getters ---
        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
            // #266: each junction endpoint carries its OWN identity type.
            let id1 = schema
                .find_model(&m.model1)
                .map_or_else(|| "string".to_string(), |md| Self::go_id_type(schema, md));
            let id2 = schema
                .find_model(&m.model2)
                .map_or_else(|| "string".to_string(), |md| Self::go_id_type(schema, md));

            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                ops.push(GoRelOp::Link {
                    method: to_pascal_case(&link_name),
                    sym: link_name,
                    left_go: id1.clone(),
                    right_go: id2.clone(),
                });
            }

            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                ops.push(GoRelOp::Unlink {
                    method: to_pascal_case(&unlink_name),
                    sym: unlink_name,
                    left_go: id1.clone(),
                    right_go: id2.clone(),
                });
            }

            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                ops.push(GoRelOp::Vec {
                    method: to_pascal_case(&fwd_name),
                    sym: fwd_name,
                    id_go: id1.clone(),
                    target: m.model2.clone(),
                });
                // The snapshot-scoped forward getter (inserted into `seen` at the
                // same point as the FFI derivation, so downstream families dedup
                // identically).
                let fwd_at_name = format!("{snake1}_{}_at", m.field1);
                if seen.insert(fwd_at_name.clone()) {
                    ops.push(GoRelOp::VecAt {
                        method: to_pascal_case(&fwd_at_name),
                        sym: fwd_at_name,
                        id_go: id1.clone(),
                        target: m.model2.clone(),
                    });
                }
            }

            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                ops.push(GoRelOp::Vec {
                    method: to_pascal_case(&rev_name),
                    sym: rev_name,
                    id_go: id2.clone(),
                    target: m.model1.clone(),
                });
            }
        }

        ops
    }

    // --- Rendering ------------------------------------------------------------

    fn model_struct(schema: &Schema, model: &forgedb_parser::Model) -> String {
        let mut s = format!("\n// {name} is a generated row of the `{name}` model.\ntype {name} struct {{\n", name = model.name);
        for field in &model.fields {
            let go_ty = Self::go_field_type(schema, field);
            let tag = if field.auto_generate
                && matches!(field.field_type, FieldType::Uuid | FieldType::Timestamp(_))
            {
                // `+uuid` / `+timestamp` autos carry `#[serde(default)]`, so a
                // create body may omit them (`create_*` synthesizes the value).
                format!("`json:\"{},omitempty\"`", field.name)
            } else {
                format!("`json:\"{}\"`", field.name)
            };
            s.push_str(&format!(
                "\t{} {} {}\n",
                go_field_name(&field.name),
                go_ty,
                tag
            ));
        }
        s.push_str("}\n");
        s
    }

    fn crud_methods(m: &GoModel, pfx: &str) -> String {
        let GoModel { name, snake, id_go } = m;
        format!(
            r#"
// Insert{name} inserts a {name} through the integrity gate, returning the new id.
func (db *DB) Insert{name}(rec {name}) ({id_go}, error) {{
	var zero {id_go}
	body, err := json.Marshal(rec)
	if err != nil {{
		return zero, err
	}}
	var idOut *C.uint8_t
	var idLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{snake}_insert(db.ptr, bytesPtr(body), C.size_t(len(body)), &idOut, &idLen, &e)
	if !bool(ok) {{
		return zero, takeError(e)
	}}
	idBytes := takeBuffer(idOut, idLen)
	var id {id_go}
	if err := json.Unmarshal(idBytes, &id); err != nil {{
		return zero, err
	}}
	return id, nil
}}

// Get{name} fetches a {name} by id, or (nil, nil) if it does not exist.
func (db *DB) Get{name}(id {id_go}) (*{name}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{snake}_get(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	b := takeBuffer(out, outLen)
	if b == nil {{
		return nil, nil
	}}
	var rec {name}
	if err := json.Unmarshal(b, &rec); err != nil {{
		return nil, err
	}}
	return &rec, nil
}}

// Count{name} returns the live {name} row count.
func (db *DB) Count{name}() (int64, error) {{
	var e *C.ForgeError
	n := int64(C.{pfx}{snake}_count(db.ptr, &e))
	if n < 0 {{
		return 0, takeError(e)
	}}
	return n, nil
}}

// All{name} returns every live {name} row.
func (db *DB) All{name}() ([]{name}, error) {{
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{snake}_all(db.ptr, &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	var recs []{name}
	if err := json.Unmarshal(takeBuffer(out, outLen), &recs); err != nil {{
		return nil, err
	}}
	return recs, nil
}}

// Update{name} replaces a {name} by id (whole-record). Returns false if absent.
func (db *DB) Update{name}(id {id_go}, rec {name}) (bool, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return false, err
	}}
	body, err := json.Marshal(rec)
	if err != nil {{
		return false, err
	}}
	var e *C.ForgeError
	r := int(C.{pfx}{snake}_update(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), bytesPtr(body), C.size_t(len(body)), &e))
	switch r {{
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, takeError(e)
	}}
}}

// Delete{name} deletes a {name} by id with referential integrity. Returns false if absent.
func (db *DB) Delete{name}(id {id_go}) (bool, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return false, err
	}}
	var e *C.ForgeError
	r := int(C.{pfx}{snake}_delete(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &e))
	switch r {{
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, takeError(e)
	}}
}}

// Get{name}At fetches a {name} by id as of a snapshot, or (nil, nil) if absent then.
func (db *DB) Get{name}At(snap *Snapshot, id {id_go}) (*{name}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{snake}_get_at(db.ptr, snap.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	b := takeBuffer(out, outLen)
	if b == nil {{
		return nil, nil
	}}
	var rec {name}
	if err := json.Unmarshal(b, &rec); err != nil {{
		return nil, err
	}}
	return &rec, nil
}}

// All{name}At returns every live {name} as of a snapshot.
func (db *DB) All{name}At(snap *Snapshot) ([]{name}, error) {{
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{snake}_all_at(db.ptr, snap.ptr, &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	var recs []{name}
	if err := json.Unmarshal(takeBuffer(out, outLen), &recs); err != nil {{
		return nil, err
	}}
	return recs, nil
}}
"#
        )
    }

    fn async_methods(m: &GoModel, pfx: &str) -> String {
        let GoModel { name, snake, id_go } = m;
        format!(
            r#"
// Insert{name}Async inserts a {name} on the async worker, yielding the new id.
func (db *DB) Insert{name}Async(rec {name}) <-chan Result[{id_go}] {{
	body, err := json.Marshal(rec)
	if err != nil {{
		return erroredResult[{id_go}](err)
	}}
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_insert_async(db.ptr, bytesPtr(body), C.size_t(len(body)), t)
	}}, func(b []byte) ({id_go}, error) {{
		var id {id_go}
		err := json.Unmarshal(b, &id)
		return id, err
	}})
}}

// Get{name}Async fetches a {name} by id on the async worker (Value nil if absent).
func (db *DB) Get{name}Async(id {id_go}) <-chan Result[*{name}] {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return erroredResult[*{name}](err)
	}}
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_get_async(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), t)
	}}, func(b []byte) (*{name}, error) {{
		if b == nil {{
			return nil, nil
		}}
		var rec {name}
		if err := json.Unmarshal(b, &rec); err != nil {{
			return nil, err
		}}
		return &rec, nil
	}})
}}

// Count{name}Async returns the live {name} row count on the async worker.
func (db *DB) Count{name}Async() <-chan Result[int64] {{
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_count_async(db.ptr, t)
	}}, func(b []byte) (int64, error) {{
		var n int64
		err := json.Unmarshal(b, &n)
		return n, err
	}})
}}

// All{name}Async returns every live {name} on the async worker.
func (db *DB) All{name}Async() <-chan Result[[]{name}] {{
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_all_async(db.ptr, t)
	}}, func(b []byte) ([]{name}, error) {{
		var recs []{name}
		err := json.Unmarshal(b, &recs)
		return recs, err
	}})
}}

// Update{name}Async replaces a {name} by id on the async worker (false if absent).
func (db *DB) Update{name}Async(id {id_go}, rec {name}) <-chan Result[bool] {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return erroredResult[bool](err)
	}}
	body, err := json.Marshal(rec)
	if err != nil {{
		return erroredResult[bool](err)
	}}
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_update_async(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), bytesPtr(body), C.size_t(len(body)), t)
	}}, func(b []byte) (bool, error) {{
		var ok bool
		err := json.Unmarshal(b, &ok)
		return ok, err
	}})
}}

// Delete{name}Async deletes a {name} by id on the async worker (false if absent).
func (db *DB) Delete{name}Async(id {id_go}) <-chan Result[bool] {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return erroredResult[bool](err)
	}}
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_delete_async(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), t)
	}}, func(b []byte) (bool, error) {{
		var ok bool
		err := json.Unmarshal(b, &ok)
		return ok, err
	}})
}}
"#
        )
    }

    fn relation_method(op: &GoRelOp, pfx: &str) -> String {
        match op {
            GoRelOp::ForwardFk {
                sym,
                method,
                source_id_go,
                target,
            } => format!(
                r#"
// {method} resolves the foreign key by the source id, or (nil, nil) if the source is absent.
func (db *DB) {method}(id {source_id_go}) (*{target}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{sym}(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	b := takeBuffer(out, outLen)
	if b == nil {{
		return nil, nil
	}}
	var rec *{target}
	if err := json.Unmarshal(b, &rec); err != nil {{
		return nil, err
	}}
	return rec, nil
}}
"#
            ),
            GoRelOp::Vec {
                sym,
                method,
                id_go,
                target,
            } => format!(
                r#"
// {method} returns the related {target} records for the given id.
func (db *DB) {method}(id {id_go}) ([]{target}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{sym}(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	var recs []{target}
	if err := json.Unmarshal(takeBuffer(out, outLen), &recs); err != nil {{
		return nil, err
	}}
	return recs, nil
}}
"#
            ),
            GoRelOp::VecAt {
                sym,
                method,
                id_go,
                target,
            } => format!(
                r#"
// {method} returns the linked {target} records for the given id as of a snapshot.
func (db *DB) {method}(snap *Snapshot, id {id_go}) ([]{target}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.{pfx}{sym}(db.ptr, snap.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
	if !bool(ok) {{
		return nil, takeError(e)
	}}
	var recs []{target}
	if err := json.Unmarshal(takeBuffer(out, outLen), &recs); err != nil {{
		return nil, err
	}}
	return recs, nil
}}
"#
            ),
            GoRelOp::Link {
                sym,
                method,
                left_go,
                right_go,
            } => format!(
                r#"
// {method} links a left and right record in the junction.
func (db *DB) {method}(left {left_go}, right {right_go}) error {{
	lb, err := json.Marshal(left)
	if err != nil {{
		return err
	}}
	rb, err := json.Marshal(right)
	if err != nil {{
		return err
	}}
	var e *C.ForgeError
	ok := C.{pfx}{sym}(db.ptr, bytesPtr(lb), C.size_t(len(lb)), bytesPtr(rb), C.size_t(len(rb)), &e)
	if !bool(ok) {{
		return takeError(e)
	}}
	return nil
}}
"#
            ),
            GoRelOp::Unlink {
                sym,
                method,
                left_go,
                right_go,
            } => format!(
                r#"
// {method} unlinks a left/right pair. Returns false if the link did not exist.
func (db *DB) {method}(left {left_go}, right {right_go}) (bool, error) {{
	lb, err := json.Marshal(left)
	if err != nil {{
		return false, err
	}}
	rb, err := json.Marshal(right)
	if err != nil {{
		return false, err
	}}
	var e *C.ForgeError
	r := int(C.{pfx}{sym}(db.ptr, bytesPtr(lb), C.size_t(len(lb)), bytesPtr(rb), C.size_t(len(rb)), &e))
	switch r {{
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, takeError(e)
	}}
}}
"#
            ),
        }
    }

    // --- Type mapping ---------------------------------------------------------

    /// The Go type of a model's identity field (`string` for a uuid PK).
    fn go_id_type(schema: &Schema, model: &forgedb_parser::Model) -> String {
        // #266: an identity that is itself a foreign key resolves through to the
        // key it ultimately backs onto.
        let Some(ty) = RustGenerator::identity_type(schema, model) else {
            return "string".to_string();
        };
        match &ty {
            FieldType::U32 => "uint32",
            FieldType::U64 => "uint64",
            FieldType::I32 => "int32",
            FieldType::I64 => "int64",
            _ => "string",
        }
        .to_string()
    }

    /// The Go type for a struct field — a pointer for a nullable value type, and
    /// `json.RawMessage` for any JSON/`char(N)`/fixed-array/virtual-relation field
    /// (a lossless passthrough that matches whatever serde emits).
    fn go_field_type(schema: &Schema, field: &forgedb_parser::Field) -> String {
        let (base, is_raw) = Self::go_scalar_type(schema, &field.field_type);
        if is_raw {
            // `json.RawMessage` holds `null` natively, so it already covers the
            // nullable case (virtual relation collection, char/array).
            "json.RawMessage".to_string()
        } else if field.is_nullable() {
            format!("*{base}")
        } else {
            base
        }
    }

    /// Map a scalar field type to its Go type. The bool is `true` when the field
    /// must be represented as `json.RawMessage` (JSON value, `char(N)`, fixed
    /// array, or a virtual relation collection). Enums and inline structs map to
    /// their generated Go named types.
    fn go_scalar_type(schema: &Schema, ft: &FieldType) -> (String, bool) {
        match ft {
            FieldType::U32 => ("uint32".to_string(), false),
            FieldType::U64 => ("uint64".to_string(), false),
            FieldType::I32 => ("int32".to_string(), false),
            FieldType::I64 => ("int64".to_string(), false),
            FieldType::F64 => ("float64".to_string(), false),
            FieldType::Bool => ("bool".to_string(), false),
            // #238: an inline `string(N)` is a string on the wire.
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                ("string".to_string(), false)
            }
            // Timestamp and decimal both serialize as strings.  A row crosses
            // cgo as opaque JSON bytes and is decoded by `encoding/json`, so
            // every Go field type here has to be whatever *serde* produces —
            // and since #254 that is an RFC 3339 string, not a number.
            FieldType::Timestamp(_) => ("string".to_string(), false),
            FieldType::Decimal => ("string".to_string(), false),
            // An enum serializes as its variant-name string → its generated
            // `type <Name> string`; an inline struct → its generated Go struct.
            FieldType::Enum(name) => (name.clone(), false),
            FieldType::StructType(name) | FieldType::OptionalStructType(name) => {
                (name.clone(), false)
            }
            // #266: an FK is the target's identity value, so it is spelled with
            // the target key's own Go type.
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::go_scalar_type(schema, &RustGenerator::resolved_type(schema, ft)),
            // Nullable wraps a scalar; return the inner mapping (the `*` is added
            // by `go_field_type` via `is_nullable`).
            FieldType::Nullable(inner) => Self::go_scalar_type(schema, inner),
            // JSON passthrough for everything whose exact shape we don't type.
            _ => ("json.RawMessage".to_string(), true),
        }
    }

    /// Emit a Go named type per declared enum (`type <Name> string` + a const per
    /// variant), matching the serde variant-name string representation.
    fn enum_types(schema: &Schema) -> String {
        let mut s = String::new();
        for e in &schema.enums {
            s.push_str(&format!(
                "\n// {name} is a generated enum (serialized as its variant name).\ntype {name} string\n\nconst (\n",
                name = e.name
            ));
            for v in &e.variants {
                s.push_str(&format!("\t{}{} {} = \"{}\"\n", e.name, v, e.name, v));
            }
            s.push_str(")\n");
        }
        s
    }

    /// Emit a Go struct per declared inline struct (fixed-size fields only), so a
    /// struct-typed model field is fully typed rather than `json.RawMessage`.
    fn struct_types(schema: &Schema) -> String {
        let mut s = String::new();
        for st in &schema.structs {
            s.push_str(&format!(
                "\n// {name} is a generated inline struct.\ntype {name} struct {{\n",
                name = st.name
            ));
            for field in &st.fields {
                s.push_str(&format!(
                    "\t{} {} `json:\"{}\"`\n",
                    go_field_name(&field.name),
                    Self::go_field_type(schema, field),
                    field.name
                ));
            }
            s.push_str("}\n");
        }
        s
    }
}

/// The token every C-symbol reference in the *static* Go/C templates below
/// carries in place of the app's per-app C-symbol prefix (#335 §2, decision 9).
///
/// It is deliberately not a legal C or Go identifier fragment: a placeholder
/// that survives substitution is a **syntax error in the emitted file**, not a
/// silently-wrong symbol that links against the wrong app.
/// `test_go_calls_match_ffi_symbols` asserts none survives.
const SYM_PLACEHOLDER: &str = "%SYM%";

/// Substitute the app's C-symbol prefix into a static template.
///
/// The prefix comes from `forgedb::naming::symbol_prefix` — the ONE definition —
/// and is threaded in from the caller. `go.rs` never derives it, and neither
/// does `ffi.rs`, which emits the symbols this file calls.
pub(crate) fn subst(template: &str, symbol_prefix: &str) -> String {
    template.replace(SYM_PLACEHOLDER, symbol_prefix)
}

/// PascalCase a snake_case identifier (`user_posts` → `UserPosts`).
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The exported Go struct field name for a snake_case schema field.
fn go_field_name(name: &str) -> String {
    to_pascal_case(name)
}

/// The generated-file banner + `package`/cgo/imports preamble.
const FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
//
// The ForgeDB Go binding links the generated engine STATICALLY over cgo.
// `forgedb build` compiles the app's FFI package and delivers its archive here
// as `libforgedb.a`; `go build` copies the archive's contents into your binary,
// so the binary depends on nothing inside ForgeDB's build cache — which is a
// cache, and may be garbage-collected at any time.
//
// A dynamic library would NOT survive that: rustc stamps an ABSOLUTE install
// name into a cdylib, so a Go binary that linked one records the cache path and
// dies once the cache is cleared.
package forgedb

/*
// `-lforgedb` resolves to libforgedb.a in this directory (${SRCDIR}), delivered
// by `forgedb build`. The per-OS lines below supply what the Rust standard
// library and the generated engine need at static link time; ONE flag set for
// both platforms does not link.
#cgo LDFLAGS: -L${SRCDIR} -lforgedb
#cgo darwin LDFLAGS: -lc++ -framework CoreFoundation -framework Security
#cgo linux LDFLAGS: -lm -ldl -lpthread
#include <stdlib.h>
#include "forgedb.h"

// The exported Go async-completion callback is defined in forgedb_async.go.
// This preamble only DECLARES it and wraps registration in a shim — the
// exported symbol itself must live in a file that has no C definitions, per
// cgo's //export rule. `forgedbGoRegister` is `static inline`, so it has
// internal linkage and needs no prefix; everything it names does.
extern void %SYM%GoCompletion(uint64_t token, int32_t status, uint8_t* payload, size_t payload_len);
static inline void forgedbGoRegister(void) { %SYM%set_completion_callback(%SYM%GoCompletion); }
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"sync"
	"sync/atomic"
	"unsafe"
)

// The engine this package links is built by ForgeDB, not by `go generate`:
//
//	forgedb build     # compiles the app's packages and delivers libforgedb.a here
//	go build ./...
//
// Re-run `forgedb build` after any schema change — the C ABI is tailored per
// schema, and the archive is linked into your binary at `go build` time.

// Keep the encoding/json import used even for a schema with no id-bearing models.
var _ = json.RawMessage(nil)
"#;

/// The schema-invariant Go spine: error/handle types + lifecycle + snapshot.
const SPINE: &str = r#"
// Error is a ForgeDB engine error surfaced across the C ABI.
type Error struct {
	Code    int
	Message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("forgedb error %d: %s", e.Code, e.Message)
}

// takeError converts a non-nil C ForgeError into a Go error, freeing it.
func takeError(errp *C.ForgeError) error {
	if errp == nil {
		return &Error{Code: 0, Message: "unknown error"}
	}
	code := int(C.%SYM%error_code(errp))
	msg := C.GoString(C.%SYM%error_message(errp))
	C.%SYM%error_free(errp)
	return &Error{Code: code, Message: msg}
}

// takeBuffer copies an engine-owned (ptr, len) buffer into Go memory and frees
// it via the engine's free_buffer entry point. A nil pointer yields nil.
func takeBuffer(ptr *C.uint8_t, length C.size_t) []byte {
	if ptr == nil {
		return nil
	}
	b := C.GoBytes(unsafe.Pointer(ptr), C.int(length))
	C.%SYM%free_buffer(ptr, length)
	return b
}

// bytesPtr views a Go byte slice as a *C.uint8_t for a synchronous C call (the
// engine reads the bytes before returning, so the borrow never escapes).
func bytesPtr(b []byte) *C.uint8_t {
	if len(b) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0]))
}

// DB is an open ForgeDB database handle (single-writer per process).
type DB struct {
	ptr *C.Db
}

// Snapshot is a cross-model-consistent point-in-time read snapshot.
type Snapshot struct {
	ptr *C.Snapshot
}

// Open opens (or creates) a database rooted at the given directory.
func Open(root string) (*DB, error) {
	croot := C.CString(root)
	defer C.free(unsafe.Pointer(croot))
	var e *C.ForgeError
	p := C.%SYM%open(croot, 0, &e)
	if p == nil {
		return nil, takeError(e)
	}
	return &DB{ptr: p}, nil
}

// Close releases the database handle and its single-writer lock.
func (db *DB) Close() {
	if db.ptr != nil {
		C.%SYM%close(db.ptr)
		db.ptr = nil
	}
}

// Commit flushes every column to durable storage (fsync).
func (db *DB) Commit() error {
	var e *C.ForgeError
	if !bool(C.%SYM%commit(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Checkpoint forces a WAL checkpoint (fsync columns, then truncate the WAL).
func (db *DB) Checkpoint() error {
	var e *C.ForgeError
	if !bool(C.%SYM%checkpoint(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Compact explicitly reclaims dead row versions.
func (db *DB) Compact() error {
	var e *C.ForgeError
	if !bool(C.%SYM%compact(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Snapshot captures a cross-model-consistent read snapshot. Free it with Free.
func (db *DB) Snapshot() (*Snapshot, error) {
	var e *C.ForgeError
	p := C.%SYM%snapshot(db.ptr, &e)
	if p == nil {
		return nil, takeError(e)
	}
	return &Snapshot{ptr: p}, nil
}

// Free releases a snapshot captured by Snapshot.
func (s *Snapshot) Free() {
	if s.ptr != nil {
		C.%SYM%snapshot_free(s.ptr)
		s.ptr = nil
	}
}

// Version returns the generated engine's version string.
func Version() string {
	return C.GoString(C.%SYM%version())
}
"#;

/// The `README.md` scaffold: build order + cgo caveats for the Go binding.
const README_SCAFFOLD: &str = r#"# ForgeDB Go binding

Generated Go package binding a ForgeDB app database over cgo — schema-tailored
CRUD, snapshot reads, relation traversal, async CRUD, and Arrow columnar export
over the generated native FFI ABI.

## Build

The package links the generated engine **statically**, out of `libforgedb.a` in
this directory. `forgedb build` compiles that archive and delivers it here:

```sh
forgedb build            # compiles the app's packages, delivers libforgedb.a here
CGO_ENABLED=1 go build ./...
```

Re-run `forgedb build` after every `.forge` schema change — the C ABI is tailored
per schema, and the archive is linked into your binary at `go build` time.

ForgeDB compiles the engine inside its own build cache, and that cache is a
cache: it can be cleared at any time. Static linking is what makes your binary
survive that. Do not point cgo at a `.dylib`/`.so` in the cache instead — rustc
stamps an absolute install name into one, so the binary would record the cache
path and die once it is gone.

If the package includes `forgedb_arrow.go` (columnar Arrow export), it depends on
the external module `github.com/apache/arrow-go/v18` (already listed in `go.mod`).
Run `go mod tidy` once to populate `go.sum` before building.

## Caveats

- **cgo is required** (`CGO_ENABLED=1`, the default with a C toolchain present).
  A pure-Go / cross-compiled build without a C toolchain will not work.
- **Cross-compilation** needs a matching C cross-toolchain and a `libforgedb.a`
  built for the target — plain `GOOS`/`GOARCH` switching is not enough.
- **Single writer per process**: `Open` takes an exclusive directory lock, same
  as every ForgeDB writer.
- **Async threading contract**: do not mix the `*Async` methods with synchronous
  methods concurrently on the same `*DB` handle — while async ops are outstanding
  the engine's worker thread is the handle's sole accessor.
"#;

/// The `forgedb_arrow.go` preamble: package + cgo + the arrow-go imports.
const ARROW_FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
//
// Arrow columnar export for the Go binding. This file pulls the external module
// github.com/apache/arrow-go/v18 — run `go mod tidy` before building. It imports
// the FFI's Arrow C-Data-Interface export (zero-copy where possible) into an
// arrow.Array.
package forgedb

/*
#include <stdint.h>
#include <stddef.h>
#include "forgedb.h"
*/
import "C"

import (
	"unsafe"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/cdata"
)
"#;

/// The static `forgedb_async.go`: the exported cgo completion callback. Its
/// preamble contains ONLY declarations (per cgo's `//export` rule); the registry
/// + registration shim live in `forgedb.go`.
const ASYNC_BRIDGE_FILE: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
//
// The async completion bridge for the Go binding. This file is kept separate
// from forgedb.go because cgo forbids C definitions in the preamble of a file
// that uses //export.
package forgedb

/*
#include <stdint.h>
#include <stddef.h>
#include "forgedb.h"
*/
import "C"

import "unsafe"

// %SYM%GoCompletion is invoked by the engine's async worker thread when an
// async op finishes. It copies the (engine-owned) payload, frees it, and hands
// the result to the waiting goroutine via the token registry in forgedb.go.
//
// Its NAME carries the app's prefix like every other C symbol here: cgo's
// //export makes it an external C symbol, so two ForgeDB Go packages linked
// into one binary would otherwise collide on it exactly as they would on
// `open` — and this one is emitted by Go, not by ffi.rs, so the ffi-side
// prefix alone does not cover it.
//
//export %SYM%GoCompletion
func %SYM%GoCompletion(token C.uint64_t, status C.int32_t, payload *C.uint8_t, payloadLen C.size_t) {
	var b []byte
	if payload != nil {
		b = C.GoBytes(unsafe.Pointer(payload), C.int(payloadLen))
		C.%SYM%free_buffer(payload, payloadLen)
	}
	deliverCompletion(uint64(token), int(status), b)
}
"#;

/// The schema-invariant async plumbing: the token→channel registry, the generic
/// `runAsync` driver over the FFI completion bridge, and one-time callback
/// registration. The exported completion callback itself lives in the separate
/// `forgedb_async.go` (cgo forbids C definitions in an `//export` file).
const ASYNC_SPINE: &str = r#"
// Result carries the outcome of an async operation delivered on its channel.
type Result[T any] struct {
	Value T
	Err   error
}

// asyncResult is the raw completion payload handed from the C callback thread.
type asyncResult struct {
	status  int
	payload []byte
}

var (
	asyncCompletions sync.Map // uint64 token -> chan asyncResult
	asyncNextToken   atomic.Uint64
	asyncRegister    sync.Once
)

// deliverCompletion is called (via the exported callback) on the engine's async
// worker thread with a finished op's token + result. It hands the payload to the
// waiting goroutine. Defined here (not in the //export file) so the registry
// stays in one place.
func deliverCompletion(token uint64, status int, payload []byte) {
	if ch, ok := asyncCompletions.LoadAndDelete(token); ok {
		ch.(chan asyncResult) <- asyncResult{status: status, payload: payload}
	}
}

// ensureAsyncRegistered installs the process-wide completion callback exactly
// once, lazily on the first async op.
func ensureAsyncRegistered() {
	asyncRegister.Do(func() { C.forgedbGoRegister() })
}

// runAsync issues an async op: it reserves a token, enqueues the C call, and
// returns a channel that yields the decoded result on completion. `call` runs
// synchronously (the engine reads any arg bytes before returning), so caller
// buffers never outlive the call.
//
// NOTE: async and sync ops must not be used concurrently on the SAME handle —
// while async ops are outstanding the engine's worker is the handle's sole
// accessor (the FFI async threading contract).
func runAsync[T any](call func(token C.uint64_t), decode func([]byte) (T, error)) <-chan Result[T] {
	ensureAsyncRegistered()
	out := make(chan Result[T], 1)
	token := asyncNextToken.Add(1)
	ch := make(chan asyncResult, 1)
	asyncCompletions.Store(token, ch)
	call(C.uint64_t(token))
	go func() {
		r := <-ch
		var zero T
		if r.status != 0 {
			out <- Result[T]{Value: zero, Err: &Error{Code: r.status, Message: string(r.payload)}}
			return
		}
		v, err := decode(r.payload)
		out <- Result[T]{Value: v, Err: err}
	}()
	return out
}

// erroredResult yields a channel that immediately delivers an error (used when
// argument marshaling fails before an async op is issued).
func erroredResult[T any](err error) <-chan Result[T] {
	out := make(chan Result[T], 1)
	var zero T
	out <- Result[T]{Value: zero, Err: err}
	return out
}
"#;

/// The `forgedb.h` fixed preamble: guard, includes, opaque handle typedefs, and
/// the schema-invariant spine prototypes.
pub(crate) const HEADER_PREAMBLE: &str = r#"/* Code generated by ForgeDB. DO NOT EDIT. */
#ifndef FORGEDB_H
#define FORGEDB_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

typedef struct Db Db;
typedef struct ForgeError ForgeError;
typedef struct Snapshot Snapshot;

/* --- lifecycle + error spine (schema-invariant) --- */
const char* %SYM%version(void);
Db* %SYM%open(const char* root, uint32_t flags, ForgeError** err_out);
void %SYM%close(Db* db);
bool %SYM%commit(Db* db, ForgeError** err_out);
bool %SYM%checkpoint(Db* db, ForgeError** err_out);
bool %SYM%compact(Db* db, ForgeError** err_out);
int32_t %SYM%error_code(const ForgeError* err);
const char* %SYM%error_message(const ForgeError* err);
void %SYM%error_free(ForgeError* err);
void %SYM%free_buffer(uint8_t* ptr, size_t len);
Snapshot* %SYM%snapshot(Db* db, ForgeError** err_out);
void %SYM%snapshot_free(Snapshot* snap);

/* --- async completion bridge (schema-invariant) --- */
typedef void (*ForgeCompletion)(uint64_t token, int32_t status, uint8_t* payload, size_t payload_len);
void %SYM%set_completion_callback(ForgeCompletion cb);

/* --- Arrow C Data Interface (abi.h layout; matches arrow-go's cdata) --- */
struct ArrowSchema {
  const char* format;
  const char* name;
  const char* metadata;
  int64_t flags;
  int64_t n_children;
  struct ArrowSchema** children;
  struct ArrowSchema* dictionary;
  void (*release)(struct ArrowSchema*);
  void* private_data;
};
struct ArrowArray {
  int64_t length;
  int64_t null_count;
  int64_t offset;
  int64_t n_buffers;
  int64_t n_children;
  const void** buffers;
  struct ArrowArray** children;
  struct ArrowArray* dictionary;
  void (*release)(struct ArrowArray*);
  void* private_data;
};
typedef struct ArrowSchema ArrowSchema;
typedef struct ArrowArray ArrowArray;
"#;
