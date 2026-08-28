use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, RelationType, Schema};
use std::collections::{HashMap, HashSet};

pub struct GoGenerator;

pub(crate) struct GoModel {
    pub(crate) name: String,
    pub(crate) snake: String,
    pub(crate) id_go: String,
}

pub(crate) enum GoRelOp {
    ForwardFk {
        sym: String,
        method: String,
        source_id_go: String,
        target: String,
    },
    Vec {
        sym: String,
        method: String,
        id_go: String,
        target: String,
    },
    VecAt {
        sym: String,
        method: String,
        id_go: String,
        target: String,
    },
    Link {
        sym: String,
        method: String,
        left_go: String,
        right_go: String,
    },
    Unlink {
        sym: String,
        method: String,
        left_go: String,
        right_go: String,
    },
}

pub(crate) struct ArrowCol {
    pub(crate) sym: String,
    pub(crate) method: String,
}

impl GoGenerator {
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

        code.push_str(&Self::enum_types(schema));
        code.push_str(&Self::struct_types(schema));

        for model in schema.models.iter() {
            code.push_str(&Self::model_struct(schema, model));
        }

        for m in &models {
            code.push_str(&Self::crud_methods(m, symbol_prefix));
        }

        for m in &models {
            code.push_str(&Self::async_methods(m, symbol_prefix));
        }

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

    fn fingerprint_check(symbol_prefix: &str, fingerprint: &str) -> String {
        format!(
            r#"
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

    pub fn generate_async_bridge(symbol_prefix: &str) -> GeneratedCode {
        GeneratedCode {
            description: "Go async completion bridge (//export callback)".to_string(),
            code: subst(ASYNC_BRIDGE_FILE, symbol_prefix),
        }
    }

    pub fn go_mod_scaffold(module: &str, needs_arrow: bool) -> String {
        let mut m = format!("module {module}\n\ngo 1.21\n");
        if needs_arrow {
            m.push_str("\nrequire github.com/apache/arrow-go/v18 v18.7.0\n");
        }
        m
    }

    pub fn needs_arrow(schema: &Schema) -> bool {
        !Self::arrow_columns(schema).is_empty()
    }

    pub fn readme_scaffold() -> &'static str {
        README_SCAFFOLD
    }

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

    pub(crate) fn relation_ops(schema: &Schema) -> Vec<GoRelOp> {
        let mut ops = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

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

        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
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

    fn model_struct(schema: &Schema, model: &forgedb_parser::Model) -> String {
        let mut s = format!("\ntype {name} struct {{\n", name = model.name);
        for field in &model.fields {
            let go_ty = Self::go_field_type(schema, field);
            let tag = if field.auto_generate
                && matches!(field.field_type, FieldType::Uuid | FieldType::Timestamp(_))
            {
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

func (db *DB) Count{name}() (int64, error) {{
	var e *C.ForgeError
	n := int64(C.{pfx}{snake}_count(db.ptr, &e))
	if n < 0 {{
		return 0, takeError(e)
	}}
	return n, nil
}}

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

func (db *DB) Count{name}Async() <-chan Result[int64] {{
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_count_async(db.ptr, t)
	}}, func(b []byte) (int64, error) {{
		var n int64
		err := json.Unmarshal(b, &n)
		return n, err
	}})
}}

func (db *DB) All{name}Async() <-chan Result[[]{name}] {{
	return runAsync(func(t C.uint64_t) {{
		C.{pfx}{snake}_all_async(db.ptr, t)
	}}, func(b []byte) ([]{name}, error) {{
		var recs []{name}
		err := json.Unmarshal(b, &recs)
		return recs, err
	}})
}}

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

    fn go_id_type(schema: &Schema, model: &forgedb_parser::Model) -> String {
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

    fn go_field_type(schema: &Schema, field: &forgedb_parser::Field) -> String {
        let (base, is_raw) = Self::go_scalar_type(schema, &field.field_type);
        if is_raw {
            "json.RawMessage".to_string()
        } else if field.is_nullable() {
            format!("*{base}")
        } else {
            base
        }
    }

    fn go_scalar_type(schema: &Schema, ft: &FieldType) -> (String, bool) {
        match ft {
            FieldType::U32 => ("uint32".to_string(), false),
            FieldType::U64 => ("uint64".to_string(), false),
            FieldType::I32 => ("int32".to_string(), false),
            FieldType::I64 => ("int64".to_string(), false),
            FieldType::F64 => ("float64".to_string(), false),
            FieldType::Bool => ("bool".to_string(), false),
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                ("string".to_string(), false)
            }
            FieldType::Timestamp(_) => ("string".to_string(), false),
            FieldType::Decimal => ("string".to_string(), false),
            FieldType::Enum(name) => (name.clone(), false),
            FieldType::StructType(name) | FieldType::OptionalStructType(name) => {
                (name.clone(), false)
            }
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::go_scalar_type(schema, &RustGenerator::resolved_type(schema, ft)),
            FieldType::Nullable(inner) => Self::go_scalar_type(schema, inner),
            _ => ("json.RawMessage".to_string(), true),
        }
    }

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

const SYM_PLACEHOLDER: &str = "%SYM%";

pub(crate) fn subst(template: &str, symbol_prefix: &str) -> String {
    template.replace(SYM_PLACEHOLDER, symbol_prefix)
}

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

fn go_field_name(name: &str) -> String {
    to_pascal_case(name)
}

const FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
package forgedb

/*
#cgo LDFLAGS: -L${SRCDIR} -lforgedb
#cgo darwin LDFLAGS: -lc++ -framework CoreFoundation -framework Security
#cgo linux LDFLAGS: -lm -ldl -lpthread
#include <stdlib.h>
#include "forgedb.h"

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


var _ = json.RawMessage(nil)
"#;

const SPINE: &str = r#"
type Error struct {
	Code    int
	Message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("forgedb error %d: %s", e.Code, e.Message)
}

func takeError(errp *C.ForgeError) error {
	if errp == nil {
		return &Error{Code: 0, Message: "unknown error"}
	}
	code := int(C.%SYM%error_code(errp))
	msg := C.GoString(C.%SYM%error_message(errp))
	C.%SYM%error_free(errp)
	return &Error{Code: code, Message: msg}
}

func takeBuffer(ptr *C.uint8_t, length C.size_t) []byte {
	if ptr == nil {
		return nil
	}
	b := C.GoBytes(unsafe.Pointer(ptr), C.int(length))
	C.%SYM%free_buffer(ptr, length)
	return b
}

func bytesPtr(b []byte) *C.uint8_t {
	if len(b) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0]))
}

type DB struct {
	ptr *C.Db
}

type Snapshot struct {
	ptr *C.Snapshot
}

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

func (db *DB) Close() {
	if db.ptr != nil {
		C.%SYM%close(db.ptr)
		db.ptr = nil
	}
}

func (db *DB) Commit() error {
	var e *C.ForgeError
	if !bool(C.%SYM%commit(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

func (db *DB) Checkpoint() error {
	var e *C.ForgeError
	if !bool(C.%SYM%checkpoint(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

func (db *DB) Compact() error {
	var e *C.ForgeError
	if !bool(C.%SYM%compact(db.ptr, &e)) {
		return takeError(e)
	}
	return nil
}

func (db *DB) Snapshot() (*Snapshot, error) {
	var e *C.ForgeError
	p := C.%SYM%snapshot(db.ptr, &e)
	if p == nil {
		return nil, takeError(e)
	}
	return &Snapshot{ptr: p}, nil
}

func (s *Snapshot) Free() {
	if s.ptr != nil {
		C.%SYM%snapshot_free(s.ptr)
		s.ptr = nil
	}
}

func Version() string {
	return C.GoString(C.%SYM%version())
}
"#;

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

If you forget, the package refuses to load. Both halves carry a fingerprint of
the generated source they came from, and `init()` compares them, so a stale
archive is a panic naming the mismatch rather than a method set that no longer
matches your schema. Most schema changes already fail at link time; this also
covers the ones that do not — a `[storage]` or `[runtime]` setting changes what
the engine does without changing a single exported symbol, and the linker cannot
see that at all.

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

const ARROW_FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
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

const ASYNC_BRIDGE_FILE: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
package forgedb

/*
#include <stdint.h>
#include <stddef.h>
#include "forgedb.h"
*/
import "C"

import "unsafe"

func %SYM%GoCompletion(token C.uint64_t, status C.int32_t, payload *C.uint8_t, payloadLen C.size_t) {
	var b []byte
	if payload != nil {
		b = C.GoBytes(unsafe.Pointer(payload), C.int(payloadLen))
		C.%SYM%free_buffer(payload, payloadLen)
	}
	deliverCompletion(uint64(token), int(status), b)
}
"#;

const ASYNC_SPINE: &str = r#"
type Result[T any] struct {
	Value T
	Err   error
}

type asyncResult struct {
	status  int
	payload []byte
}

var (
	asyncCompletions sync.Map // uint64 token -> chan asyncResult
	asyncNextToken   atomic.Uint64
	asyncRegister    sync.Once
)

func deliverCompletion(token uint64, status int, payload []byte) {
	if ch, ok := asyncCompletions.LoadAndDelete(token); ok {
		ch.(chan asyncResult) <- asyncResult{status: status, payload: payload}
	}
}

func ensureAsyncRegistered() {
	asyncRegister.Do(func() { C.forgedbGoRegister() })
}

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

func erroredResult[T any](err error) <-chan Result[T] {
	out := make(chan Result[T], 1)
	var zero T
	out <- Result[T]{Value: zero, Err: err}
	return out
}
"#;

pub(crate) const HEADER_PREAMBLE: &str = r#"/* Code generated by ForgeDB. DO NOT EDIT. */
#ifndef FORGEDB_H
#define FORGEDB_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

typedef struct Db Db;
typedef struct ForgeError ForgeError;
typedef struct Snapshot Snapshot;

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

typedef void (*ForgeCompletion)(uint64_t token, int32_t status, uint8_t* payload, size_t payload_len);
void %SYM%set_completion_callback(ForgeCompletion cb);

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
