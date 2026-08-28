use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::rust::RustGenerator;

fn py_stub_type(field_type: &forgedb_parser::FieldType) -> &'static str {
    use forgedb_parser::FieldType;
    match field_type {
        FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => "int",
        FieldType::F64 => "float",
        FieldType::Bool => "bool",
        FieldType::String
        | FieldType::StringN { .. }
        | FieldType::Uuid
        | FieldType::Timestamp(_)
        | FieldType::Decimal
        | FieldType::Enum(_) => "str",
        FieldType::Nullable(_) => "Any",
        _ => "Any",
    }
}

pub struct PyO3Generator;

impl PyO3Generator {
    pub const EXTENSION_STEM: &str = "_forgedb_native";

    pub const EXTENSION_SUFFIX: &str = ".abi3.so";

    pub fn extension_file() -> String {
        format!("{}{}", Self::EXTENSION_STEM, Self::EXTENSION_SUFFIX)
    }

    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let row_classes = Self::generate_row_classes(schema);
        let db_methods = Self::generate_db_methods(schema);
        let relation_methods = Self::generate_relation_methods(schema);
        let arrow_methods = Self::generate_arrow_methods(schema);
        let arrow_spine = Self::generate_arrow_spine(schema);
        let registrations = Self::generate_registrations(schema);
        let arrow_registration = Self::generate_arrow_registration(schema);
        let stem_ident = format_ident!("{}", Self::EXTENSION_STEM);

        let tokens = quote! {
            #![allow(warnings)]

            use forgedb_core as database;

            use pyo3::prelude::*;
            use pyo3::exceptions::PyException;
            use pyo3::types::PyCapsule;
            use std::ffi::{CString, c_char, c_void};
            use std::panic::{AssertUnwindSafe, catch_unwind};
            use std::ptr;

            use database::Database;
            use forgedb_core::forgedb_types::Uuid;

            pyo3::create_exception!(
                forgedb,
                ForgeDbError,
                PyException,
                "Raised on any ForgeDB engine or validation error."
            );

            fn to_py_err<E: std::fmt::Display>(e: E) -> PyErr {
                ForgeDbError::new_err(e.to_string())
            }

            fn panic_to_py_err(payload: Box<dyn std::any::Any + Send>) -> PyErr {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "engine panic".to_string());
                ForgeDbError::new_err(msg)
            }

            #(#row_classes)*

            #arrow_spine

            #[pyclass]
            pub struct ForgeDb {
                inner: Database,
            }

            #[pymethods]
            impl ForgeDb {
                #[staticmethod]
                fn open(root: String) -> PyResult<Self> {
                    let root = std::path::PathBuf::from(root);
                    match catch_unwind(AssertUnwindSafe(|| Database::open_at(root))) {
                        Ok(inner) => Ok(Self { inner }),
                        Err(p) => Err(panic_to_py_err(p)),
                    }
                }

                fn commit(&mut self) -> PyResult<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.inner.commit())) {
                        Ok(r) => r.map_err(to_py_err),
                        Err(p) => Err(panic_to_py_err(p)),
                    }
                }

                fn checkpoint(&mut self) -> PyResult<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.inner.checkpoint())) {
                        Ok(()) => Ok(()),
                        Err(p) => Err(panic_to_py_err(p)),
                    }
                }

                fn compact(&mut self) -> PyResult<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.inner.compact())) {
                        Ok(()) => Ok(()),
                        Err(p) => Err(panic_to_py_err(p)),
                    }
                }

                #(#db_methods)*

                #(#relation_methods)*

                #(#arrow_methods)*
            }

            mod fingerprint;

            #[pymodule]
            fn #stem_ident(m: &Bound<'_, PyModule>) -> PyResult<()> {
                m.add_class::<ForgeDb>()?;
                #(#registrations)*
                #arrow_registration
                m.add("ForgeDbError", m.py().get_type::<ForgeDbError>())?;
                m.add("__fingerprint__", fingerprint::FINGERPRINT)?;
                Ok(())
            }
        };

        let syntax_tree = syn::parse_file(&tokens.to_string()).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!(
                "Failed to parse generated PyO3 binding: {e}"
            ))
        })?;
        let code = prettyplease::unparse(&syntax_tree);

        Ok(GeneratedCode {
            code,
            description: format!("Python binding (PyO3) ({} models)", schema.models.len()),
        })
    }

    fn identity_models(schema: &Schema) -> impl Iterator<Item = &forgedb_parser::Model> {
        schema
            .models
            .iter()
            .filter(|m| m.has_identity())
    }

    fn returns_py_bound(ret_ty: &TokenStream) -> bool {
        ret_ty.to_string().contains("'py")
    }

    fn pyo3_getter(schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        field_name: &proc_macro2::Ident,
    ) -> (TokenStream, TokenStream) {
        use forgedb_parser::{FieldType, RelationType};
        match field_type {
            FieldType::U32 => (
                quote! { u32 },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::U64 => (
                quote! { u64 },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::I32 => (
                quote! { i32 },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::I64 => (
                quote! { i64 },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::F64 => (
                quote! { f64 },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::Bool => (
                quote! { bool },
                quote! { Ok(self.inner.#field_name) },
            ),

            FieldType::String | FieldType::StringN { .. } => (
                quote! { String },
                quote! { Ok(self.inner.#field_name.clone()) },
            ),

            FieldType::Uuid => (
                quote! { String },
                quote! { Ok(self.inner.#field_name.to_string()) },
            ),

            FieldType::Timestamp(_) => (
                quote! { String },
                quote! { Ok(self.inner.#field_name.to_string()) },
            ),

            FieldType::Decimal => (
                quote! { String },
                quote! { Ok(self.inner.#field_name.to_string()) },
            ),

            FieldType::Enum(_) => (
                quote! { String },
                quote! {
                    serde_json::to_value(&self.inner.#field_name)
                        .map_err(to_py_err)
                        .map(|__v| __v.as_str().unwrap_or_default().to_string())
                },
            ),

            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::pyo3_getter(schema, &RustGenerator::resolved_type(schema, field_type), field_name),

            FieldType::Nullable(inner) => {
                let (inner_ret, inner_body) = Self::pyo3_nullable_getter(inner, field_name);
                (inner_ret, inner_body)
            }

            _ => (
                quote! { Bound<'py, PyAny> },
                quote! { pythonize::pythonize(py, &self.inner.#field_name).map_err(to_py_err) },
            ),
        }
    }

    fn pyo3_nullable_getter(
        inner: &forgedb_parser::FieldType,
        field_name: &proc_macro2::Ident,
    ) -> (TokenStream, TokenStream) {
        use forgedb_parser::FieldType;
        match inner {
            FieldType::U32 => (
                quote! { Option<u32> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::U64 => (
                quote! { Option<u64> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::I32 => (
                quote! { Option<i32> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::I64 => (
                quote! { Option<i64> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::F64 => (
                quote! { Option<f64> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::Bool => (
                quote! { Option<bool> },
                quote! { Ok(self.inner.#field_name) },
            ),
            FieldType::String | FieldType::StringN { .. } => (
                quote! { Option<String> },
                quote! { Ok(self.inner.#field_name.clone()) },
            ),
            FieldType::Uuid => (
                quote! { Option<String> },
                quote! { Ok(self.inner.#field_name.map(|__u| __u.to_string())) },
            ),
            FieldType::Timestamp(_) => (
                quote! { Option<String> },
                quote! { Ok(self.inner.#field_name.map(|__t| __t.to_string())) },
            ),
            FieldType::Decimal => (
                quote! { Option<String> },
                quote! { Ok(self.inner.#field_name.map(|__d| __d.to_string())) },
            ),
            FieldType::Enum(_) => (
                quote! { Option<String> },
                quote! {
                    self.inner.#field_name.as_ref()
                        .map(|__e| serde_json::to_value(__e)
                            .map(|__v| __v.as_str().unwrap_or_default().to_string())
                            .map_err(to_py_err))
                        .transpose()
                },
            ),
            _ => (
                quote! { Bound<'py, PyAny> },
                quote! { pythonize::pythonize(py, &self.inner.#field_name).map_err(to_py_err) },
            ),
        }
    }

    fn generate_row_classes(schema: &Schema) -> Vec<TokenStream> {
        Self::identity_models(schema)
            .map(|model| {
                let model_ident = format_ident!("{}", model.name);
                let py_ident = format_ident!("Py{}", model.name);
                let name_str = model.name.as_str();

                let getters: Vec<TokenStream> = model
                    .fields
                    .iter()
                    .filter(|f| !Self::is_virtual_relation(&f.field_type))
                    .map(|f| {
                        let fname = format_ident!("{}", f.name);
                        let (ret_ty, body) = Self::pyo3_getter(schema, &f.field_type, &fname);
                        let needs_py_lifetime = Self::returns_py_bound(&ret_ty);
                        if needs_py_lifetime {
                            quote! {
                                #[getter]
                                fn #fname<'py>(&self, py: Python<'py>) -> PyResult<#ret_ty> {
                                    #body
                                }
                            }
                        } else {
                            quote! {
                                #[getter]
                                fn #fname(&self) -> PyResult<#ret_ty> {
                                    #body
                                }
                            }
                        }
                    })
                    .collect();


                quote! {
                    #[pyclass(name = #name_str)]
                    #[derive(Clone)]
                    pub struct #py_ident {
                        inner: database::#model_ident,
                    }

                    #[pymethods]
                    impl #py_ident {
                        #[new]
                        fn new(data: &Bound<'_, PyAny>) -> PyResult<Self> {
                            let inner: database::#model_ident =
                                pythonize::depythonize(data).map_err(to_py_err)?;
                            Ok(Self { inner })
                        }

                        fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                            pythonize::pythonize(py, &self.inner).map_err(to_py_err)
                        }

                        fn __repr__(&self) -> String {
                            format!("{:?}", self.inner)
                        }

                        #(#getters)*
                    }

                    impl #py_ident {
                        fn from_record(inner: database::#model_ident) -> Self {
                            Self { inner }
                        }
                    }
                }
            })
            .collect()
    }

    fn generate_db_methods(schema: &Schema) -> Vec<TokenStream> {
        Self::identity_models(schema)
            .map(|model| {
                let snake = RustGenerator::to_snake_case(&model.name);
                let model_ident = format_ident!("{}", model.name);
                let py_ident = format_ident!("Py{}", model.name);
                let storage = format_ident!("{}", snake);
                let id_ty = RustGenerator::id_type_tokens(schema, model);

                let create_fn = format_ident!("create_{}", snake);
                let update_fn = format_ident!("update_{}", snake);
                let delete_fn = format_ident!("delete_{}", snake);

                let create_m = format_ident!("create_{}", snake);
                let get_m = format_ident!("get_{}", snake);
                let all_m = format_ident!("all_{}", snake);
                let count_m = format_ident!("count_{}", snake);
                let update_m = format_ident!("update_{}", snake);
                let delete_m = format_ident!("delete_{}", snake);


                quote! {
                    fn #create_m<'py>(
                        &mut self,
                        record: &Bound<'py, PyAny>,
                    ) -> PyResult<Bound<'py, PyAny>> {
                        let py = record.py();
                        let record: database::#model_ident =
                            pythonize::depythonize(record).map_err(to_py_err)?;
                        let id = match catch_unwind(AssertUnwindSafe(|| self.inner.#create_fn(record))) {
                            Ok(r) => r.map_err(to_py_err)?,
                            Err(p) => return Err(panic_to_py_err(p)),
                        };
                        pythonize::pythonize(py, &id).map_err(to_py_err)
                    }

                    fn #get_m(&self, id: &Bound<'_, PyAny>) -> PyResult<Option<#py_ident>> {
                        let id: #id_ty = pythonize::depythonize(id).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#storage.get(id))) {
                            Ok(opt) => Ok(opt.map(#py_ident::from_record)),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }

                    fn #all_m(&self) -> PyResult<Vec<#py_ident>> {
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#storage.all())) {
                            Ok(rows) => Ok(rows.into_iter().map(#py_ident::from_record).collect()),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }

                    fn #count_m(&self) -> PyResult<usize> {
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#storage.row_count())) {
                            Ok(n) => Ok(n),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }

                    fn #update_m(
                        &mut self,
                        id: &Bound<'_, PyAny>,
                        record: &Bound<'_, PyAny>,
                    ) -> PyResult<bool> {
                        let id: #id_ty = pythonize::depythonize(id).map_err(to_py_err)?;
                        let record: database::#model_ident =
                            pythonize::depythonize(record).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#update_fn(id, record))) {
                            Ok(r) => r.map_err(to_py_err),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }

                    fn #delete_m(&mut self, id: &Bound<'_, PyAny>) -> PyResult<bool> {
                        let id: #id_ty = pythonize::depythonize(id).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#delete_fn(id))) {
                            Ok(r) => r.map_err(to_py_err),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }
                }
            })
            .collect()
    }

    fn is_identity_model(model: &forgedb_parser::Model) -> bool {
        model.has_identity()
    }

    fn generate_relation_methods(schema: &Schema) -> Vec<TokenStream> {
        use forgedb_parser::{FieldType, RelationType};
        use std::collections::{HashMap, HashSet};

        let mut methods = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_has_id = Self::is_identity_model(model);
            let source_id_ty = RustGenerator::id_type_tokens(schema, model);
            let storage = format_ident!("{}", model_snake);
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
                if !model_has_id || !Self::is_identity_model(target) {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let py_target = format_ident!("Py{}", target.name);
                methods.push(quote! {
                    fn #method_ident(&self, id: &Bound<'_, PyAny>) -> PyResult<Option<#py_target>> {
                        let id: #source_id_ty = pythonize::depythonize(id).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| {
                            self.inner.#storage.get(id).and_then(|__rec| self.inner.#method_ident(&__rec))
                        })) {
                            Ok(opt) => Ok(opt.map(#py_target::from_record)),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }
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
                format!("{}_{}", RustGenerator::to_snake_case(&p.parent_model), p.parent_field)
            };
            if !seen.insert(method_name.clone()) {
                continue;
            }
            let Some(child) = schema.find_model(&p.child_model) else {
                continue;
            };
            if !Self::is_identity_model(child) {
                continue;
            }
            let method_ident = format_ident!("{}", method_name);
            let py_child = format_ident!("Py{}", child.name);
            let parent_id_ty = RustGenerator::id_type_tokens(schema, parent);
            methods.push(quote! {
                fn #method_ident(&self, id: &Bound<'_, PyAny>) -> PyResult<Vec<#py_child>> {
                    let id: #parent_id_ty = pythonize::depythonize(id).map_err(to_py_err)?;
                    match catch_unwind(AssertUnwindSafe(|| self.inner.#method_ident(id))) {
                        Ok(rows) => Ok(rows.into_iter().map(#py_child::from_record).collect()),
                        Err(p) => Err(panic_to_py_err(p)),
                    }
                }
            });
        }

        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
            let (lk, rk) = RustGenerator::junction_key_idents(schema, &m);
            let model1 = schema.find_model(&m.model1);
            let model2 = schema.find_model(&m.model2);

            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                methods.push(quote! {
                    fn #link_ident(&mut self, left: &Bound<'_, PyAny>, right: &Bound<'_, PyAny>) -> PyResult<()> {
                        let left: #lk = pythonize::depythonize(left).map_err(to_py_err)?;
                        let right: #rk = pythonize::depythonize(right).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#link_ident(left, right))) {
                            Ok(()) => Ok(()),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }
                });
            }

            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                methods.push(quote! {
                    fn #unlink_ident(&mut self, left: &Bound<'_, PyAny>, right: &Bound<'_, PyAny>) -> PyResult<bool> {
                        let left: #lk = pythonize::depythonize(left).map_err(to_py_err)?;
                        let right: #rk = pythonize::depythonize(right).map_err(to_py_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.inner.#unlink_ident(left, right))) {
                            Ok(removed) => Ok(removed),
                            Err(p) => Err(panic_to_py_err(p)),
                        }
                    }
                });
            }

            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                if let Some(model2) = model2 {
                    if Self::is_identity_model(model2) {
                        let fwd_ident = format_ident!("{}", fwd_name);
                        let py_b = format_ident!("Py{}", model2.name);
                        methods.push(quote! {
                            fn #fwd_ident(&self, id: &Bound<'_, PyAny>) -> PyResult<Vec<#py_b>> {
                                let id: #lk = pythonize::depythonize(id).map_err(to_py_err)?;
                                match catch_unwind(AssertUnwindSafe(|| self.inner.#fwd_ident(id))) {
                                    Ok(rows) => Ok(rows.into_iter().map(#py_b::from_record).collect()),
                                    Err(p) => Err(panic_to_py_err(p)),
                                }
                            }
                        });
                    }
                }
            }

            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                if let Some(model1) = model1 {
                    if Self::is_identity_model(model1) {
                        let rev_ident = format_ident!("{}", rev_name);
                        let py_a = format_ident!("Py{}", model1.name);
                        methods.push(quote! {
                            fn #rev_ident(&self, id: &Bound<'_, PyAny>) -> PyResult<Vec<#py_a>> {
                                let id: #rk = pythonize::depythonize(id).map_err(to_py_err)?;
                                match catch_unwind(AssertUnwindSafe(|| self.inner.#rev_ident(id))) {
                                    Ok(rows) => Ok(rows.into_iter().map(#py_a::from_record).collect()),
                                    Err(p) => Err(panic_to_py_err(p)),
                                }
                            }
                        });
                    }
                }
            }
        }

        methods
    }

    fn generate_arrow_spine(schema: &Schema) -> TokenStream {
        if !Self::has_arrow_columns(schema) {
            return quote! {};
        }
        quote! {
            #[repr(C)]
            pub struct ArrowSchema {
                format: *const c_char,
                name: *const c_char,
                metadata: *const c_char,
                flags: i64,
                n_children: i64,
                children: *mut *mut ArrowSchema,
                dictionary: *mut ArrowSchema,
                release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
                private_data: *mut c_void,
            }
            unsafe impl Send for ArrowSchema {}

            #[repr(C)]
            pub struct ArrowArray {
                length: i64,
                null_count: i64,
                offset: i64,
                n_buffers: i64,
                n_children: i64,
                buffers: *mut *const c_void,
                children: *mut *mut ArrowArray,
                dictionary: *mut ArrowArray,
                release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
                private_data: *mut c_void,
            }
            unsafe impl Send for ArrowArray {}

            struct ArrowArrayOwner {
                _export: forgedb_core::forgedb_storage::ColumnExport,
                _buffers: Vec<*const c_void>,
            }

            unsafe extern "C" fn arrow_array_release(array: *mut ArrowArray) {
                if array.is_null() { return; }
                let a = &mut *array;
                if a.release.is_none() { return; }
                if !a.private_data.is_null() {
                    drop(Box::from_raw(a.private_data as *mut ArrowArrayOwner));
                }
                a.private_data = ptr::null_mut();
                a.release = None;
            }

            unsafe extern "C" fn arrow_schema_release(schema: *mut ArrowSchema) {
                if schema.is_null() { return; }
                let s = &mut *schema;
                s.release = None;
            }

            fn arrow_build_primitive(
                format: *const c_char,
                export: forgedb_core::forgedb_storage::ColumnExport,
                length: usize,
            ) -> (ArrowSchema, ArrowArray) {
                let data_ptr = export.as_ptr() as *const c_void;
                let buffers: Vec<*const c_void> = vec![ptr::null(), data_ptr];
                let buffers_ptr = buffers.as_ptr() as *mut *const c_void;
                let owner = Box::new(ArrowArrayOwner { _export: export, _buffers: buffers });
                let array = ArrowArray {
                    length: length as i64,
                    null_count: 0,
                    offset: 0,
                    n_buffers: 2,
                    n_children: 0,
                    buffers: buffers_ptr,
                    children: ptr::null_mut(),
                    dictionary: ptr::null_mut(),
                    release: Some(arrow_array_release),
                    private_data: Box::into_raw(owner) as *mut c_void,
                };
                let schema = ArrowSchema {
                    format,
                    name: ptr::null(),
                    metadata: ptr::null(),
                    flags: 0,
                    n_children: 0,
                    children: ptr::null_mut(),
                    dictionary: ptr::null_mut(),
                    release: Some(arrow_schema_release),
                    private_data: ptr::null_mut(),
                };
                (schema, array)
            }

            fn arrow_format_cstr(fmt: &str) -> *const c_char {
                match fmt {
                    "i" => b"i\0".as_ptr() as *const c_char,
                    "l" => b"l\0".as_ptr() as *const c_char,
                    "I" => b"I\0".as_ptr() as *const c_char,
                    "L" => b"L\0".as_ptr() as *const c_char,
                    "g" => b"g\0".as_ptr() as *const c_char,
                    "w:16" => b"w:16\0".as_ptr() as *const c_char,
                    _ => b"l\0".as_ptr() as *const c_char,
                }
            }

            #[pyclass]
            pub struct ArrowColumn {
                export: Option<forgedb_core::forgedb_storage::ColumnExport>,
                length: usize,
                format: &'static str,
            }

            #[pymethods]
            impl ArrowColumn {
                #[pyo3(signature = (requested_schema=None))]
                fn __arrow_c_array__<'py>(
                    &mut self,
                    py: Python<'py>,
                    requested_schema: Option<Bound<'py, PyAny>>,
                ) -> PyResult<(Bound<'py, PyCapsule>, Bound<'py, PyCapsule>)> {
                    let _ = requested_schema;
                    let export = self.export.take().ok_or_else(|| {
                        ForgeDbError::new_err("Arrow column already exported (single-shot)")
                    })?;
                    let fmt = arrow_format_cstr(self.format);
                    let (schema_val, array_val) = arrow_build_primitive(fmt, export, self.length);
                    let array_cap = PyCapsule::new_with_destructor(
                        py,
                        array_val,
                        Some(CString::new("arrow_array").unwrap()),
                        |mut a: ArrowArray, _ctx: *mut c_void| {
                            if let Some(rel) = a.release {
                                unsafe { rel(&mut a) }
                            }
                        },
                    )?;
                    let schema_cap = PyCapsule::new_with_destructor(
                        py,
                        schema_val,
                        Some(CString::new("arrow_schema").unwrap()),
                        |mut s: ArrowSchema, _ctx: *mut c_void| {
                            if let Some(rel) = s.release {
                                unsafe { rel(&mut s) }
                            }
                        },
                    )?;
                    Ok((schema_cap, array_cap))
                }
            }
        }
    }

    fn generate_arrow_methods(schema: &Schema) -> Vec<TokenStream> {
        let mut methods = Vec::new();
        for model in Self::identity_models(schema) {
            let snake = RustGenerator::to_snake_case(&model.name);
            let storage = format_ident!("{}", snake);
            for field in &model.fields {
                let Some(fmt) = RustGenerator::arrow_export_format(schema, &field.field_type) else {
                    continue;
                };
                let method_ident = format_ident!("{}_{}_arrow", snake, field.name);
                let export_method = format_ident!("export_col_{}", field.name);
                methods.push(quote! {
                    fn #method_ident(&self) -> PyResult<ArrowColumn> {
                        let (export, length) = match catch_unwind(AssertUnwindSafe(|| {
                            let live = self.inner.#storage.export_live_indices();
                            let n = live.len();
                            self.inner.#storage.#export_method(&live).map(|e| (e, n))
                        })) {
                            Ok(Ok(pair)) => pair,
                            Ok(Err(e)) => return Err(to_py_err(e)),
                            Err(p) => return Err(panic_to_py_err(p)),
                        };
                        Ok(ArrowColumn { export: Some(export), length, format: #fmt })
                    }
                });
            }
        }
        methods
    }

    fn generate_arrow_registration(schema: &Schema) -> TokenStream {
        if Self::has_arrow_columns(schema) {
            quote! { m.add_class::<ArrowColumn>()?; }
        } else {
            quote! {}
        }
    }

    fn has_arrow_columns(schema: &Schema) -> bool {
        Self::identity_models(schema).any(|m| {
            m.fields
                .iter()
                .any(|f| RustGenerator::arrow_export_format(schema, &f.field_type).is_some())
        })
    }

    fn generate_registrations(schema: &Schema) -> Vec<TokenStream> {
        Self::identity_models(schema)
            .map(|model| {
                let py_ident = format_ident!("Py{}", model.name);
                quote! { m.add_class::<#py_ident>()?; }
            })
            .collect()
    }

    fn is_virtual_relation(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::{FieldType, RelationType};
        matches!(
            field_type,
            FieldType::Relation(RelationType::OneToMany(_))
                | FieldType::Relation(RelationType::ManyToMany(_))
        )
    }

    pub fn cargo_toml(crate_name: &str, core_package: &str) -> String {
        format!(
            r#"# Generated by ForgeDB. Do not edit — rewritten in full on every generate.
#
[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"

[lib]
#
crate-type = ["cdylib"]

[dependencies]
forgedb_core = {{ package = "{core_package}", path = "../core" }}

pyo3 = {{ version = "0.23", features = ["abi3-py38", "extension-module"] }}
pythonize = "0.23"
serde_json = "1"

[build-dependencies]
pyo3-build-config = "0.23"
"#
        )
    }

    pub fn python_module(schema: &Schema, fingerprint: &str) -> Result<GeneratedCode> {
        let stem = Self::EXTENSION_STEM;
        let mut names: Vec<String> = vec!["ForgeDb".to_string(), "ForgeDbError".to_string()];
        for model in Self::identity_models(schema) {
            names.push(model.name.clone());
        }

        let mut code = format!(
            r#""""Generated by ForgeDB (#337). DO NOT EDIT.

The Python entry point. `forgedb build` compiles the extension and delivers it
beside this file as `{file}`; this module imports it and refuses to hand it to
you if it was built from different generated source than this file was written
from.
"""

__fingerprint__ = "{fingerprint}"

try:
    import {stem} as _native
except ImportError as _exc:  # pragma: no cover - the never-built path
    raise ImportError(
        "ForgeDB: the native extension `{file}` is missing beside "
        + __file__
        + ".\nGenerated source is committed; the compiled extension is not. "
        "Run `forgedb build`."
    ) from _exc

_built = getattr(_native, "__fingerprint__", None)
if _built != __fingerprint__:
    raise ImportError(
        "ForgeDB: the native extension beside "
        + __file__
        + " was built from a different schema than the code beside it.\n"
        "  this module expects: " + __fingerprint__ + "\n"
        "  the extension reports: "
        + ("(no fingerprint - an extension older than this CLI)" if _built is None else _built)
        + "\nRun `forgedb build` to recompile it."
    )

"#,
            file = Self::extension_file(),
            stem = stem,
            fingerprint = fingerprint,
        );

        for name in &names {
            code.push_str(&format!("{name} = _native.{name}\n"));
        }
        code.push_str("\n__all__ = [\n");
        for name in &names {
            code.push_str(&format!("    \"{name}\",\n"));
        }
        code.push_str("]\n");

        Ok(GeneratedCode {
            code,
            description: format!(
                "Python entry module (load-time fingerprint check, {} classes)",
                names.len()
            ),
        })
    }

    pub fn type_stub(schema: &Schema) -> Result<GeneratedCode> {
        let mut code = String::from(
            "# Generated by ForgeDB (#337). DO NOT EDIT.\n             from typing import Any, Optional\n\n             __fingerprint__: str\n\n             class ForgeDbError(Exception): ...\n\n",
        );

        for model in Self::identity_models(schema) {
            code.push_str(&format!("class {}:\n", model.name));
            code.push_str("    def __init__(self, data: dict[str, Any]) -> None: ...\n");
            code.push_str("    def to_dict(self) -> dict[str, Any]: ...\n");
            for field in &model.fields {
                code.push_str(&format!(
                    "    {}: {}\n",
                    field.name,
                    py_stub_type(&field.field_type)
                ));
            }
            code.push('\n');
        }

        code.push_str("class ForgeDb:\n");
        code.push_str("    @staticmethod\n");
        code.push_str("    def open(root: str) -> \"ForgeDb\": ...\n");
        code.push_str("    def commit(self) -> None: ...\n");
        code.push_str("    def checkpoint(self) -> None: ...\n");
        code.push_str("    def compact(self) -> None: ...\n");
        for model in Self::identity_models(schema) {
            let snake = RustGenerator::to_snake_case(&model.name);
            let n = &model.name;
            code.push_str(&format!("    def create_{snake}(self, record: dict[str, Any]) -> Any: ...\n"));
            code.push_str(&format!("    def get_{snake}(self, id: Any) -> Optional[{n}]: ...\n"));
            code.push_str(&format!("    def all_{snake}(self) -> list[{n}]: ...\n"));
            code.push_str(&format!("    def count_{snake}(self) -> int: ...\n"));
            code.push_str(&format!("    def update_{snake}(self, id: Any, record: dict[str, Any]) -> int: ...\n"));
            code.push_str(&format!("    def delete_{snake}(self, id: Any) -> bool: ...\n"));
        }

        Ok(GeneratedCode {
            code,
            description: format!("Python type stub ({} models)", schema.models.len()),
        })
    }

    pub fn build_rs_scaffold() -> &'static str {
        "// Generated by ForgeDB — PyO3 build script (schema-invariant).\n\
         fn main() {\n    \
         pyo3_build_config::add_extension_module_link_args();\n\
         }\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE_PKG: &str = "blog-3f2a1b4c5d6e7f80-core";
    const PYO3_PKG: &str = "blog-3f2a1b4c5d6e7f80-pyo3";

    const SRC: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
}
"#;

    const COMPONENT_SRC: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
  card: tsx://components/user/card
}
"#;

    fn generate(src: &str) -> String {
        let mut parser = forgedb_parser::Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        PyO3Generator::generate(&schema).unwrap().code
    }

    fn flat(code: &str) -> String {
        code.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn the_seam_links_core_rather_than_a_sibling_copy() {
        let flat = flat(&generate(SRC));
        assert!(
            flat.contains("useforgedb_coreasdatabase;"),
            "the wrapper must link the app's `core` cache package"
        );
        assert!(
            !flat.contains("moddatabase;"),
            "the old `mod database;` seam (a sibling database.rs copy) must be gone"
        );
        assert!(
            flat.contains("usedatabase::Database;"),
            "the alias must keep the existing `database::` paths resolving"
        );
    }

    #[test]
    fn the_substrate_is_reached_through_core() {
        let flat = flat(&generate(SRC));
        assert!(
            flat.contains("useforgedb_core::forgedb_types::Uuid;"),
            "Uuid must come through `core`'s re-export"
        );
        assert!(
            !flat.contains("useforgedb_types::Uuid;"),
            "the wrapper still pins `forgedb-types` directly instead of routing through `core`"
        );
        assert!(
            flat.contains("forgedb_core::forgedb_storage::ColumnExport"),
            "the zero-copy export buffer must come through `core`'s re-export"
        );
        assert!(
            !flat.contains("_export:forgedb_storage::ColumnExport"),
            "the wrapper still names `forgedb_storage` absolutely"
        );
    }

    #[test]
    fn a_component_ref_getter_binds_the_py_lifetime() {
        let flat = flat(&generate(COMPONENT_SRC));

        assert!(
            flat.contains("fncard<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>"),
            "the component getter must bind `'py` and take `py`:\n{}",
            generate(COMPONENT_SRC)
        );
        assert!(
            !flat.contains("fncard(&self)->PyResult<Bound<'py,PyAny>>"),
            "the component getter names `'py` without declaring it (nine hard errors)"
        );
    }

    #[test]
    fn the_py_lifetime_is_derived_from_the_return_type() {
        assert!(PyO3Generator::returns_py_bound(&quote! { Bound<'py, PyAny> }));
        assert!(!PyO3Generator::returns_py_bound(&quote! { String }));
        assert!(!PyO3Generator::returns_py_bound(&quote! { Option<String> }));
        assert!(!PyO3Generator::returns_py_bound(&quote! { u32 }));
    }

    #[test]
    fn no_getter_names_the_py_lifetime_without_binding_it() {
        const RET: &str = "->PyResult<Bound<'py,PyAny>>";
        for src in [SRC, COMPONENT_SRC] {
            let code = generate(src);
            let flat = flat(&code);
            let mut from = 0usize;
            let mut seen = 0usize;
            while let Some(at) = flat[from..].find(RET) {
                let end = from + at;
                let start = flat[..end]
                    .rfind("fn")
                    .expect("a signature precedes every return type");
                let sig = &flat[start..end];
                assert!(
                    sig.contains("<'py>"),
                    "this signature names `'py` without binding it: `{sig}`\n\n{code}"
                );
                assert!(
                    sig.contains("py:Python<'py>") || sig.contains("Bound<'py,"),
                    "this signature binds `'py` but has no way to obtain a \
                     `Python<'py>`: `{sig}`\n\n{code}"
                );
                seen += 1;
                from = end + RET.len();
            }
            assert!(seen > 0, "no pythonize-returning signature was examined:\n{code}");
        }
    }

    #[test]
    fn the_pymodule_name_equals_the_extension_stem() {
        let flat = flat(&generate(SRC));
        let needle = format!("#[pymodule]fn{}(", PyO3Generator::EXTENSION_STEM);
        assert!(
            flat.contains(&needle),
            "the #[pymodule] function name must equal EXTENSION_STEM: CPython resolves \
             PyInit_<stem> from the DELIVERED FILENAME, and the delivery table reads the \
             same constant"
        );
        assert!(
            PyO3Generator::extension_file().starts_with(PyO3Generator::EXTENSION_STEM),
            "the delivered filename must carry the stem, or `import` cannot find PyInit_"
        );
    }

    #[test]
    fn the_manifest_parses_as_toml() {
        let manifest = PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG);
        toml::from_str::<toml::Value>(&manifest)
            .unwrap_or_else(|e| panic!("pyo3 manifest is not valid TOML: {e}\n{manifest}"));
    }

    #[test]
    fn the_manifest_pins_no_substrate() {
        let manifest = PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG);
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        let deps = doc["dependencies"].as_table().unwrap();

        let forgedb: Vec<&str> = deps
            .keys()
            .map(|k| k.as_str())
            .filter(|k| k.starts_with("forgedb"))
            .collect();
        assert_eq!(
            forgedb,
            vec!["forgedb_core"],
            "the only ForgeDB dependency may be the app's own `core`: {forgedb:?}"
        );
        let core = doc["dependencies"]["forgedb_core"].as_table().unwrap();
        assert_eq!(core["package"].as_str(), Some(CORE_PKG));
        assert_eq!(core["path"].as_str(), Some("../core"));
    }

    #[test]
    fn the_manifest_sets_no_lib_name() {
        let manifest = PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG);
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        let lib = doc["lib"].as_table().unwrap();
        assert!(
            !lib.contains_key("name"),
            "[lib] name must be derived from the package name: {lib:?}"
        );
        assert_eq!(
            lib["crate-type"].as_array().unwrap()[0].as_str(),
            Some("cdylib")
        );
    }

    #[test]
    fn the_manifest_emits_no_profile_table() {
        let manifest = PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG);
        assert!(!manifest.contains("[profile"), "{manifest}");
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        assert!(doc.get("profile").is_none(), "{manifest}");
    }

    #[test]
    fn a_build_script_supplies_the_extension_module_link_args() {
        let build_rs = PyO3Generator::build_rs_scaffold();
        assert!(
            build_rs.contains("pyo3_build_config::add_extension_module_link_args();"),
            "the build script must emit the macOS `-undefined dynamic_lookup` link args:\n{build_rs}"
        );
        assert!(build_rs.contains("fn main()"), "{build_rs}");

        let manifest = PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG);
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        let build_config = doc["build-dependencies"]["pyo3-build-config"]
            .as_str()
            .unwrap_or_else(|| panic!("pyo3-build-config must be a build-dependency:\n{manifest}"));
        let pyo3 = doc["dependencies"]["pyo3"]["version"].as_str().unwrap();
        assert_eq!(
            build_config, pyo3,
            "pyo3-build-config must be pinned to the same version as pyo3"
        );
    }
}
