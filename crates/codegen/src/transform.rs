use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Model, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub struct TransformCrate {
    pub crate_name: String,
    pub cargo_toml: String,
    pub sources: Vec<(String, String)>,
}

pub struct VersionSchema<'a> {
    pub version: u32,
    pub schema: &'a Schema,
}

pub struct TransformPlan<'a> {
    pub versions: Vec<VersionSchema<'a>>,
    pub hops: Vec<HopPlan>,
}

pub struct HopPlan {
    pub from_version: u32,
    pub to_version: u32,
    pub migration_id: String,
    pub model_ops: Vec<ModelOp>,
    pub authored_src: Option<String>,
    pub escape: Option<EscapeBridge>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EscapeBridge {
    pub program: String,
    pub args: Vec<String>,
}

pub struct ModelOp {
    pub model: String,
    pub source_model: String,
    pub field_renames: Vec<(String, String)>,
    pub field_removes: Vec<String>,
    pub field_adds: Vec<(String, String)>,
    pub field_copies: Vec<(String, String)>,
    pub field_null_fills: Vec<(String, String)>,
}

fn is_transactable(model: &Model) -> bool {
    model.has_identity()
}

pub struct TransformGenerator;

impl TransformGenerator {
    pub fn generate(plan: &TransformPlan, crate_name: &str) -> Result<TransformCrate> {
        use crate::CodegenError;

        if plan.hops.is_empty() {
            return Err(CodegenError::GenerationFailed(
                "transformer range is empty (from == to): nothing to migrate".into(),
            ));
        }
        if plan.versions.len() != plan.hops.len() + 1 {
            return Err(CodegenError::GenerationFailed(format!(
                "transformer plan is inconsistent: {} version schemas for {} hops \
                 (expected {} schemas)",
                plan.versions.len(),
                plan.hops.len(),
                plan.hops.len() + 1
            )));
        }
        for (i, hop) in plan.hops.iter().enumerate() {
            if plan.versions[i].version != hop.from_version
                || plan.versions[i + 1].version != hop.to_version
                || hop.to_version != hop.from_version + 1
            {
                return Err(CodegenError::GenerationFailed(format!(
                    "transformer hop {} (v{}→v{}) does not match the contiguous \
                     serial version sequence (C1 — never a synthesized jump)",
                    i, hop.from_version, hop.to_version
                )));
            }
        }

        let mut sources = Vec::new();

        for vs in &plan.versions {
            let code = RustGenerator::generate_with_schema_version(vs.schema, vs.version)?.code;
            sources.push((format!("src/v{}.rs", vs.version), code));
        }

        for hop in &plan.hops {
            if let Some(src) = &hop.authored_src {
                sources.push((format!("src/{}.rs", authored_mod_name(hop)), src.clone()));
            }
        }

        let main = Self::generate_main(plan)?;
        sources.push(("src/main.rs".to_string(), main));

        Ok(TransformCrate {
            crate_name: crate_name.to_string(),
            cargo_toml: Self::cargo_toml(crate_name),
            sources,
        })
    }

    fn generate_main(plan: &TransformPlan) -> Result<String> {
        let from = plan.versions.first().unwrap().version;
        let to = plan.versions.last().unwrap().version;

        let version_mods: Vec<TokenStream> = plan
            .versions
            .iter()
            .map(|vs| {
                let m = format_ident!("v{}", vs.version);
                quote! { mod #m; }
            })
            .collect();
        let authored_mods: Vec<TokenStream> = plan
            .hops
            .iter()
            .filter(|h| h.authored_src.is_some())
            .map(|h| {
                let m = format_ident!("{}", authored_mod_name(h));
                quote! { mod #m; }
            })
            .collect();

        let hop_fns: Vec<TokenStream> = plan
            .hops
            .iter()
            .enumerate()
            .map(|(i, hop)| {
                Self::generate_hop_fn(hop, plan.versions[i].schema, plan.versions[i + 1].schema)
            })
            .collect();

        let escape_support = plan
            .hops
            .iter()
            .any(|h| h.escape.is_some())
            .then(escape_support)
            .unwrap_or_default();

        let run = Self::generate_run(plan);

        let usage = format!(
            "ForgeDB migration transformer (format v{from} -> v{to})\n\
             usage: <this-bin> <src-data-dir> <dest-data-dir>"
        );

        let tokens = quote! {
            #![allow(warnings)]

            #(#version_mods)*
            #(#authored_mods)*

            #escape_support

            #(#hop_fns)*

            #run

            fn main() {
                let __args: Vec<String> = std::env::args().collect();
                if __args.len() != 3 {
                    eprintln!(#usage);
                    std::process::exit(2);
                }
                let __src = std::path::PathBuf::from(&__args[1]);
                let __dst = std::path::PathBuf::from(&__args[2]);
                match run(&__src, &__dst) {
                    Ok(()) => {
                        println!(
                            "\u{2713} migrated {:?} -> {:?} (format v{} -> v{})",
                            __src, __dst, #from, #to
                        );
                    }
                    Err(__e) => {
                        eprintln!("migration failed: {}", __e);
                        std::process::exit(1);
                    }
                }
            }
        };

        let file = syn::parse2::<syn::File>(tokens).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!("transformer main.rs parse: {e}"))
        })?;
        Ok(prettyplease::unparse(&file))
    }

    fn generate_hop_fn(hop: &HopPlan, schema_from: &Schema, schema_to: &Schema) -> TokenStream {
        let vfrom = format_ident!("v{}", hop.from_version);
        let vto = format_ident!("v{}", hop.to_version);
        let fn_name = format_ident!("transform_v{}_to_v{}", hop.from_version, hop.to_version);
        let to_v = hop.to_version;
        let authored = hop
            .authored_src
            .as_ref()
            .map(|_| format_ident!("{}", authored_mod_name(hop)));

        let escape_spawn = hop.escape.as_ref().map(|b| {
            let program = &b.program;
            let args = &b.args;
            quote! {
                let mut __escape = __Escape::spawn(#program, &[#(#args),*])?;
            }
        });
        let escape_finish = hop.escape.as_ref().map(|_| {
            quote! { __escape.finish()?; }
        });

        let mut model_loops = Vec::new();
        for model in &schema_to.models {
            if !is_transactable(model) {
                continue;
            }
            let op = hop.model_ops.iter().find(|o| o.model == model.name);
            let source_name = op.map(|o| o.source_model.as_str()).unwrap_or(model.name.as_str());
            let Some(src_model) = schema_from.models.iter().find(|m| m.name == source_name) else {
                continue;
            };
            if !is_transactable(src_model) {
                continue;
            }

            let src_field = format_ident!("{}", RustGenerator::to_snake_case(source_name));
            let dst_field = format_ident!("{}", RustGenerator::to_snake_case(&model.name));
            let dst_ty = format_ident!("{}", model.name);
            let model_str = model.name.clone();

            let mut ops = Vec::new();
            if let Some(op) = op {
                for (old, new) in &op.field_renames {
                    ops.push(quote! {
                        if let Some(__obj) = __j.as_object_mut() {
                            if let Some(__v) = __obj.remove(#old) {
                                __obj.insert(#new.to_string(), __v);
                            }
                        }
                    });
                }
                for (from, to) in &op.field_copies {
                    ops.push(quote! {
                        if let Some(__obj) = __j.as_object_mut() {
                            if let Some(__v) = __obj.get(#from).cloned() {
                                __obj.insert(#to.to_string(), __v);
                            }
                        }
                    });
                }
                for f in &op.field_removes {
                    ops.push(quote! {
                        if let Some(__obj) = __j.as_object_mut() { __obj.remove(#f); }
                    });
                }
                for (name, json) in &op.field_null_fills {
                    ops.push(quote! {
                        if let Some(__obj) = __j.as_object_mut() {
                            if __obj.get(#name).map(|v| v.is_null()).unwrap_or(true) {
                                __obj.insert(
                                    #name.to_string(),
                                    serde_json::from_str(#json).unwrap(),
                                );
                            }
                        }
                    });
                }
                for (name, json) in &op.field_adds {
                    ops.push(quote! {
                        if let Some(__obj) = __j.as_object_mut() {
                            __obj.insert(
                                #name.to_string(),
                                serde_json::from_str(#json).unwrap(),
                            );
                        }
                    });
                }
            }

            let authored_call = authored.as_ref().map(|am| {
                quote! { __j = #am::authored_transform(#model_str, __j); }
            });
            let escape_call = hop.escape.as_ref().map(|_| {
                quote! { __j = __escape.row(#model_str, __j)?; }
            });

            model_loops.push(quote! {
                for __row in __src.#src_field.all() {
                    let mut __j = serde_json::to_value(&__row)
                        .map_err(|e| format!("serialize {}: {}", #model_str, e))?;
                    #(#ops)*
                    #authored_call
                    #escape_call
                    let __rec: #vto::#dst_ty = serde_json::from_value(__j)
                        .map_err(|e| format!("decode {} at v{}: {}", #model_str, #to_v, e))?;
                    __dst.#dst_field.insert(__rec)
                        .map_err(|e| format!("insert {}: {:?}", #model_str, e))?;
                }
            });
        }

        let from_j = RustGenerator::valid_m2m(schema_from);
        let to_j = RustGenerator::valid_m2m(schema_to);
        let mut junction_loops = Vec::new();
        for m in &to_j {
            let jf = RustGenerator::junction_field_ident(m);
            let in_from = from_j
                .iter()
                .any(|fm| RustGenerator::junction_field_ident(fm) == jf);
            if in_from {
                junction_loops.push(quote! {
                    for (__l, __r) in __src.#jf.pairs() { __dst.#jf.link(__l, __r); }
                });
            }
        }

        quote! {
            fn #fn_name(
                __src_dir: &std::path::Path,
                __dst_dir: &std::path::Path,
            ) -> ::std::result::Result<(), String> {
                std::fs::create_dir_all(__dst_dir).map_err(|e| format!("mkdir dst: {}", e))?;
                #escape_spawn
                let __src = #vfrom::Database::open_at(__src_dir.to_path_buf());
                let mut __dst = #vto::Database::open_at(__dst_dir.to_path_buf());
                #(#model_loops)*
                #(#junction_loops)*
                #escape_finish
                __dst.commit().map_err(|e| format!("commit v{}: {}", #to_v, e))?;
                Ok(())
            }
        }
    }

    fn generate_run(plan: &TransformPlan) -> TokenStream {
        let n = plan.hops.len();
        let mut stmts = Vec::new();
        for (i, hop) in plan.hops.iter().enumerate() {
            let fname = format_ident!("transform_v{}_to_v{}", hop.from_version, hop.to_version);
            let this = format_ident!("__step_{}", i + 1);
            let to_v = hop.to_version;
            let src_expr = if i == 0 {
                quote! { __src }
            } else {
                let prev = format_ident!("__step_{}", i);
                quote! { &#prev }
            };
            stmts.push(quote! {
                let #this = __work.join(format!("v{}", #to_v));
                #fname(#src_expr, &#this)?;
            });
        }
        let last = format_ident!("__step_{}", n);

        quote! {
            fn run(
                __src: &std::path::Path,
                __final_dst: &std::path::Path,
            ) -> ::std::result::Result<(), String> {
                if __final_dst.exists() {
                    let __nonempty = std::fs::read_dir(__final_dst)
                        .map(|mut d| d.next().is_some())
                        .unwrap_or(false);
                    if __nonempty {
                        return Err(format!(
                            "destination {:?} exists and is non-empty; refusing to \
                             overwrite (the retained source dir is your rollback)",
                            __final_dst
                        ));
                    }
                }
                let __parent = __final_dst
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let __work = __parent.join(".forgedb-transform-work");
                let _ = std::fs::remove_dir_all(&__work);
                std::fs::create_dir_all(&__work).map_err(|e| format!("mkdir work: {}", e))?;

                #(#stmts)*

                if let Some(__p) = __final_dst.parent() {
                    if !__p.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(__p);
                    }
                }
                std::fs::rename(&#last, __final_dst)
                    .map_err(|e| format!("atomic publish rename failed: {}", e))?;
                let _ = std::fs::remove_dir_all(&__work);
                Ok(())
            }
        }
    }

    pub fn cargo_toml(crate_name: &str) -> String {
        format!(
            r#"[package]
name = "{crate_name}"
version = "0.0.0"
edition = "2024"

# The offline ForgeDB migration transformer (#74). Compiled once for a fixed
# origin->destination version range, run against data-at-rest with the app stopped.
[[bin]]
name = "{crate_name}"
path = "src/main.rs"

{CLASS_C_SUBSTRATE_DEPS}"#,
            CLASS_C_SUBSTRATE_DEPS = CLASS_C_SUBSTRATE_DEPS,
        )
    }
}

pub(crate) const CLASS_C_SUBSTRATE_DEPS: &str = r#"[dependencies]
forgedb-storage = "0.3"
forgedb-types = "0.3"
forgedb-changefeed = "0.2"
forgedb-wal = "0.2"
forgedb-compaction = "0.1"
forgedb-txn = "0.1"
forgedb-coordinator = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
rust_decimal = { version = "1", features = ["serde-with-str"] }
utoipa = { version = "5", features = ["uuid"] }
regex = "1"
"#;

fn authored_mod_name(hop: &HopPlan) -> String {
    format!("authored_{}", hop.migration_id)
}

impl TransformGenerator {
    pub fn generate_main_code(plan: &TransformPlan) -> Result<GeneratedCode> {
        let code = Self::generate_main(plan)?;
        Ok(GeneratedCode {
            code,
            description: "ForgeDB migration transformer entrypoint".to_string(),
        })
    }
}

fn escape_support() -> TokenStream {
    quote! {
        struct __Escape {
            child: std::process::Child,
            stdin: Option<std::process::ChildStdin>,
            stdout: std::io::BufReader<std::process::ChildStdout>,
            stderr: Option<std::thread::JoinHandle<String>>,
            rows: u64,
            program: String,
        }

        impl __Escape {
            fn spawn(program: &str, args: &[&str]) -> ::std::result::Result<Self, String> {
                use std::io::Read;
                let mut child = std::process::Command::new(program)
                    .args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!(
                        "could not start the transform runtime `{}`: {}", program, e
                    ))?;
                let stdin = Some(child.stdin.take().expect("stdin was piped"));
                let stdout = std::io::BufReader::new(
                    child.stdout.take().expect("stdout was piped")
                );
                let mut err = child.stderr.take().expect("stderr was piped");
                let stderr = std::thread::spawn(move || {
                    let mut s = String::new();
                    let _ = err.read_to_string(&mut s);
                    s
                });
                Ok(Self {
                    child,
                    stdin,
                    stdout,
                    stderr: Some(stderr),
                    rows: 0,
                    program: program.to_string(),
                })
            }

            fn row(
                &mut self,
                model: &str,
                row: serde_json::Value,
            ) -> ::std::result::Result<serde_json::Value, String> {
                use std::io::{BufRead, Write};
                let req = serde_json::json!({ "model": model, "row": row });
                let line = serde_json::to_string(&req)
                    .map_err(|e| format!("serialize {} for the transform runtime: {}", model, e))?;
                let __wrote = match self.stdin.as_mut() {
                    None => Err(std::io::Error::other("the transform runtime is closed")),
                    Some(__in) => __in
                        .write_all(line.as_bytes())
                        .and_then(|_| __in.write_all(b"\n"))
                        .and_then(|_| __in.flush()),
                };
                if let Err(e) = __wrote {
                    return Err(self.died(&format!("writing a {} row: {}", model, e)));
                }

                let mut reply = String::new();
                match self.stdout.read_line(&mut reply) {
                    Err(e) => Err(self.died(&format!("reading the reply for {}: {}", model, e))),
                    Ok(0) => Err(self.died(&format!(
                        "it exited after {} row(s), before replying about {}",
                        self.rows, model
                    ))),
                    Ok(_) => {
                        self.rows += 1;
                        serde_json::from_str(reply.trim()).map_err(|e| {
                            format!(
                                "the transform runtime replied with something that is not a \
                                 JSON row for {}: {}\n  reply: {}",
                                model, e, reply.trim()
                            )
                        })
                    }
                }
            }

            fn died(&mut self, what: &str) -> String {
                let tail = self.drain_stderr();
                format!(
                    "the transform runtime `{}` failed while {}.{}",
                    self.program, what, tail
                )
            }

            fn drain_stderr(&mut self) -> String {
                match self.stderr.take().and_then(|h| h.join().ok()) {
                    Some(s) if !s.trim().is_empty() => format!("\n--- its output ---\n{}", s.trim()),
                    _ => String::new(),
                }
            }

            fn finish(mut self) -> ::std::result::Result<(), String> {
                drop(self.stdin.take());
                let status = self
                    .child
                    .wait()
                    .map_err(|e| format!("waiting for the transform runtime: {}", e))?;
                if status.success() {
                    return Ok(());
                }
                let tail = self.drain_stderr();
                Err(format!(
                    "the transform runtime `{}` exited {} after {} row(s).{}",
                    self.program, status, self.rows, tail
                ))
            }
        }
    }
}
