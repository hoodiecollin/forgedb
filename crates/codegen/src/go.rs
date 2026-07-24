//! Golang binding generator — an **experimental** class-2 transport target
//! (RFC #203, sibling of the PyO3/NAPI/WASM bindings under the #122 taxonomy).
//!
//! Emits, per schema, an idiomatic Go package (`forgedb.go` + a `forgedb.h` C
//! header) that calls the SAME generated native FFI C-ABI (`crates/codegen/src/
//! ffi.rs`) over cgo. It rides the existing `cdylib` unchanged — it adds **no new
//! C symbol and no new substrate dep** (the property that keeps it off the
//! publish-gap critical path and safe to ship experimental).
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
//! **Scope (first cut).** Sync core: per-model CRUD (`Insert`/`Get`/`Count`/
//! `All`/`Update`/`Delete`), point-in-time `_at` reads over a `Snapshot`, and
//! relation traversal (forward FK, reverse 1:M, M2M link/unlink + query getters).
//! The async completion bridge and Arrow columnar export are deferred (their C
//! symbols exist on the ABI; the typed Go wrappers are a follow-up).
//!
//! Field-shape fidelity: scalars map to their Go equivalents; `json`, inline
//! structs, `char(N)`, fixed arrays, and virtual relation-collection fields map
//! to `json.RawMessage` (a lossless JSON passthrough that round-trips whatever
//! serde emits), so the Go struct matches the generated `database.rs` serde shape
//! exactly.

use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, RelationType, Schema};
use std::collections::{HashMap, HashSet};

/// Generates the experimental Go cgo binding for a schema.
pub struct GoGenerator;

/// One id-bearing model exposed over the Go binding (CRUD + snapshot reads).
struct GoModel {
    /// PascalCase model name (also the Go struct name).
    name: String,
    /// snake_case name — the `forgedb_<snake>_<op>` C-symbol infix.
    snake: String,
    /// Go type of the model's identity field (`string` for uuid PKs).
    id_go: String,
}

/// A relation-traversal C entry point mirrored one-for-one from the FFI
/// generator (`generate_relation_ops`). `sym` is the C-symbol suffix (without
/// the `forgedb_` prefix); `method` is its PascalCase Go method name.
enum GoRelOp {
    /// Forward FK: resolve `*Target`/`?Target` by the source id → `*Target`.
    ForwardFk {
        sym: String,
        method: String,
        source_id_go: String,
        target: String,
    },
    /// Reverse 1:M / M2M query getter: all records for a (uuid) id → `[]Target`.
    Vec {
        sym: String,
        method: String,
        target: String,
    },
    /// M2M link (both uuid ids) → `error`.
    Link { sym: String, method: String },
    /// M2M unlink (both uuid ids) → `(bool, error)`.
    Unlink { sym: String, method: String },
}

impl GoGenerator {
    /// Generate the `forgedb.go` package for a schema.
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let models = Self::crud_models(schema);
        let rel_ops = Self::relation_ops(schema);

        let mut code = String::new();
        code.push_str(Self::file_header());
        code.push_str(SPINE);

        // --- Per-model row structs ---
        for model in schema.models.iter() {
            code.push_str(&Self::model_struct(model));
        }

        // --- Per-model CRUD + snapshot reads ---
        for m in &models {
            code.push_str(&Self::crud_methods(m));
        }

        // --- Relation traversal ---
        for op in &rel_ops {
            code.push_str(&Self::relation_method(op));
        }

        Ok(GeneratedCode {
            description: format!(
                "experimental Go cgo binding ({} models, {} relation ops)",
                models.len(),
                rel_ops.len()
            ),
            code,
        })
    }

    /// Generate the `forgedb.h` C header declaring every `forgedb_*` prototype
    /// the Go package binds (the schema-invariant spine + the per-model +
    /// relation symbols). cgo `#include`s it; the symbols are defined by the FFI
    /// `cdylib` this links against.
    pub fn generate_header(schema: &Schema) -> Result<GeneratedCode> {
        let models = Self::crud_models(schema);
        let rel_ops = Self::relation_ops(schema);

        let mut h = String::new();
        h.push_str(HEADER_PREAMBLE);

        for m in &models {
            let s = &m.snake;
            h.push_str(&format!(
                "\n/* --- {name} --- */\n\
                 bool forgedb_{s}_insert(Db* db, const uint8_t* record, size_t record_len, uint8_t** id_out, size_t* id_len_out, ForgeError** err_out);\n\
                 bool forgedb_{s}_get(Db* db, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 int64_t forgedb_{s}_count(Db* db, ForgeError** err_out);\n\
                 bool forgedb_{s}_all(Db* db, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 int32_t forgedb_{s}_update(Db* db, const uint8_t* id, size_t id_len, const uint8_t* record, size_t record_len, ForgeError** err_out);\n\
                 int32_t forgedb_{s}_delete(Db* db, const uint8_t* id, size_t id_len, ForgeError** err_out);\n\
                 bool forgedb_{s}_get_at(Db* db, const Snapshot* snap, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 bool forgedb_{s}_all_at(Db* db, const Snapshot* snap, uint8_t** out, size_t* out_len, ForgeError** err_out);\n",
                name = m.name,
                s = s,
            ));
        }

        if !rel_ops.is_empty() {
            h.push_str("\n/* --- relations --- */\n");
            for op in &rel_ops {
                h.push_str(&match op {
                    GoRelOp::ForwardFk { sym, .. } | GoRelOp::Vec { sym, .. } => format!(
                        "bool forgedb_{sym}(Db* db, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n"
                    ),
                    GoRelOp::Link { sym, .. } => format!(
                        "bool forgedb_{sym}(Db* db, const uint8_t* left, size_t left_len, const uint8_t* right, size_t right_len, ForgeError** err_out);\n"
                    ),
                    GoRelOp::Unlink { sym, .. } => format!(
                        "int32_t forgedb_{sym}(Db* db, const uint8_t* left, size_t left_len, const uint8_t* right, size_t right_len, ForgeError** err_out);\n"
                    ),
                });
            }
        }

        h.push_str("\n#endif /* FORGEDB_H */\n");
        Ok(GeneratedCode {
            description: format!("Go binding C header ({} models)", models.len()),
            code: h,
        })
    }

    /// A `go.mod` for the generated Go package. User-editable, so the CLI writes
    /// it ONLY when absent (like every other binding scaffold).
    pub fn go_mod_scaffold(module: &str) -> String {
        format!("module {module}\n\ngo 1.21\n")
    }

    // --- Derivation (shared by `generate` and `generate_header`) --------------

    /// The id-bearing models (same filter as the FFI per-model ops).
    fn crud_models(schema: &Schema) -> Vec<GoModel> {
        schema
            .models
            .iter()
            .filter(|m| m.fields.iter().any(|f| f.name == "id" || f.auto_generate))
            .map(|m| GoModel {
                name: m.name.clone(),
                snake: RustGenerator::to_snake_case(&m.name),
                id_go: Self::go_id_type(m),
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
    fn relation_ops(schema: &Schema) -> Vec<GoRelOp> {
        let mut ops = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // --- A. Forward FK getters (`*Target` / `?Target`) ---
        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_has_id = model.fields.iter().any(|f| f.name == "id" || f.auto_generate);
            let id_go = Self::go_id_type(model);
            for field in &model.fields {
                let target_name = match &field.field_type {
                    FieldType::Relation(RelationType::RequiredReference(t))
                    | FieldType::Relation(RelationType::OptionalReference(t)) => t,
                    _ => continue,
                };
                let Some(target) = schema.find_model(target_name) else {
                    continue;
                };
                if !RustGenerator::is_uuid_pk(target) {
                    continue;
                }
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
            if !RustGenerator::is_uuid_pk(parent) {
                continue;
            }
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
                target: p.child_model.clone(),
            });
        }

        // --- C. Many-to-many link / unlink + query getters ---
        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);

            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                ops.push(GoRelOp::Link {
                    method: to_pascal_case(&link_name),
                    sym: link_name,
                });
            }

            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                ops.push(GoRelOp::Unlink {
                    method: to_pascal_case(&unlink_name),
                    sym: unlink_name,
                });
            }

            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                ops.push(GoRelOp::Vec {
                    method: to_pascal_case(&fwd_name),
                    sym: fwd_name,
                    target: m.model2.clone(),
                });
                // Reserve the snapshot-scoped `_at` name in `seen` (not surfaced
                // as a Go method in this first cut) so downstream families dedup
                // exactly as the FFI derivation does.
                let fwd_at_name = format!("{snake1}_{}_at", m.field1);
                seen.insert(fwd_at_name);
            }

            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                ops.push(GoRelOp::Vec {
                    method: to_pascal_case(&rev_name),
                    sym: rev_name,
                    target: m.model1.clone(),
                });
            }
        }

        ops
    }

    // --- Rendering ------------------------------------------------------------

    fn model_struct(model: &forgedb_parser::Model) -> String {
        let mut s = format!("\n// {name} is a generated row of the `{name}` model.\ntype {name} struct {{\n", name = model.name);
        for field in &model.fields {
            let go_ty = Self::go_field_type(field);
            let tag = if field.auto_generate
                && matches!(field.field_type, FieldType::Uuid | FieldType::Timestamp)
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

    fn crud_methods(m: &GoModel) -> String {
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
	ok := C.forgedb_{snake}_insert(db.ptr, bytesPtr(body), C.size_t(len(body)), &idOut, &idLen, &e)
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
	ok := C.forgedb_{snake}_get(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
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
	n := int64(C.forgedb_{snake}_count(db.ptr, &e))
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
	ok := C.forgedb_{snake}_all(db.ptr, &out, &outLen, &e)
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
	r := int(C.forgedb_{snake}_update(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), bytesPtr(body), C.size_t(len(body)), &e))
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
	r := int(C.forgedb_{snake}_delete(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &e))
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
	ok := C.forgedb_{snake}_get_at(db.ptr, snap.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
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
	ok := C.forgedb_{snake}_all_at(db.ptr, snap.ptr, &out, &outLen, &e)
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

    fn relation_method(op: &GoRelOp) -> String {
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
	ok := C.forgedb_{sym}(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
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
            GoRelOp::Vec { sym, method, target } => format!(
                r#"
// {method} returns the related {target} records for the given id.
func (db *DB) {method}(id string) ([]{target}, error) {{
	idBytes, err := json.Marshal(id)
	if err != nil {{
		return nil, err
	}}
	var out *C.uint8_t
	var outLen C.size_t
	var e *C.ForgeError
	ok := C.forgedb_{sym}(db.ptr, bytesPtr(idBytes), C.size_t(len(idBytes)), &out, &outLen, &e)
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
            GoRelOp::Link { sym, method } => format!(
                r#"
// {method} links a left and right record in the junction.
func (db *DB) {method}(left string, right string) error {{
	lb, err := json.Marshal(left)
	if err != nil {{
		return err
	}}
	rb, err := json.Marshal(right)
	if err != nil {{
		return err
	}}
	var e *C.ForgeError
	ok := C.forgedb_{sym}(db.ptr, bytesPtr(lb), C.size_t(len(lb)), bytesPtr(rb), C.size_t(len(rb)), &e)
	if !bool(ok) {{
		return takeError(e)
	}}
	return nil
}}
"#
            ),
            GoRelOp::Unlink { sym, method } => format!(
                r#"
// {method} unlinks a left/right pair. Returns false if the link did not exist.
func (db *DB) {method}(left string, right string) (bool, error) {{
	lb, err := json.Marshal(left)
	if err != nil {{
		return false, err
	}}
	rb, err := json.Marshal(right)
	if err != nil {{
		return false, err
	}}
	var e *C.ForgeError
	r := int(C.forgedb_{sym}(db.ptr, bytesPtr(lb), C.size_t(len(lb)), bytesPtr(rb), C.size_t(len(rb)), &e))
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
    fn go_id_type(model: &forgedb_parser::Model) -> String {
        let Some(f) = model.fields.iter().find(|f| f.name == "id" || f.auto_generate) else {
            return "string".to_string();
        };
        match &f.field_type {
            FieldType::U32 => "uint32",
            FieldType::U64 => "uint64",
            FieldType::I32 => "int32",
            FieldType::I64 => "int64",
            _ => "string",
        }
        .to_string()
    }

    /// The Go type for a struct field — a pointer for a nullable scalar, and
    /// `json.RawMessage` for any JSON/struct/array/virtual-relation field (a
    /// lossless passthrough that matches whatever serde emits).
    fn go_field_type(field: &forgedb_parser::Field) -> String {
        let (base, is_raw) = Self::go_scalar_type(&field.field_type);
        if is_raw {
            // `json.RawMessage` holds `null` natively, so it already covers the
            // nullable case (optional struct, virtual relation collection).
            "json.RawMessage".to_string()
        } else if field.is_nullable() {
            format!("*{base}")
        } else {
            base.to_string()
        }
    }

    /// Map a scalar field type to its Go type. The bool is `true` when the field
    /// must be represented as `json.RawMessage` (JSON value, inline struct,
    /// `char(N)`, fixed array, or a virtual relation collection).
    fn go_scalar_type(ft: &FieldType) -> (&'static str, bool) {
        match ft {
            FieldType::U32 => ("uint32", false),
            FieldType::U64 => ("uint64", false),
            FieldType::I32 => ("int32", false),
            FieldType::I64 => ("int64", false),
            FieldType::F64 => ("float64", false),
            FieldType::Bool => ("bool", false),
            FieldType::String | FieldType::Uuid => ("string", false),
            // Timestamp serializes as an i64; decimal + enum serialize as strings.
            FieldType::Timestamp => ("int64", false),
            FieldType::Decimal => ("string", false),
            FieldType::Enum(_) => ("string", false),
            // FK scalars are stored as the (uuid) reference id.
            FieldType::Relation(RelationType::RequiredReference(_))
            | FieldType::Relation(RelationType::OptionalReference(_)) => ("string", false),
            // Nullable wraps a scalar; return the inner mapping (the `*` is added
            // by `go_field_type` via `is_nullable`).
            FieldType::Nullable(inner) => Self::go_scalar_type(inner),
            // JSON passthrough for everything whose exact shape we don't type.
            _ => ("json.RawMessage", true),
        }
    }

    /// The experimental-marked file header + `package`/cgo/imports preamble.
    fn file_header() -> &'static str {
        FILE_HEADER
    }
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

/// The experimental banner + `package`/cgo/imports preamble.
const FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
//
// EXPERIMENTAL: the ForgeDB Go binding target is experimental. Its Go API and the
// underlying C ABI are unstable and may change without notice; it is NOT covered
// by v1 stability (see docs/WHAT_V1_IS.md). It rides the generated native FFI
// cdylib over cgo — build the sibling `../ffi` crate with `cargo build --release`
// before `go build` so the shared library is available to link and load.
package forgedb

/*
#cgo LDFLAGS: -L${SRCDIR}/../ffi/target/release -lforgedb_ffi_engine -Wl,-rpath,${SRCDIR}/../ffi/target/release
#include <stdlib.h>
#include "forgedb.h"
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"unsafe"
)

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
	code := int(C.forgedb_error_code(errp))
	msg := C.GoString(C.forgedb_error_message(errp))
	C.forgedb_error_free(errp)
	return &Error{Code: code, Message: msg}
}

// takeBuffer copies an engine-owned (ptr, len) buffer into Go memory and frees
// it via forgedb_free_buffer. A nil pointer yields nil.
func takeBuffer(ptr *C.uint8_t, length C.size_t) []byte {
	if ptr == nil {
		return nil
	}
	b := C.GoBytes(unsafe.Pointer(ptr), C.int(length))
	C.forgedb_free_buffer(ptr, length)
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
	p := C.forgedb_open(croot, 0, &e)
	if p == nil {
		return nil, takeError(e)
	}
	return &DB{ptr: p}, nil
}

// Close releases the database handle and its single-writer lock.
func (db *DB) Close() {
	if db.ptr != nil {
		C.forgedb_close(db.ptr)
		db.ptr = nil
	}
}

// Commit flushes every column to durable storage (fsync).
func (db *DB) Commit() error {
	var e *C.ForgeError
	if !bool(C.forgedb_commit(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Checkpoint forces a WAL checkpoint (fsync columns, then truncate the WAL).
func (db *DB) Checkpoint() error {
	var e *C.ForgeError
	if !bool(C.forgedb_checkpoint(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Compact explicitly reclaims dead row versions.
func (db *DB) Compact() error {
	var e *C.ForgeError
	if !bool(C.forgedb_compact(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

// Snapshot captures a cross-model-consistent read snapshot. Free it with Free.
func (db *DB) Snapshot() (*Snapshot, error) {
	var e *C.ForgeError
	p := C.forgedb_snapshot(db.ptr, &e)
	if p == nil {
		return nil, takeError(e)
	}
	return &Snapshot{ptr: p}, nil
}

// Free releases a snapshot captured by Snapshot.
func (s *Snapshot) Free() {
	if s.ptr != nil {
		C.forgedb_snapshot_free(s.ptr)
		s.ptr = nil
	}
}

// Version returns the generated engine's version string.
func Version() string {
	return C.GoString(C.forgedb_version())
}
"#;

/// The `forgedb.h` fixed preamble: guard, includes, opaque handle typedefs, and
/// the schema-invariant spine prototypes.
const HEADER_PREAMBLE: &str = r#"/* Code generated by ForgeDB. DO NOT EDIT. */
/* EXPERIMENTAL: the Go binding C ABI is unstable and may change without notice. */
#ifndef FORGEDB_H
#define FORGEDB_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

typedef struct Db Db;
typedef struct ForgeError ForgeError;
typedef struct Snapshot Snapshot;

/* --- lifecycle + error spine (schema-invariant) --- */
const char* forgedb_version(void);
Db* forgedb_open(const char* root, uint32_t flags, ForgeError** err_out);
void forgedb_close(Db* db);
bool forgedb_commit(Db* db, ForgeError** err_out);
bool forgedb_checkpoint(Db* db, ForgeError** err_out);
bool forgedb_compact(Db* db, ForgeError** err_out);
int32_t forgedb_error_code(const ForgeError* err);
const char* forgedb_error_message(const ForgeError* err);
void forgedb_error_free(ForgeError* err);
void forgedb_free_buffer(uint8_t* ptr, size_t len);
Snapshot* forgedb_snapshot(Db* db, ForgeError** err_out);
void forgedb_snapshot_free(Snapshot* snap);
"#;
