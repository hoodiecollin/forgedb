use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::rust::RustGenerator;

struct JsDecl {
    name: String,
    params: String,
    ret: String,
}

impl JsDecl {
    fn new(name: impl Into<String>, params: impl Into<String>, ret: impl Into<String>) -> Self {
        Self { name: name.into(), params: params.into(), ret: ret.into() }
    }

    fn render(&self) -> String {
        format!("  {}({}): {};\n", self.name, self.params, self.ret)
    }
}

fn push_item(methods: &mut Vec<NapiItem>, js: Vec<JsDecl>, item: TokenStream) {
    methods.push(NapiItem { item, js });
}

struct NapiItem {
    item: TokenStream,
    js: Vec<JsDecl>,
}

fn lower_camel_of_snake(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper_next = false;
    for ch in snake.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

pub struct NapiGenerator;

impl NapiGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let row_structs = Self::generate_row_structs(schema);
        let db_methods: Vec<TokenStream> =
            Self::generate_db_methods(schema).into_iter().map(|m| m.item).collect();
        let relation_methods: Vec<TokenStream> =
            Self::generate_relation_methods(schema).into_iter().map(|m| m.item).collect();
        let arrow_methods: Vec<TokenStream> =
            Self::generate_arrow_methods(schema).into_iter().map(|m| m.item).collect();

        let tokens = quote! {
            #![allow(warnings)]

            use forgedb_core as database;

            use napi::bindgen_prelude::*;
            use napi::{Env, JsUnknown, Task};
            use napi_derive::napi;
            use std::panic::{AssertUnwindSafe, catch_unwind};
            use std::sync::{Arc, RwLock};

            use database::Database;
            use forgedb_core::forgedb_types::Uuid;

            mod fingerprint;

            #[napi(js_name = "__forgedbFingerprint")]
            pub fn forgedb_fingerprint() -> String {
                fingerprint::FINGERPRINT.to_string()
            }

            fn to_napi_err<E: std::fmt::Display>(e: E) -> Error {
                Error::from_reason(e.to_string())
            }

            fn panic_to_napi_err(payload: Box<dyn std::any::Any + Send>) -> Error {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "engine panic".to_string());
                Error::from_reason(msg)
            }

            pub struct AsyncOp {
                op: Option<Box<dyn FnOnce() -> std::result::Result<serde_json::Value, String> + Send>>,
            }

            impl AsyncOp {
                fn new(
                    op: impl FnOnce() -> std::result::Result<serde_json::Value, String> + Send + 'static,
                ) -> Self {
                    Self { op: Some(Box::new(op)) }
                }
            }

            impl Task for AsyncOp {
                type Output = serde_json::Value;
                type JsValue = JsUnknown;

                fn compute(&mut self) -> Result<serde_json::Value> {
                    let op = self.op.take().expect("AsyncOp computed exactly once");
                    match catch_unwind(AssertUnwindSafe(move || op())) {
                        Ok(Ok(v)) => Ok(v),
                        Ok(Err(e)) => Err(Error::from_reason(e)),
                        Err(p) => Err(panic_to_napi_err(p)),
                    }
                }

                fn resolve(&mut self, env: Env, output: serde_json::Value) -> Result<JsUnknown> {
                    env.to_js_value(&output).map_err(to_napi_err)
                }
            }

            #(#row_structs)*

            #[napi(js_name = "ForgeDb")]
            pub struct ForgeDb {
                inner: Arc<RwLock<Database>>,
            }

            #[napi]
            impl ForgeDb {
                #[napi(factory)]
                pub fn open(root: String) -> Result<Self> {
                    let root = std::path::PathBuf::from(root);
                    match catch_unwind(AssertUnwindSafe(|| Database::open_at(root))) {
                        Ok(inner) => Ok(Self { inner: Arc::new(RwLock::new(inner)) }),
                        Err(p) => Err(panic_to_napi_err(p)),
                    }
                }

                #[napi]
                pub fn commit(&self) -> Result<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.write().commit())) {
                        Ok(r) => r.map_err(to_napi_err),
                        Err(p) => Err(panic_to_napi_err(p)),
                    }
                }

                #[napi]
                pub fn checkpoint(&self) -> Result<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.write().checkpoint())) {
                        Ok(()) => Ok(()),
                        Err(p) => Err(panic_to_napi_err(p)),
                    }
                }

                #[napi]
                pub fn compact(&self) -> Result<()> {
                    match catch_unwind(AssertUnwindSafe(|| self.write().compact())) {
                        Ok(()) => Ok(()),
                        Err(p) => Err(panic_to_napi_err(p)),
                    }
                }

                #[napi(js_name = "commitAsync")]
                pub fn commit_async(&self) -> AsyncTask<AsyncOp> {
                    let inner = self.inner.clone();
                    AsyncTask::new(AsyncOp::new(move || {
                        let mut db = inner.write().unwrap_or_else(|e| e.into_inner());
                        db.commit().map_err(|e| e.to_string())?;
                        Ok(serde_json::Value::Null)
                    }))
                }

                #(#db_methods)*

                #(#relation_methods)*

                #(#arrow_methods)*
            }

            impl ForgeDb {
                fn read(&self) -> std::sync::RwLockReadGuard<'_, Database> {
                    self.inner.read().unwrap_or_else(|e| e.into_inner())
                }

                fn write(&self) -> std::sync::RwLockWriteGuard<'_, Database> {
                    self.inner.write().unwrap_or_else(|e| e.into_inner())
                }
            }
        };

        let syntax_tree = syn::parse_file(&tokens.to_string()).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!(
                "Failed to parse generated NAPI-RS binding: {e}"
            ))
        })?;
        let code = prettyplease::unparse(&syntax_tree);

        Ok(GeneratedCode {
            code,
            description: format!("Node/Bun binding (NAPI-RS) ({} models)", schema.models.len()),
        })
    }

    fn identity_models(schema: &Schema) -> impl Iterator<Item = &forgedb_parser::Model> {
        schema
            .models
            .iter()
            .filter(|m| m.has_identity())
    }

    fn is_virtual_relation(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::{FieldType, RelationType};
        matches!(
            field_type,
            FieldType::Relation(RelationType::OneToMany(_))
                | FieldType::Relation(RelationType::ManyToMany(_))
        )
    }

    fn napi_field_type(schema: &Schema, field_type: &forgedb_parser::FieldType) -> Option<TokenStream> {
        use forgedb_parser::{FieldType, RelationType};
        match field_type {
            FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_)) => None,

            FieldType::U32 => Some(quote! { u32 }),
            FieldType::U64 => Some(quote! { i64 }),
            FieldType::I32 => Some(quote! { i32 }),
            FieldType::I64 => Some(quote! { i64 }),
            FieldType::F64 => Some(quote! { f64 }),
            FieldType::Bool => Some(quote! { bool }),

            FieldType::String | FieldType::StringN { .. } => Some(quote! { String }),

            FieldType::Uuid => Some(quote! { String }),

            FieldType::Timestamp(_) => Some(quote! { String }),

            FieldType::Decimal => Some(quote! { String }),

            FieldType::Enum(_) => Some(quote! { String }),

            FieldType::Json => Some(quote! { serde_json::Value }),

            FieldType::Bytes(_) => Some(quote! { Vec<u8> }),

            FieldType::FixedArray(inner, _) => {
                let inner_ty = Self::napi_field_type(schema, inner)?;
                Some(quote! { Vec<#inner_ty> })
            }

            FieldType::StructType(_) | FieldType::OptionalStructType(_) => {
                Some(quote! { serde_json::Value })
            }

            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::napi_field_type(schema, &RustGenerator::resolved_type(schema, field_type)),

            FieldType::Nullable(inner) => {
                let inner_ty = Self::napi_field_type(schema, inner)?;
                Some(quote! { Option<#inner_ty> })
            }

            _ => Some(quote! { serde_json::Value }),
        }
    }

    fn napi_field_conv(schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        field_name: &proc_macro2::Ident,
    ) -> TokenStream {
        use forgedb_parser::{FieldType, RelationType};
        match field_type {
            FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_)) => quote! { () },

            FieldType::U32 | FieldType::I32 | FieldType::I64 | FieldType::F64 | FieldType::Bool => {
                quote! { __src.#field_name }
            }

            FieldType::U64 => quote! { __src.#field_name as i64 },

            FieldType::String | FieldType::StringN { .. } => {
                quote! { __src.#field_name.clone() }
            }

            FieldType::Uuid => quote! { __src.#field_name.to_string() },

            FieldType::Timestamp(_) => quote! { __src.#field_name.to_string() },

            FieldType::Decimal => quote! { __src.#field_name.to_string() },

            FieldType::Enum(_) => {
                quote! { serde_json::to_value(&__src.#field_name).unwrap_or_default().as_str().unwrap_or_default().to_string() }
            }

            FieldType::Json => quote! { __src.#field_name.clone() },

            FieldType::Bytes(_) => quote! { __src.#field_name.to_vec() },

            FieldType::FixedArray(inner, _) => {
                let elem_conv = Self::napi_scalar_conv(inner, &quote! { __e });
                quote! { __src.#field_name.iter().map(|__e| #elem_conv).collect() }
            }

            FieldType::StructType(_) | FieldType::OptionalStructType(_) => {
                quote! { serde_json::to_value(&__src.#field_name).unwrap_or_default() }
            }

            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::napi_field_conv(
                schema,
                &RustGenerator::resolved_type(schema, field_type),
                field_name,
            ),

            FieldType::Nullable(inner) => {
                let inner_conv = Self::napi_nullable_inner_conv(inner, field_name);
                quote! { #inner_conv }
            }

            _ => quote! { serde_json::to_value(&__src.#field_name).unwrap_or_default() },
        }
    }

    fn napi_nullable_inner_conv(
        inner: &forgedb_parser::FieldType,
        field_name: &proc_macro2::Ident,
    ) -> TokenStream {
        use forgedb_parser::FieldType;
        match inner {
            FieldType::String | FieldType::StringN { .. } => {
                quote! { __src.#field_name.clone() }
            }
            FieldType::U32 | FieldType::I32 | FieldType::I64 | FieldType::F64 | FieldType::Bool => {
                quote! { __src.#field_name }
            }
            FieldType::U64 => quote! { __src.#field_name.map(|__v| __v as i64) },
            FieldType::Uuid => quote! { __src.#field_name.map(|__u| __u.to_string()) },
            FieldType::Timestamp(_) => quote! { __src.#field_name.map(|__t| __t.to_string()) },
            FieldType::Decimal => quote! { __src.#field_name.map(|__d| __d.to_string()) },
            FieldType::Enum(_) => {
                quote! {
                    __src.#field_name.as_ref().and_then(|__e|
                        serde_json::to_value(__e).ok()
                            .and_then(|__v| __v.as_str().map(|__s| __s.to_string()))
                    )
                }
            }
            FieldType::Json => quote! { __src.#field_name.clone() },
            FieldType::Bytes(_) => quote! { __src.#field_name.map(|__b| __b.to_vec()) },
            FieldType::StructType(_) | FieldType::OptionalStructType(_) => {
                quote! { __src.#field_name.as_ref().map(|__s| serde_json::to_value(__s).unwrap_or_default()) }
            }
            _ => quote! { __src.#field_name.as_ref().map(|__v| serde_json::to_value(__v).unwrap_or_default()) },
        }
    }

    fn napi_scalar_conv(inner: &forgedb_parser::FieldType, elem_expr: &TokenStream) -> TokenStream {
        use forgedb_parser::FieldType;
        match inner {
            FieldType::U32 | FieldType::I32 | FieldType::I64 | FieldType::F64 | FieldType::Bool => {
                quote! { *#elem_expr }
            }
            FieldType::U64 => quote! { *#elem_expr as i64 },
            FieldType::Uuid => quote! { #elem_expr.to_string() },
            _ => quote! { serde_json::to_value(#elem_expr).unwrap_or_default() },
        }
    }

    fn generate_row_structs(schema: &Schema) -> Vec<TokenStream> {
        Self::identity_models(schema)
            .map(|model| {
                let model_ident = format_ident!("{}", model.name);
                let napi_ident = format_ident!("Napi{}", model.name);
                let napi_name_str = model.name.as_str();

                let fields: Vec<_> = model
                    .fields
                    .iter()
                    .filter(|f| !Self::is_virtual_relation(&f.field_type))
                    .filter_map(|f| {
                        let fname = format_ident!("{}", f.name);
                        let ty = Self::napi_field_type(schema, &f.field_type)?;
                        let js_key = f.name.as_str();
                        Some(quote! {
                            #[napi(js_name = #js_key)]
                            pub #fname: #ty
                        })
                    })
                    .collect();

                let conv_exprs: Vec<_> = model
                    .fields
                    .iter()
                    .filter(|f| !Self::is_virtual_relation(&f.field_type))
                    .filter_map(|f| {
                        let fname = format_ident!("{}", f.name);
                        let _ = Self::napi_field_type(schema, &f.field_type)?;
                        let conv = Self::napi_field_conv(schema, &f.field_type, &fname);
                        Some(quote! { #fname: #conv })
                    })
                    .collect();

                quote! {
                    #[napi(object, js_name = #napi_name_str)]
                    pub struct #napi_ident {
                        #(#fields),*
                    }

                    impl #napi_ident {
                        fn from_record(__src: &database::#model_ident) -> Self {
                            Self {
                                #(#conv_exprs),*
                            }
                        }
                    }
                }
            })
            .collect()
    }

    fn generate_db_methods(schema: &Schema) -> Vec<NapiItem> {
        Self::identity_models(schema)
            .map(|model| {
                let snake = RustGenerator::to_snake_case(&model.name);
                let model_ident = format_ident!("{}", model.name);
                let napi_ident = format_ident!("Napi{}", model.name);
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

                let create_async_m = format_ident!("create_{}_async", snake);
                let get_async_m = format_ident!("get_{}_async", snake);
                let all_async_m = format_ident!("all_{}_async", snake);
                let update_async_m = format_ident!("update_{}_async", snake);
                let delete_async_m = format_ident!("delete_{}_async", snake);
                let create_async_js = format!("create{}Async", model.name);
                let get_async_js = format!("get{}Async", model.name);
                let all_async_js = format!("all{}Async", model.name);
                let update_async_js = format!("update{}Async", model.name);
                let delete_async_js = format!("delete{}Async", model.name);

                let n = &model.name;
                let js = vec![
                    JsDecl::new(lower_camel_of_snake(&format!("create_{snake}")), "record: unknown", "unknown"),
                    JsDecl::new(lower_camel_of_snake(&format!("get_{snake}")), "id: unknown", format!("{n} | null")),
                    JsDecl::new(lower_camel_of_snake(&format!("all_{snake}")), "", format!("{n}[]")),
                    JsDecl::new(lower_camel_of_snake(&format!("count_{snake}")), "", "number"),
                    JsDecl::new(lower_camel_of_snake(&format!("update_{snake}")), "id: unknown, record: unknown", "boolean"),
                    JsDecl::new(lower_camel_of_snake(&format!("delete_{snake}")), "id: unknown", "boolean"),
                    JsDecl::new(create_async_js.clone(), "record: unknown", "Promise<unknown>"),
                    JsDecl::new(get_async_js.clone(), "id: unknown", "Promise<unknown>"),
                    JsDecl::new(all_async_js.clone(), "", "Promise<unknown>"),
                    JsDecl::new(update_async_js.clone(), "id: unknown, record: unknown", "Promise<unknown>"),
                    JsDecl::new(delete_async_js.clone(), "id: unknown", "Promise<unknown>"),
                ];


                let item = quote! {
                    #[napi]
                    pub fn #create_m(&self, env: Env, record: JsUnknown) -> Result<JsUnknown> {
                        let record: database::#model_ident =
                            env.from_js_value(record).map_err(to_napi_err)?;
                        let id = match catch_unwind(AssertUnwindSafe(|| self.write().#create_fn(record))) {
                            Ok(r) => r.map_err(to_napi_err)?,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        env.to_js_value(&id).map_err(to_napi_err)
                    }

                    #[napi]
                    pub fn #get_m(&self, env: Env, id: JsUnknown) -> Result<Option<#napi_ident>> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let opt = match catch_unwind(AssertUnwindSafe(|| self.read().#storage.get(id))) {
                            Ok(opt) => opt,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        Ok(opt.as_ref().map(#napi_ident::from_record))
                    }

                    #[napi]
                    pub fn #all_m(&self) -> Result<Vec<#napi_ident>> {
                        let rows = match catch_unwind(AssertUnwindSafe(|| self.read().#storage.all())) {
                            Ok(rows) => rows,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        Ok(rows.iter().map(#napi_ident::from_record).collect())
                    }

                    #[napi]
                    pub fn #count_m(&self) -> Result<i64> {
                        match catch_unwind(AssertUnwindSafe(|| self.read().#storage.row_count())) {
                            Ok(n) => Ok(n as i64),
                            Err(p) => Err(panic_to_napi_err(p)),
                        }
                    }

                    #[napi]
                    pub fn #update_m(
                        &self,
                        env: Env,
                        id: JsUnknown,
                        record: JsUnknown,
                    ) -> Result<bool> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let record: database::#model_ident =
                            env.from_js_value(record).map_err(to_napi_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.write().#update_fn(id, record))) {
                            Ok(r) => r.map_err(to_napi_err),
                            Err(p) => Err(panic_to_napi_err(p)),
                        }
                    }

                    #[napi]
                    pub fn #delete_m(&self, env: Env, id: JsUnknown) -> Result<bool> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.write().#delete_fn(id))) {
                            Ok(r) => r.map_err(to_napi_err),
                            Err(p) => Err(panic_to_napi_err(p)),
                        }
                    }

                    #[napi(js_name = #create_async_js)]
                    pub fn #create_async_m(&self, env: Env, record: JsUnknown) -> Result<AsyncTask<AsyncOp>> {
                        let record: database::#model_ident =
                            env.from_js_value(record).map_err(to_napi_err)?;
                        let inner = self.inner.clone();
                        Ok(AsyncTask::new(AsyncOp::new(move || {
                            let mut db = inner.write().unwrap_or_else(|e| e.into_inner());
                            let id = db.#create_fn(record).map_err(|e| e.to_string())?;
                            serde_json::to_value(&id).map_err(|e| e.to_string())
                        })))
                    }

                    #[napi(js_name = #get_async_js)]
                    pub fn #get_async_m(&self, env: Env, id: JsUnknown) -> Result<AsyncTask<AsyncOp>> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let inner = self.inner.clone();
                        Ok(AsyncTask::new(AsyncOp::new(move || {
                            let db = inner.read().unwrap_or_else(|e| e.into_inner());
                            let opt = db.#storage.get(id);
                            serde_json::to_value(&opt).map_err(|e| e.to_string())
                        })))
                    }

                    #[napi(js_name = #all_async_js)]
                    pub fn #all_async_m(&self) -> AsyncTask<AsyncOp> {
                        let inner = self.inner.clone();
                        AsyncTask::new(AsyncOp::new(move || {
                            let db = inner.read().unwrap_or_else(|e| e.into_inner());
                            let rows = db.#storage.all();
                            serde_json::to_value(&rows).map_err(|e| e.to_string())
                        }))
                    }

                    #[napi(js_name = #update_async_js)]
                    pub fn #update_async_m(&self, env: Env, id: JsUnknown, record: JsUnknown) -> Result<AsyncTask<AsyncOp>> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let record: database::#model_ident =
                            env.from_js_value(record).map_err(to_napi_err)?;
                        let inner = self.inner.clone();
                        Ok(AsyncTask::new(AsyncOp::new(move || {
                            let mut db = inner.write().unwrap_or_else(|e| e.into_inner());
                            let ok = db.#update_fn(id, record).map_err(|e| e.to_string())?;
                            serde_json::to_value(ok).map_err(|e| e.to_string())
                        })))
                    }

                    #[napi(js_name = #delete_async_js)]
                    pub fn #delete_async_m(&self, env: Env, id: JsUnknown) -> Result<AsyncTask<AsyncOp>> {
                        let id: #id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let inner = self.inner.clone();
                        Ok(AsyncTask::new(AsyncOp::new(move || {
                            let mut db = inner.write().unwrap_or_else(|e| e.into_inner());
                            let ok = db.#delete_fn(id).map_err(|e| e.to_string())?;
                            serde_json::to_value(ok).map_err(|e| e.to_string())
                        })))
                    }
                };
                NapiItem { item, js }
            })
            .collect()
    }

    fn generate_relation_methods(schema: &Schema) -> Vec<NapiItem> {
        use forgedb_parser::{FieldType, RelationType};
        use std::collections::{HashMap, HashSet};

        let mut methods: Vec<NapiItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_has_id = model.has_identity();
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
                if !model_has_id {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let napi_target = format_ident!("Napi{}", target.name);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&method_name),
                    "id: unknown",
                    format!("{} | null", target.name),
                )];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #method_ident(&self, env: Env, id: JsUnknown) -> Result<Option<#napi_target>> {
                        let id: #source_id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                        let resolved = match catch_unwind(AssertUnwindSafe(|| {
                            let __db = self.read();
                            __db.#storage.get(id).and_then(|__rec| __db.#method_ident(&__rec))
                        })) {
                            Ok(opt) => opt,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        Ok(resolved.as_ref().map(#napi_target::from_record))
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
            let method_ident = format_ident!("{}", method_name);
            let napi_child = format_ident!("Napi{}", child.name);
            let parent_id_ty = RustGenerator::id_type_tokens(schema, parent);
            let js = vec![JsDecl::new(
                lower_camel_of_snake(&method_name),
                "id: unknown",
                format!("{}[]", child.name),
            )];
            push_item(&mut methods, js, quote! {
                #[napi]
                pub fn #method_ident(&self, env: Env, id: JsUnknown) -> Result<Vec<#napi_child>> {
                    let id: #parent_id_ty = env.from_js_value(id).map_err(to_napi_err)?;
                    let rows = match catch_unwind(AssertUnwindSafe(|| self.read().#method_ident(id))) {
                        Ok(rows) => rows,
                        Err(p) => return Err(panic_to_napi_err(p)),
                    };
                    Ok(rows.iter().map(#napi_child::from_record).collect())
                }
            });
        }

        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
            let (lk, rk) = RustGenerator::junction_key_idents(schema, &m);

            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&link_name), "left: unknown, right: unknown", "void")];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #link_ident(&self, env: Env, left: JsUnknown, right: JsUnknown) -> Result<()> {
                        let left: #lk = env.from_js_value(left).map_err(to_napi_err)?;
                        let right: #rk = env.from_js_value(right).map_err(to_napi_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.write().#link_ident(left, right))) {
                            Ok(()) => Ok(()),
                            Err(p) => Err(panic_to_napi_err(p)),
                        }
                    }
                });
            }

            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&unlink_name), "left: unknown, right: unknown", "boolean")];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #unlink_ident(&self, env: Env, left: JsUnknown, right: JsUnknown) -> Result<bool> {
                        let left: #lk = env.from_js_value(left).map_err(to_napi_err)?;
                        let right: #rk = env.from_js_value(right).map_err(to_napi_err)?;
                        match catch_unwind(AssertUnwindSafe(|| self.write().#unlink_ident(left, right))) {
                            Ok(removed) => Ok(removed),
                            Err(p) => Err(panic_to_napi_err(p)),
                        }
                    }
                });
            }

            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                let fwd_ident = format_ident!("{}", fwd_name);
                let napi_b = format_ident!("Napi{}", m.model2);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&fwd_name), "id: unknown", format!("{}[]", m.model2))];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #fwd_ident(&self, env: Env, id: JsUnknown) -> Result<Vec<#napi_b>> {
                        let id: #lk = env.from_js_value(id).map_err(to_napi_err)?;
                        let rows = match catch_unwind(AssertUnwindSafe(|| self.read().#fwd_ident(id))) {
                            Ok(rows) => rows,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        Ok(rows.iter().map(#napi_b::from_record).collect())
                    }
                });
            }

            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                let rev_ident = format_ident!("{}", rev_name);
                let napi_a = format_ident!("Napi{}", m.model1);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&rev_name), "id: unknown", format!("{}[]", m.model1))];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #rev_ident(&self, env: Env, id: JsUnknown) -> Result<Vec<#napi_a>> {
                        let id: #rk = env.from_js_value(id).map_err(to_napi_err)?;
                        let rows = match catch_unwind(AssertUnwindSafe(|| self.read().#rev_ident(id))) {
                            Ok(rows) => rows,
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        Ok(rows.iter().map(#napi_a::from_record).collect())
                    }
                });
            }
        }

        methods
    }

    fn generate_arrow_methods(schema: &Schema) -> Vec<NapiItem> {
        let mut methods: Vec<NapiItem> = Vec::new();
        for model in Self::identity_models(schema) {
            let snake = RustGenerator::to_snake_case(&model.name);
            let storage = format_ident!("{}", snake);
            for field in &model.fields {
                let Some(fmt) = RustGenerator::arrow_export_format(schema, &field.field_type) else {
                    continue;
                };
                let method_ident = format_ident!("{}_{}_arrow", snake, field.name);
                let export_method = format_ident!("export_col_{}", field.name);
                let js = vec![JsDecl::new(
                    lower_camel_of_snake(&format!("{snake}_{}_arrow", field.name)),
                    "",
                    "ArrowColumn",
                )];
                push_item(&mut methods, js, quote! {
                    #[napi]
                    pub fn #method_ident(&self, env: Env) -> Result<JsUnknown> {
                        let (export, length) = match catch_unwind(AssertUnwindSafe(|| {
                            let __db = self.read();
                            let live = __db.#storage.export_live_indices();
                            let n = live.len();
                            __db.#storage.#export_method(&live).map(|e| (e, n))
                        })) {
                            Ok(Ok(pair)) => pair,
                            Ok(Err(e)) => return Err(to_napi_err(e)),
                            Err(p) => return Err(panic_to_napi_err(p)),
                        };
                        let data_ptr = export.as_ptr() as *mut u8;
                        let byte_len = export.len();
                        let ab = unsafe {
                            env.create_arraybuffer_with_borrowed_data(
                                data_ptr,
                                byte_len,
                                export,
                                |_export: forgedb_core::forgedb_storage::ColumnExport, _env: Env| {},
                            )
                        }.map_err(to_napi_err)?;
                        let mut obj = env.create_object().map_err(to_napi_err)?;
                        obj.set_named_property("buffer", ab.into_raw()).map_err(to_napi_err)?;
                        obj.set_named_property("format", #fmt).map_err(to_napi_err)?;
                        obj.set_named_property("length", length as u32).map_err(to_napi_err)?;
                        Ok(obj.into_unknown())
                    }
                });
            }
        }
        methods
    }

    pub fn cargo_toml(crate_name: &str, core_package: &str) -> String {
        format!(
            r#"# Generated by ForgeDB. Do not edit — rewritten in full on every generate.
#
# NOTHING IN THE CACHE IS USER-EDITABLE. The scaffolds in your output directory
# are written only when absent, because they are yours; this file is ForgeDB's.
[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"

[lib]
# cdylib: the Node-API addon (`.node`) both Node and Bun load (Option A).
#
# `name` is deliberately unset — cargo derives it from the per-app package name.
# Hardcoding it made two apps in one workspace collide at a cargo WARNING and
# exit 0. The delivered filename is a delivery-time rename.
crate-type = ["cdylib"]

[dependencies]
# The one generated database for this app. Renamed so no generated source
# carries this app's package name, and the SOLE route to the substrate: this crate pins
# none of it, so `Uuid`/`ColumnExport` here ARE the database's types.
forgedb_core = {{ package = "{core_package}", path = "../core" }}

napi = {{ version = "2", default-features = false, features = ["napi8", "serde-json"] }}
napi-derive = "2"
serde_json = "1"

[build-dependencies]
napi-build = "2"
"#
        )
    }

    pub fn build_rs_scaffold() -> &'static str {
        "// Generated by ForgeDB — NAPI-RS build script (schema-invariant).\n\
         fn main() {\n    \
         napi_build::setup();\n\
         }\n"
    }

    pub fn package_json_scaffold() -> &'static str {
        r#"{
  "name": "forgedb",
  "version": "0.1.0",
  "description": "Generated ForgeDB Node/Bun binding (NAPI-RS)",
  "main": "index.js",
  "types": "index.d.ts"
}
"#
    }

    pub const LEGACY_MAIN: &str = "forgedb.node";

    pub fn entry_module(fingerprint: &str) -> GeneratedCode {
        let code = format!(
            r#"// Generated by ForgeDB (#337). DO NOT EDIT.
'use strict';

const path = require('path');

const FINGERPRINT = '{fingerprint}';

const ADDON = path.join(__dirname, 'forgedb.node');

let addon;
try {{
  addon = require(ADDON);
}} catch (err) {{
  if (err && err.code === 'MODULE_NOT_FOUND') {{
    throw new Error(
      'ForgeDB: the native addon is missing (' + ADDON + ').\n' +
      'Generated source is committed; the compiled addon is not. Run `forgedb build`.'
    );
  }}
  throw err;
}}

const built = typeof addon.__forgedbFingerprint === 'function'
  ? addon.__forgedbFingerprint()
  : undefined;

if (built !== FINGERPRINT) {{
  throw new Error(
    'ForgeDB: ' + ADDON + ' was built from a different schema than the code beside it.\n' +
    '  this module expects: ' + FINGERPRINT + '\n' +
    '  the addon reports:   ' + (built === undefined ? '(no fingerprint — an addon older than this CLI)' : built) + '\n' +
    'Run `forgedb build` to recompile it.'
  );
}}

module.exports = addon;
"#
        );
        GeneratedCode {
            code,
            description: "Node/Bun entry module (load-time fingerprint check)".to_string(),
        }
    }

    pub fn type_declarations(schema: &Schema) -> Result<GeneratedCode> {
        let mut d = String::new();
        d.push_str("// Generated by ForgeDB (#337). DO NOT EDIT.\n");

        for model in schema.models.iter() {
            d.push_str(&format!("export interface {} {{\n", model.name));
            for field in &model.fields {
                let ty = crate::typescript::TypeScriptGenerator::map_field_type(&field.field_type);
                let nullable = if field.is_nullable() { " | null" } else { "" };
                d.push_str(&format!("  {}: {}{};\n", field.name, ty, nullable));
            }
            d.push_str("}\n\n");
        }

        let methods: Vec<NapiItem> = Self::generate_db_methods(schema)
            .into_iter()
            .chain(Self::generate_relation_methods(schema))
            .chain(Self::generate_arrow_methods(schema))
            .collect();
        let decls: Vec<&JsDecl> = methods.iter().flat_map(|m| m.js.iter()).collect();

        if decls.iter().any(|j| j.ret == "ArrowColumn") {
            d.push_str("export interface ArrowColumn {\n");
            d.push_str("  buffer: ArrayBuffer;\n");
            d.push_str("  format: string;\n");
            d.push_str("  length: number;\n");
            d.push_str("}\n\n");
        }

        d.push_str("export declare class ForgeDb {\n");
        d.push_str("  static open(root: string): ForgeDb;\n");
        d.push_str("  commit(): void;\n");
        d.push_str("  checkpoint(): void;\n");
        d.push_str("  compact(): void;\n");
        d.push_str("  commitAsync(): Promise<void>;\n");
        for js in &decls {
            d.push_str(&js.render());
        }
        d.push_str("}\n\n");
        d.push_str("export declare function __forgedbFingerprint(): string;\n");

        Ok(GeneratedCode {
            code: d,
            description: format!(
                "Node/Bun type declarations ({} models, {} methods)",
                schema.models.len(),
                decls.len() + 5
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE_PKG: &str = "blog-3f2a1b4c5d6e7f80-core";
    const NAPI_PKG: &str = "blog-3f2a1b4c5d6e7f80-napi";

    const RELATIONAL_SRC: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  name: string
  posts: [Post]
}
"#;

    fn relational_schema() -> forgedb_parser::Schema {
        let mut parser = forgedb_parser::Parser::new(RELATIONAL_SRC).unwrap();
        parser.parse().unwrap()
    }

    fn exported_js_methods(code: &str) -> Vec<String> {
        const HEAD: &str = "#[napi]\nimpl ForgeDb {";
        let start = code
            .find(HEAD)
            .expect("the generated binding must contain a `#[napi] impl ForgeDb` block");
        let rest = &code[start + HEAD.len()..];
        let end = rest
            .find("\n}\n")
            .expect("the `impl ForgeDb` block must close at column 0");
        let block = &rest[..end];

        let mut out = Vec::new();
        for (i, line) in block.lines().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("#[napi") {
                continue;
            }
            if let Some(js) = t.split("js_name = \"").nth(1).and_then(|r| r.split('"').next()) {
                out.push(js.to_string());
                continue;
            }
            let Some(sig) = block
                .lines()
                .skip(i + 1)
                .find(|l| l.trim_start().starts_with("pub fn "))
            else {
                continue;
            };
            let name = sig.trim_start()["pub fn ".len()..]
                .split(['(', '<'])
                .next()
                .unwrap()
                .trim()
                .to_string();
            out.push(super::lower_camel_of_snake(&name));
        }
        out
    }

    fn declared_js_methods(dts: &str) -> Vec<String> {
        let start = dts
            .find("export declare class ForgeDb {")
            .expect("the declarations must contain the `ForgeDb` class");
        let rest = &dts[start..];
        let end = rest
            .find("\n}\n")
            .expect("the class body must close at column 0");
        rest[..end]
            .lines()
            .filter_map(|l| {
                let t = l.trim_start().trim_start_matches("static ");
                let (name, tail) = t.split_once('(')?;
                if !tail.contains(')') || name.is_empty() {
                    return None;
                }
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    .then(|| name.to_string())
            })
            .collect()
    }

    #[test]
    fn the_declared_surface_is_the_exported_surface() {
        let schema = relational_schema();
        let code = NapiGenerator::generate(&schema).unwrap().code;
        let dts = NapiGenerator::type_declarations(&schema).unwrap().code;

        let exported = exported_js_methods(&code);
        let declared = declared_js_methods(&dts);

        assert!(
            exported.len() > 40,
            "extractor found only {} exported methods — it stopped matching: {:?}",
            exported.len(),
            exported
        );
        let e: std::collections::BTreeSet<_> = exported.iter().collect();
        let d: std::collections::BTreeSet<_> = declared.iter().collect();
        let undeclared: Vec<_> = e.difference(&d).collect();
        let phantom: Vec<_> = d.difference(&e).collect();
        assert!(
            undeclared.is_empty(),
            "exported by the addon, undeclared in index.d.ts (a type error to call): {undeclared:?}"
        );
        assert!(
            phantom.is_empty(),
            "declared in index.d.ts but not exported (a runtime TypeError): {phantom:?}"
        );
        assert_eq!(
            exported.len(),
            declared.len(),
            "the same names, but not the same number of them — one side has a duplicate"
        );
    }

    #[test]
    fn the_relation_families_reach_the_declarations() {
        let dts = NapiGenerator::type_declarations(&relational_schema())
            .unwrap()
            .code;
        for (needle, why) in [
            ("postAuthor(id: unknown): User | null", "forward FK getter"),
            ("userPosts(id: unknown): Post[]", "reverse one-to-many getter"),
            ("linkPostTag(left: unknown, right: unknown): void", "m2m link"),
            (
                "unlinkPostTag(left: unknown, right: unknown): boolean",
                "m2m unlink",
            ),
            ("postTags(id: unknown): Tag[]", "m2m forward query"),
            ("tagPosts(id: unknown): Post[]", "m2m reverse query"),
            ("userViewsArrow(): ArrowColumn", "Arrow column export"),
            ("export interface ArrowColumn", "the Arrow result shape"),
        ] {
            assert!(dts.contains(needle), "{why} missing from index.d.ts: {needle}");
        }
    }

    #[test]
    fn update_is_declared_boolean_not_number() {
        let dts = NapiGenerator::type_declarations(&relational_schema())
            .unwrap()
            .code;
        assert!(
            dts.contains("updateUser(id: unknown, record: unknown): boolean"),
            "update must be declared `boolean`: {dts}"
        );
        assert!(
            !dts.contains("): number;\n  deleteUser"),
            "the old `number` return survived"
        );
    }

    const SRC: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
}
"#;

    fn generated() -> String {
        let mut parser = forgedb_parser::Parser::new(SRC).unwrap();
        let schema = parser.parse().unwrap();
        NapiGenerator::generate(&schema).unwrap().code
    }

    fn flat(code: &str) -> String {
        code.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn the_seam_links_core_rather_than_a_sibling_copy() {
        let flat = flat(&generated());
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
        let flat = flat(&generated());
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
    fn the_manifest_parses_as_toml() {
        let manifest = NapiGenerator::cargo_toml(NAPI_PKG, CORE_PKG);
        toml::from_str::<toml::Value>(&manifest)
            .unwrap_or_else(|e| panic!("napi manifest is not valid TOML: {e}\n{manifest}"));
    }

    #[test]
    fn the_manifest_pins_no_substrate() {
        let manifest = NapiGenerator::cargo_toml(NAPI_PKG, CORE_PKG);
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
        let manifest = NapiGenerator::cargo_toml(NAPI_PKG, CORE_PKG);
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
        let manifest = NapiGenerator::cargo_toml(NAPI_PKG, CORE_PKG);
        assert!(!manifest.contains("[profile"), "{manifest}");
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        assert!(doc.get("profile").is_none(), "{manifest}");
    }

    #[test]
    fn the_package_json_does_not_reach_for_napi_cli() {
        let pkg = NapiGenerator::package_json_scaffold();
        let doc: serde_json::Value = serde_json::from_str(pkg).expect("package.json is valid JSON");

        assert!(
            doc.get("devDependencies").is_none(),
            "@napi-rs/cli must not be a dependency of anything: {pkg}"
        );
        assert!(
            doc.get("scripts").is_none(),
            "there is no `napi build` step any more: {pkg}"
        );
        assert!(
            !pkg.contains("napi build") && !pkg.contains("@napi-rs/cli"),
            "{pkg}"
        );
        assert_eq!(doc["main"].as_str(), Some("index.js"));
        assert_eq!(doc["types"].as_str(), Some("index.d.ts"));
    }

    #[test]
    fn the_build_script_still_wires_the_platform_link_args() {
        assert!(
            NapiGenerator::build_rs_scaffold().contains("napi_build::setup();"),
            "the build script is the plain-cargo link path"
        );
        let manifest = NapiGenerator::cargo_toml(NAPI_PKG, CORE_PKG);
        let doc: toml::Value = toml::from_str(&manifest).unwrap();
        assert!(
            doc["build-dependencies"]["napi-build"].as_str().is_some(),
            "{manifest}"
        );
    }
}
