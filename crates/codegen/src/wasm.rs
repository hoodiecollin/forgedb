use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, Model, RelationType, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};

pub struct WasmGenerator;

#[derive(Clone, Copy)]
enum ReadRet {
    Count,
    Optional,
    Array,
}

struct ReadMethod {
    js_name: String,
    takes_id: bool,
    ret: ReadRet,
    rust: TokenStream,
}

impl WasmGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let surface = Self::read_surface(schema);
        let reads: Vec<&TokenStream> = surface.iter().map(|m| &m.rust).collect();

        let inline_str_import = if RustGenerator::needs_inline_str(schema) {
            quote! { use forgedb_core::forgedb_types::InlineStr; }
        } else {
            quote! {}
        };
        let tokens = quote! {
            #![allow(warnings)]

            use forgedb_core as database;

            use std::cell::Cell;
            use std::cell::RefCell;
            use std::path::PathBuf;

            use forgedb_core::forgedb_changefeed::durable::PersistedEvent;
            use forgedb_core::forgedb_storage::persist::{self, Backend};
            use forgedb_core::forgedb_storage::store;
            #inline_str_import
            use forgedb_core::forgedb_types::Uuid;
            use wasm_bindgen::prelude::*;

            use database::Database;

            fn watermark_key(db_name: &str) -> PathBuf {
                PathBuf::from(format!("{db_name}/_watermark"))
            }

            #[wasm_bindgen]
            pub struct Replica {
                db: RefCell<Database>,
                backend: Backend,
                db_name: String,
                watermark: Cell<u64>,
            }

            #[wasm_bindgen]
            impl Replica {
                pub async fn open(db_name: String, backend: String) -> Result<Replica, JsValue> {
                    let backend = Backend::from_str_lossy(&backend);
                    store::clear();
                    persist::hydrate(backend, &db_name)
                        .await
                        .map_err(|e| JsValue::from_str(&format!("hydrate failed: {e:?}")))?;

                    let root = PathBuf::from(&db_name);
                    let db = Database::open_at(root);

                    let watermark = store::get(&watermark_key(&db_name))
                        .filter(|b| b.len() == 8)
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                        .unwrap_or(0);

                    Ok(Replica {
                        db: RefCell::new(db),
                        backend,
                        db_name,
                        watermark: Cell::new(watermark),
                    })
                }

                #[wasm_bindgen(js_name = applyWire)]
                pub fn apply_wire(&self, bytes: &[u8]) -> Result<(), JsValue> {
                    let ev = PersistedEvent::from_wire(bytes)
                        .map_err(|e| JsValue::from_str(&format!("bad frame: {e}")))?;
                    let offset = ev.offset;
                    self.db
                        .borrow_mut()
                        .apply_frame(&ev)
                        .map_err(|e| JsValue::from_str(&format!("apply failed: {e}")))?;
                    if offset > self.watermark.get() {
                        self.watermark.set(offset);
                    }
                    Ok(())
                }

                pub async fn commit(&self) -> Result<(), JsValue> {
                    {
                        self.db
                            .borrow_mut()
                            .commit()
                            .map_err(|e| JsValue::from_str(&format!("commit failed: {e}")))?;
                    }
                    store::put(
                        watermark_key(&self.db_name),
                        self.watermark.get().to_le_bytes().to_vec(),
                    );
                    persist::commit(self.backend, &self.db_name)
                        .await
                        .map_err(|e| JsValue::from_str(&format!("persist failed: {e:?}")))?;
                    Ok(())
                }

                pub fn watermark(&self) -> u64 {
                    self.watermark.get()
                }

                #(#reads)*
            }
        };

        let syntax_tree = syn::parse_file(&tokens.to_string()).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!(
                "Failed to parse generated wasm transport: {e}"
            ))
        })?;
        let code = prettyplease::unparse(&syntax_tree);

        Ok(GeneratedCode {
            code,
            description: format!(
                "wasm-bindgen browser read-replica transport ({} models)",
                schema.models.len()
            ),
        })
    }

    fn read_surface(schema: &Schema) -> Vec<ReadMethod> {
        let mut methods: Vec<ReadMethod> = Vec::new();
        let mut seen_rust: HashSet<String> = HashSet::new();
        let mut seen_js: HashSet<String> = HashSet::new();

        let mut push = |methods: &mut Vec<ReadMethod>,
                        rust_name: &str,
                        js_name: &str,
                        takes_id: bool,
                        ret: ReadRet,
                        tokens: TokenStream| {
            if !seen_rust.insert(rust_name.to_string()) || !seen_js.insert(js_name.to_string()) {
                return;
            }
            methods.push(ReadMethod {
                js_name: js_name.to_string(),
                takes_id,
                ret,
                rust: tokens,
            });
        };

        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_field = format_ident!("{model_snake}");

            let count_rust = format!("{model_snake}_count");
            let count_js = format!("{}Count", lower_camel_from_pascal(&model.name));
            let count_ident = format_ident!("{count_rust}");
            push(
                &mut methods,
                &count_rust,
                &count_js,
                false,
                ReadRet::Count,
                quote! {
                    #[wasm_bindgen(js_name = #count_js)]
                    pub fn #count_ident(&self) -> u32 {
                        self.db.borrow().#model_field.all().len() as u32
                    }
                },
            );

            let all_rust = format!("all_{model_snake}");
            let all_js = format!("all{}s", model.name);
            let all_ident = format_ident!("{all_rust}");
            push(
                &mut methods,
                &all_rust,
                &all_js,
                false,
                ReadRet::Array,
                quote! {
                    #[wasm_bindgen(js_name = #all_js)]
                    pub fn #all_ident(&self) -> String {
                        serde_json::to_string(&self.db.borrow().#model_field.all())
                            .unwrap_or_else(|_| "[]".to_string())
                    }
                },
            );

            if model.identity_field().is_some() {
                let get_rust = format!("get_{model_snake}");
                let get_js = format!("get{}", model.name);
                let get_ident = format_ident!("{get_rust}");
                let pk_parse = pk_parse_opt(schema, model);
                push(
                    &mut methods,
                    &get_rust,
                    &get_js,
                    true,
                    ReadRet::Optional,
                    quote! {
                        #[wasm_bindgen(js_name = #get_js)]
                        pub fn #get_ident(&self, id: String) -> Option<String> {
                            let __pk = #pk_parse?;
                            let __rec = self.db.borrow().#model_field.get(__pk)?;
                            serde_json::to_string(&__rec).ok()
                        }
                    },
                );
            }
        }

        for model in &schema.models {
            let Some(_) = model.identity_field() else {
                continue;
            };
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_field = format_ident!("{model_snake}");
            let pk_parse = pk_parse_opt(schema, model);
            for field in &model.fields {
                let target_name = match &field.field_type {
                    FieldType::Relation(RelationType::RequiredReference(t))
                    | FieldType::Relation(RelationType::OptionalReference(t)) => t,
                    _ => continue,
                };
                if schema.find_model(target_name).is_none() {
                    continue;
                }
                let method_name = format!("{model_snake}_{}", field.name);
                let method_ident = format_ident!("{method_name}");
                let js = lower_camel_from_snake(&method_name);
                push(
                    &mut methods,
                    &method_name,
                    &js,
                    true,
                    ReadRet::Optional,
                    quote! {
                        #[wasm_bindgen(js_name = #js)]
                        pub fn #method_ident(&self, id: String) -> Option<String> {
                            let __pk = #pk_parse?;
                            let __db = self.db.borrow();
                            let __rec = __db.#model_field.get(__pk)?;
                            let __res = __db.#method_ident(&__rec)?;
                            serde_json::to_string(&__res).ok()
                        }
                    },
                );
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
            let parent_pk_parse = pk_parse_opt(schema, parent);
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
            let method_ident = format_ident!("{method_name}");
            let js = lower_camel_from_snake(&method_name);
            push(
                &mut methods,
                &method_name,
                &js,
                true,
                ReadRet::Array,
                quote! {
                    #[wasm_bindgen(js_name = #js)]
                    pub fn #method_ident(&self, id: String) -> String {
                        let Some(__pk) = #parent_pk_parse else {
                            return "[]".to_string();
                        };
                        serde_json::to_string(&self.db.borrow().#method_ident(__pk))
                            .unwrap_or_else(|_| "[]".to_string())
                    }
                },
            );
        }

        for m in RustGenerator::valid_m2m(schema) {
            for (model_name, field) in
                [(&m.model1, &m.field1), (&m.model2, &m.field2)]
            {
                let Some(endpoint) = schema.find_model(model_name) else {
                    continue;
                };
                let pk_parse = pk_parse_opt(schema, endpoint);
                let method_name =
                    format!("{}_{}", RustGenerator::to_snake_case(model_name), field);
                let method_ident = format_ident!("{method_name}");
                let js = lower_camel_from_snake(&method_name);
                push(
                    &mut methods,
                    &method_name,
                    &js,
                    true,
                    ReadRet::Array,
                    quote! {
                        #[wasm_bindgen(js_name = #js)]
                        pub fn #method_ident(&self, id: String) -> String {
                            let Some(__pk) = #pk_parse else {
                                return "[]".to_string();
                            };
                            serde_json::to_string(&self.db.borrow().#method_ident(__pk))
                                .unwrap_or_else(|_| "[]".to_string())
                        }
                    },
                );
            }
        }

        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_field = format_ident!("{model_snake}");
            for proj in &model.projections {
                let pascal = RustGenerator::projection_pascal(&proj.name);

                let all_rust = format!("all_{model_snake}_{}", proj.name);
                let all_js = format!("all{}{}", model.name, pascal);
                let all_ident = format_ident!("{all_rust}");
                let proj_all = format_ident!("all_{}", proj.name);
                push(
                    &mut methods,
                    &all_rust,
                    &all_js,
                    false,
                    ReadRet::Array,
                    quote! {
                        #[wasm_bindgen(js_name = #all_js)]
                        pub fn #all_ident(&self) -> String {
                            serde_json::to_string(&self.db.borrow().#model_field.#proj_all())
                                .unwrap_or_else(|_| "[]".to_string())
                        }
                    },
                );

                if model.identity_field().is_some() {
                    let get_rust = format!("get_{model_snake}_{}", proj.name);
                    let get_js = format!("get{}{}", model.name, pascal);
                    let get_ident = format_ident!("{get_rust}");
                    let proj_get = format_ident!("get_{}", proj.name);
                    let pk_parse = pk_parse_opt(schema, model);
                    push(
                        &mut methods,
                        &get_rust,
                        &get_js,
                        true,
                        ReadRet::Optional,
                        quote! {
                            #[wasm_bindgen(js_name = #get_js)]
                            pub fn #get_ident(&self, id: String) -> Option<String> {
                                let __pk = #pk_parse?;
                                let __rec = self.db.borrow().#model_field.#proj_get(__pk)?;
                                serde_json::to_string(&__rec).ok()
                            }
                        },
                    );
                }
            }
        }

        methods
    }

    pub fn generate_client(schema: &Schema) -> Result<GeneratedCode> {
        let surface = Self::read_surface(schema);

        let mut methods = String::new();
        for m in &surface {
            let (sig_args, call_args) = if m.takes_id {
                ("id: string", "[id]")
            } else {
                ("", "[]")
            };
            let (ret_ty, body) = match m.ret {
                ReadRet::Count => (
                    "number".to_string(),
                    format!("return (await this.#call('{}', {})) as number;", m.js_name, call_args),
                ),
                ReadRet::Optional => (
                    "Record<string, unknown> | null".to_string(),
                    format!(
                        "const r = await this.#call('{}', {}); return r == null ? null : JSON.parse(r as string);",
                        m.js_name, call_args
                    ),
                ),
                ReadRet::Array => (
                    "Record<string, unknown>[]".to_string(),
                    format!(
                        "const r = await this.#call('{}', {}); return JSON.parse((r as string) ?? '[]');",
                        m.js_name, call_args
                    ),
                ),
            };
            methods.push_str(&format!(
                "\n  async {name}({sig}): Promise<{ret}> {{\n    {body}\n  }}\n",
                name = m.js_name,
                sig = sig_args,
                ret = ret_ty,
                body = body,
            ));
        }

        let code = format!(
            r#"// Generated by ForgeDB — browser read-replica async client.
// DO NOT EDIT - This file is auto-generated.

export type ReplicaBackend = "indexeddb" | "opfs" | "auto";

export interface ReplicaOpenOptions {{
  backend?: ReplicaBackend;
  replicateUrl?: string;
}}

interface Pending {{
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}}

export class ReplicaClient {{
  #worker: Worker;
  #seq = 0;
  #pending = new Map<number, Pending>();

  constructor(workerUrl: string | URL, private wasmUrl: string) {{
    this.#worker = new Worker(workerUrl, {{ type: "module" }});
    this.#worker.onmessage = (e: MessageEvent) => {{
      const {{ id, ok, result, error }} = e.data ?? {{}};
      const p = this.#pending.get(id);
      if (!p) return;
      this.#pending.delete(id);
      if (ok) p.resolve(result);
      else p.reject(new Error(String(error)));
    }};
  }}

  #call(method: string, args: unknown[]): Promise<unknown> {{
    const id = ++this.#seq;
    return new Promise((resolve, reject) => {{
      this.#pending.set(id, {{ resolve, reject }});
      this.#worker.postMessage({{ id, method, args }});
    }});
  }}

  async init(): Promise<void> {{
    await this.#call("__init", [this.wasmUrl]);
  }}

  async open(dbName: string, opts: ReplicaOpenOptions = {{}}): Promise<void> {{
    await this.#call("__open", [
      dbName,
      opts.backend ?? "auto",
      opts.replicateUrl ?? null,
    ]);
  }}

  async applyWire(frame: Uint8Array): Promise<void> {{
    await this.#call("__applyWire", [frame]);
  }}

  async commit(): Promise<void> {{
    await this.#call("commit", []);
  }}

  async watermark(): Promise<number> {{
    return Number(await this.#call("watermark", []));
  }}

  close(): void {{
    this.#worker.terminate();
  }}
{methods}}}
"#,
            methods = methods
        );

        Ok(GeneratedCode {
            code,
            description: format!(
                "async ReplicaClient (main-thread, {} read methods)",
                surface.len()
            ),
        })
    }

    pub fn worker_bootstrap() -> &'static str {
        WORKER_BOOTSTRAP_JS
    }

    pub fn worker_bootstrap_with_config(config: crate::GenConfig) -> String {
        WORKER_BOOTSTRAP_JS
            .replace(
                "const COMMIT_DEBOUNCE_MS = 250;",
                &format!("const COMMIT_DEBOUNCE_MS = {};", config.wasm_commit_debounce_ms),
            )
            .replace(
                "const COMMIT_MAX_FRAMES = 100;",
                &format!("const COMMIT_MAX_FRAMES = {};", config.wasm_commit_max_frames),
            )
    }

    pub fn cargo_toml(crate_name: &str, core_pkg: &str) -> String {
        format!(
            r#"# Generated by ForgeDB. Do not edit — rewritten in full on every generate.
[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"

# Compiled to WebAssembly with `wasm-pack build --target web`.
[lib]
crate-type = ["cdylib"]

[dependencies]
# The app's ONE generated database, linked as a crate. Every substrate type this
# transport names (`persist`, `store`, `PersistedEvent`, `Uuid`, `InlineStr`)
# comes through its re-exports, so this package pins NO substrate.
forgedb_core = {{ package = "{core_pkg}", path = "../core" }}
serde_json = "1"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
"#
        )
    }
}

const WORKER_BOOTSTRAP_JS: &str = r#"// Generated by ForgeDB — browser read-replica Worker bootstrap (STATIC).
// DO NOT EDIT - schema-agnostic. Runs the wasm engine, follows /replicate,

let mod = null;      // the wasm-pack ES module (booted on __init)
let replica = null;  // the generated Replica instance (opened on __open)
let ws = null;

let pendingFrames = 0;
let commitTimer = null;
let committing = false;
const COMMIT_DEBOUNCE_MS = 250;
const COMMIT_MAX_FRAMES = 100;

function scheduleCommit() {
  pendingFrames++;
  if (pendingFrames >= COMMIT_MAX_FRAMES) { void doCommit(); return; }
  if (commitTimer) clearTimeout(commitTimer);
  commitTimer = setTimeout(() => { void doCommit(); }, COMMIT_DEBOUNCE_MS);
}

async function doCommit() {
  if (commitTimer) { clearTimeout(commitTimer); commitTimer = null; }
  if (committing || !replica) return;
  committing = true;
  pendingFrames = 0;
  try {
    await replica.commit();
  } catch (err) {
    console.error('[forgedb replica] commit failed', err);
  } finally {
    committing = false;
  }
}

async function probeBackend() {
  try {
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle('__forgedb_probe', { create: true });
    const fh = await dir.getFileHandle('p', { create: true });
    const ah = await fh.createSyncAccessHandle();
    const ok = typeof ah.getSize() === 'number'; // sync semantics -> a number
    ah.close();
    try { await dir.removeEntry('p'); } catch (_) {}
    try { await root.removeEntry('__forgedb_probe', { recursive: true }); } catch (_) {}
    return ok ? 'opfs' : 'indexeddb';
  } catch (_) {
    return 'indexeddb';
  }
}

function connect(url) {
  const sep = url.includes('?') ? '&' : '?';
  ws = new WebSocket(`${url}${sep}after=${replica.watermark()}`);
  ws.binaryType = 'arraybuffer';
  ws.onmessage = (e) => {
    if (typeof e.data === 'string') return;
    try {
      replica.applyWire(new Uint8Array(e.data));
      scheduleCommit();
    } catch (err) {
      console.error('[forgedb replica] apply failed', err);
    }
  };
  ws.onerror = (err) => console.error('[forgedb replica] ws error', err);
}

self.onmessage = async (e) => {
  const { id, method, args } = e.data || {};
  try {
    let result;
    if (method === '__init') {
      mod = await import(args[0]);
      await mod.default();
    } else if (method === '__open') {
      let [dbName, backend, replicateUrl] = args;
      if (backend === 'auto') backend = await probeBackend();
      replica = await mod.Replica.open(dbName, backend);
      if (replicateUrl) connect(replicateUrl);
      result = backend;
    } else if (method === '__applyWire') {
      replica.applyWire(new Uint8Array(args[0]));
      scheduleCommit();
    } else if (method === 'commit') {
      await replica.commit();
    } else if (method === 'watermark') {
      result = replica.watermark();
    } else {
      result = replica[method](...args);
    }
    self.postMessage({ id, ok: true, result });
  } catch (err) {
    self.postMessage({ id, ok: false, error: String((err && err.stack) || err) });
  }
};
"#;

fn pk_parse_opt(schema: &Schema, model: &Model) -> TokenStream {
    match RustGenerator::identity_type(schema, model).as_ref() {
        Some(FieldType::U32) => quote! { id.parse::<u32>().ok() },
        Some(FieldType::U64) => quote! { id.parse::<u64>().ok() },
        Some(FieldType::I32) => quote! { id.parse::<i32>().ok() },
        Some(FieldType::I64) => quote! { id.parse::<i64>().ok() },
        Some(ty @ FieldType::StringN { .. }) => {
            let key_ty = RustGenerator::key_type_ident(schema, ty);
            quote! { <#key_ty>::try_from(id.as_str()).ok() }
        }
        Some(FieldType::String) => quote! { Some(id) },
        _ => quote! { Uuid::parse_str(&id).ok() },
    }
}

fn lower_camel_from_pascal(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lower_camel_from_snake(snake: &str) -> String {
    let mut out = String::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn replica_for(src: &str) -> String {
        let mut parser = forgedb_parser::Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        WasmGenerator::generate(&schema).unwrap().code
    }

    const SRC: &str = "User {\n  id: +uuid\n  email: string\n}\n";

    #[test]
    fn the_seam_links_core_instead_of_declaring_a_database_module() {
        let code = replica_for(SRC);
        let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

        assert!(
            flat.contains("useforgedb_coreasdatabase;"),
            "the replica must alias the `core` crate as `database`:\n{code:.600}"
        );
        assert!(
            !flat.contains("moddatabase;"),
            "a `mod database;` seam means a SECOND, separately-generated database:\n{code:.600}"
        );
        assert!(
            flat.contains("usedatabase::Database;"),
            "the transport still reaches the generated `Database`:\n{code:.600}"
        );
    }

    #[test]
    fn no_substrate_is_named_outside_core() {
        let code = replica_for(SRC);
        let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();

        for routed in [
            "useforgedb_core::forgedb_changefeed::durable::PersistedEvent;",
            "useforgedb_core::forgedb_storage::persist::",
            "useforgedb_core::forgedb_storage::store;",
            "useforgedb_core::forgedb_types::Uuid;",
        ] {
            assert!(flat.contains(routed), "missing core-routed import {routed}");
        }

        for absolute in [
            "useforgedb_changefeed::",
            "useforgedb_storage::",
            "useforgedb_types::",
        ] {
            assert!(
                !flat.contains(absolute),
                "substrate named absolutely ({absolute}) — it must go through `core`:\n{code:.600}"
            );
        }
    }

    #[test]
    fn the_inline_str_import_is_conditional_and_core_routed() {
        let with = replica_for("Doc {\n  id: string(26!)\n  title: string\n}\n");
        let without = replica_for(SRC);
        let flat = |c: &str| c.chars().filter(|c| !c.is_whitespace()).collect::<String>();

        assert!(
            flat(&with).contains("useforgedb_core::forgedb_types::InlineStr;"),
            "an inline-string schema imports InlineStr through core:\n{with:.600}"
        );
        assert!(
            !flat(&without).contains("useforgedb_core::forgedb_types::InlineStr;"),
            "a schema with no inline strings carries no InlineStr import:\n{without:.600}"
        );
    }

    #[test]
    fn the_manifest_pins_no_substrate() {
        let manifest = WasmGenerator::cargo_toml("app-wasm", "app-3f2a-core");

        assert!(
            manifest.contains(r#"forgedb_core = { package = "app-3f2a-core", path = "../core" }"#),
            "{manifest}"
        );
        for pin in [
            "forgedb-storage",
            "forgedb-types",
            "forgedb-changefeed",
            "forgedb-wal",
            "forgedb-compaction",
            "forgedb-txn",
            "forgedb-coordinator",
        ] {
            assert!(
                !manifest.contains(pin),
                "wrapper still pins substrate `{pin}`:\n{manifest}"
            );
        }
    }

    #[test]
    fn no_profile_table_is_emitted() {
        let manifest = WasmGenerator::cargo_toml("app-wasm", "app-3f2a-core");
        assert!(!manifest.contains("[profile"), "{manifest}");
        assert!(!manifest.contains("opt-level"), "{manifest}");
    }

    #[test]
    fn the_manifest_parses_as_toml() {
        let manifest = WasmGenerator::cargo_toml("app-wasm", "app-3f2a-core");
        toml::from_str::<toml::Value>(&manifest)
            .unwrap_or_else(|e| panic!("wasm manifest is not valid TOML: {e}\n{manifest}"));
    }
}
