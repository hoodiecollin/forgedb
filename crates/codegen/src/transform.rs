//! Data-transform migration generator (#74 Phase 3).
//!
//! Emits the offline **transformer bin** for a specific origin→destination
//! **version range** — the one operator artifact that migrates a data dir from an
//! old schema version to a new one. Because schema versions are serial, the
//! sequence between origin and destination is deterministic; the generated crate
//! embeds ONLY that sequence as generated typed modules (`v1`, `v2`, … — each a
//! full [`RustGenerator`] emission of that version's database) and replays a fixed,
//! straight-line chain of named hop functions over a src→dest data dir.
//!
//! ## Identity (the PM red lines this file must hold — DV-1)
//!
//! - **No schema at runtime (C1).** The embedded version set is fixed at
//!   *generation* time by the `--from`/`--to` inputs; the emitted crate parses no
//!   `.forge`, links no `forgedb-parser` and no `forgedb-migrations`. Each version's
//!   structs/readers/writers are baked-in generated code.
//! - **Straight-line replay, no interpreter (C2/C8/DV-11).** `run` is a fixed
//!   call-chain of `transform_vN_to_vM` functions — there is no `Vec<Step>`
//!   descriptor loop and no runtime mechanism-selection branch. Each hop's behavior
//!   is frozen at generation time (auto-derived structural ops for provable hops,
//!   an embedded frozen `transform.rs` for authored ones — C13).
//! - **Provider-free (C4/DV-7).** The crate links the same schema-agnostic
//!   substrate the generated app links (storage/types/changefeed/wal/txn/…) and
//!   nothing that interprets a schema.
//!
//! Each hop reads every row via the `v_from` typed structs, produces the `v_to`
//! record (JSON is the transport between the two typed structs — the field-name
//! ops are compile-time constants baked from the frozen diff, never read from a
//! schema at runtime), and writes via the `v_to` writer (`insert` preserves the
//! record's id). The per-version format guard baked into each `Database` (#74
//! Phase 1) enforces the version interlock for free: `vN::Database::open_at`
//! refuses a dir not stamped at format `vN`.

use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Model, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// The generated per-range transformer crate.
pub struct TransformCrate {
    pub crate_name: String,
    /// `Cargo.toml`.
    ///
    /// It used to be described here as "a user-editable scaffold, written ONLY
    /// when absent". It is neither, since #335 §9: both class-C packages are
    /// emitted into ForgeDB's build cache, where **nothing is the user's** and
    /// every file — the manifest included — is rewritten in full on every
    /// emission. Carried forward unchanged, a CLI upgrade that bumps a
    /// substrate pin would never reach an existing member, and the stale pin
    /// would sit in a directory the user never opens.
    pub cargo_toml: String,
    /// `(relative path from crate root, content)` for every generated source file
    /// (`src/*.rs`) — always (re)written on generate.
    pub sources: Vec<(String, String)>,
}

/// One version's schema in the range, paired with its schema serial.
pub struct VersionSchema<'a> {
    pub version: u32,
    pub schema: &'a Schema,
}

/// The plan the CLI hands the generator: the ordered contiguous version schemas in
/// the range plus one hop per adjacent pair (`versions.len() - 1` hops).
pub struct TransformPlan<'a> {
    pub versions: Vec<VersionSchema<'a>>,
    pub hops: Vec<HopPlan>,
}

/// One hop's frozen behavior (built by the CLI from the committed migration record
/// + the dest schema's field types — see `crate::commands::migrate`).
pub struct HopPlan {
    pub from_version: u32,
    pub to_version: u32,
    /// The migration's id (used to name the embedded authored module).
    pub migration_id: String,
    /// Per-changed-model structural ops the differ could PROVE (additive/rename/
    /// drop). Unchanged models are copied by the generator with no ops.
    pub model_ops: Vec<ModelOp>,
    /// The frozen authored `transform.rs` source, embedded verbatim (C13) when the
    /// hop carries `Authored` residue; `None` for a fully-automatic hop.
    pub authored_src: Option<String>,
    /// A non-Rust escape: everything needed to reach the author's OWN runtime,
    /// **all baked at generation time** (#374 direction C).
    ///
    /// Mutually exclusive with [`authored_src`](Self::authored_src): a Rust
    /// escape is compiled INTO this crate, a TypeScript or Python one runs out
    /// of process on the interpreter the author already has.
    pub escape: Option<EscapeBridge>,
}

/// How the emitted hop reaches the author's own runtime (#374).
///
/// # Where the identity temptation lives
///
/// The bridge is schema-agnostic *by construction* — it reads JSON lines and
/// writes JSON lines — which makes it look like the one piece of this design
/// that should be a shipped crate or a shared runtime helper. It is not, and the
/// violation that matters is one step further and much easier to reach by
/// accident: giving the bridge a **list of models**, or a **table of per-field
/// ops**, so one loop could drive every model. That is a descriptor loop — the
/// schema, at run time, in a generic evaluator.
///
/// So there is no model list and no op descriptor in this struct, and there is
/// none in the emitted code either: the copy loop is emitted **per model**, and
/// the model name is a **string literal at its own call site**, exactly where
/// `authored_transform(#model_str, __j)` already puts it.
#[derive(Debug, Clone, PartialEq)]
pub struct EscapeBridge {
    /// Absolute path to the interpreter, resolved from `[toolchain]` at
    /// `migrate build` and version-checked there — never looked up at run time.
    pub program: String,
    /// Baked argv. The last element is the **copy** of the author's script
    /// inside the cache member: `migrate build` verifies the script against the
    /// recorded scaffold hash and then copies it, so what runs is exactly what
    /// was checked.
    pub args: Vec<String>,
}

/// The structural row ops for one model across one hop (all applied to the row's
/// JSON before it is decoded into the `v_to` struct).
pub struct ModelOp {
    /// Destination (`v_to`) model name.
    pub model: String,
    /// Source (`v_from`) model name — equals `model` except across a `RenameModel`.
    pub source_model: String,
    /// `(old_name, new_name)` field renames.
    pub field_renames: Vec<(String, String)>,
    /// Removed field names (the key is dropped from the row).
    pub field_removes: Vec<String>,
    /// `(field_name, json_literal)` additive fields — the literal is the ONE
    /// lowering `forgedb_codegen::default_fill` produced from the destination
    /// field's `@default`, or the lowering of a recorded
    /// `Answer::Constant` (#374).
    ///
    /// A required field with **neither** contributes NO entry, on purpose: the
    /// key is then absent from the row and the destination decode fails with
    /// `missing field`, naming it. Emitting a type-zero here instead is what
    /// made an unanswered hop write `""` and exit 0.
    pub field_adds: Vec<(String, String)>,
    /// `(source_field, destination_field)` per-row copies — the lowering of a
    /// recorded `Answer::CopyField` (#374).
    ///
    /// A copy, not a constant: the value is read from **this row**, which is
    /// what makes `slug = title` mean each row's own title rather than the
    /// first one's.
    pub field_copies: Vec<(String, String)>,
    /// `(field_name, json_literal)` fills for a field narrowing to NOT NULL
    /// (#374). Written over the key **only when it is null**, because the rows
    /// that already have a value keep it.
    pub field_null_fills: Vec<(String, String)>,
}

/// Whether a model participates in the copy loop: it must be id-bearing (its rows
/// are addressed by id via `all()`/`insert`). Non-id models (pure value tables) are
/// out of scope for the transformer, exactly as they are for the mutation surface.
fn is_transactable(model: &Model) -> bool {
    model.has_identity()
}

/// Generates the offline transformer crate.
pub struct TransformGenerator;

impl TransformGenerator {
    /// Emit the transformer crate for `plan` (a contiguous version range).
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
        // The version schemas must be the contiguous serial sequence the hops walk.
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

        // One generated module per version in the range — each the full typed
        // database for that version, baked with its own `EXPECTED_SCHEMA_VERSION`
        // so its open-guard enforces the version interlock (C5/C11).
        for vs in &plan.versions {
            let code = RustGenerator::generate_with_schema_version(vs.schema, vs.version)?.code;
            sources.push((format!("src/v{}.rs", vs.version), code));
        }

        // Embed each authored hop's FROZEN transform source verbatim (C13 — the
        // generator never re-synthesizes a body it was handed).
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

    /// Build `src/main.rs`: the module decls, the frozen hop functions, the fixed
    /// straight-line `run` chain, and `main`.
    fn generate_main(plan: &TransformPlan) -> Result<String> {
        let from = plan.versions.first().unwrap().version;
        let to = plan.versions.last().unwrap().version;

        // `mod v1; mod v2; …` + `mod <authored>;`
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

        // One frozen hop fn per adjacent version pair.
        let hop_fns: Vec<TokenStream> = plan
            .hops
            .iter()
            .enumerate()
            .map(|(i, hop)| {
                Self::generate_hop_fn(hop, plan.versions[i].schema, plan.versions[i + 1].schema)
            })
            .collect();

        // The bridge's process glue, emitted only when some hop needs it.
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

    /// One hop: read every row via `v_from` structs → apply the frozen JSON ops →
    /// (authored call) → decode into the `v_to` struct → write via `v_to` writer.
    fn generate_hop_fn(hop: &HopPlan, schema_from: &Schema, schema_to: &Schema) -> TokenStream {
        let vfrom = format_ident!("v{}", hop.from_version);
        let vto = format_ident!("v{}", hop.to_version);
        let fn_name = format_ident!("transform_v{}_to_v{}", hop.from_version, hop.to_version);
        let to_v = hop.to_version;
        let authored = hop
            .authored_src
            .as_ref()
            .map(|_| format_ident!("{}", authored_mod_name(hop)));

        // The author's own runtime, spawned ONCE per hop. Not once per model and
        // not once per row: a process per row would be the same work done N
        // times, and a process per model would need a model list to drive it.
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

        // One copy loop per dest model that has a source in `v_from`.
        let mut model_loops = Vec::new();
        for model in &schema_to.models {
            if !is_transactable(model) {
                continue;
            }
            let op = hop.model_ops.iter().find(|o| o.model == model.name);
            let source_name = op.map(|o| o.source_model.as_str()).unwrap_or(model.name.as_str());
            let Some(src_model) = schema_from.models.iter().find(|m| m.name == source_name) else {
                // No source (a newly-added model): starts empty in v_to.
                continue;
            };
            if !is_transactable(src_model) {
                continue;
            }

            let src_field = format_ident!("{}", RustGenerator::to_snake_case(source_name));
            let dst_field = format_ident!("{}", RustGenerator::to_snake_case(&model.name));
            let dst_ty = format_ident!("{}", model.name);
            let model_str = model.name.clone();

            // Frozen structural JSON ops (order: rename → remove → add), all baked
            // from the diff — no schema is read at runtime.
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
                // Copies run BEFORE removes and adds: the source is named as
                // it exists in the row after any rename, and may itself be a
                // field this hop is dropping.
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
                // A narrowing to NOT NULL replaces only the nulls; a row that
                // already had a value keeps it, which a plain insert would
                // overwrite.
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
            // Same shape as the line above it, and deliberately so: the model
            // name is a LITERAL at this call site, not an entry in a table the
            // bridge walks.
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

        // Copy M2M junction pairs for junctions present in BOTH versions.
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
                // Read via the v_from typed structs. `vN::Database::open_at` runs
                // the #74 Phase 1 format guard, refusing a dir not stamped at
                // format v{from} — the version interlock, for free. Both dirs are
                // opened exclusive (#89 DirLock): migration is offline (C12).
                let __src = #vfrom::Database::open_at(__src_dir.to_path_buf());
                let mut __dst = #vto::Database::open_at(__dst_dir.to_path_buf());
                #(#model_loops)*
                #(#junction_loops)*
                #escape_finish
                // Materialize + fsync the destination-version columns.
                __dst.commit().map_err(|e| format!("commit v{}: {}", #to_v, e))?;
                Ok(())
            }
        }
    }

    /// The fixed straight-line replay chain (C2/DV-11 — no descriptor loop).
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
            /// Replay the fixed embedded version sequence over a src→dest data dir.
            /// Each hop writes an intermediate destination-version dir under a work
            /// dir; the fully-materialized final dir is atomic-renamed into place at
            /// the end (all-or-nothing at the range level — the retained source dir
            /// is the rollback). A crash mid-replay leaves either the pristine
            /// source or a cleanly-versioned intermediate the app refuses (DV-6).
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
                // Work dir a sibling of the destination so the final rename is on
                // the same filesystem (atomic).
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

    /// The provider-free `Cargo.toml` (C4/DV-7): the same substrate closure the
    /// generated app links, and NOTHING that interprets a schema (no
    /// `forgedb-parser`, no `forgedb-migrations`).
    ///
    /// # The `[[bin]]` name IS the package name
    ///
    /// It used to be the literal `forgedb-transform`, and `engine.rs` reused
    /// this manifest verbatim — so one app's own `transform/` and `engine/`
    /// declared the **same bin**. Cargo reports that as `warning: output
    /// filename collision`, **exits 0**, and leaves one file on disk, while the
    /// CLI resolved the transformer by that fixed literal: a data-corruption
    /// -class failure behind a warning (#335 §2).
    ///
    /// The package name handed in here is already app-unique and
    /// range-stamped (`crate::naming::package_name` in the CLI), and
    /// `naming::bin_name` **is** `naming::package_name` — so deriving the bin
    /// from the package makes those two names one fact instead of two facts
    /// that have to agree.
    ///
    /// # No `[workspace]`, and no `[profile]`
    ///
    /// This package is emitted as a **member of ForgeDB's own cache
    /// workspace** (#335 §1/§9). A `[package]` with no `[workspace]` table is
    /// #328 only when it lands under a *foreign* cargo root, and the resolution
    /// to that is the cache root — not a table here, which would make the
    /// member its own workspace and cut it out of the shared lockfile and
    /// `target/`.
    ///
    /// `[profile.release] opt-level = 2` is gone for a mechanical reason:
    /// cargo **ignores** a `[profile]` in a non-root member (and warns), so
    /// leaving it here would be an optimization level that reads as applied and
    /// is not. The floor is set by the build driver on the root invocation.
    ///
    /// Written **always**, never only-if-absent: nothing in the cache is the
    /// user's, and a manifest carried forward unchanged is how a bumped
    /// substrate pin fails to reach an existing member.
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

/// The substrate dependency block shared by the two **class-C** packages —
/// `transform-<from>-<to>` and `engine-<from>-<to>`.
///
/// The two manifests are emitted by two different functions on purpose (§2:
/// `engine.rs` reusing `TransformGenerator::cargo_toml` verbatim is what made
/// them declare the same `[[bin]]`), but the *dependency* half is one constant,
/// because a substrate pin bumped in one and not the other is invisible: these
/// manifests live in the build cache, in a directory the user never opens, where
/// the publish-gap check cannot see them.
///
/// It is the same schema-agnostic substrate the generated app links, and nothing
/// that interprets a schema — the identity red line: no schema at runtime, no
/// migration engine. The version modules are baked-in generated typed code.
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

/// The module name for a hop's embedded authored transform: `authored_<id>`.
fn authored_mod_name(hop: &HopPlan) -> String {
    format!("authored_{}", hop.migration_id)
}

/// Convenience wrapper mirroring the other generators' `GeneratedCode` return for
/// the single-file (`main.rs`) artifact — used by the codegen guard tests.
impl TransformGenerator {
    /// Generate just `src/main.rs` as a [`GeneratedCode`] (the identity-critical
    /// artifact the guards inspect).
    pub fn generate_main_code(plan: &TransformPlan) -> Result<GeneratedCode> {
        let code = Self::generate_main(plan)?;
        Ok(GeneratedCode {
            code,
            description: "ForgeDB migration transformer entrypoint".to_string(),
        })
    }
}

/// The process glue for a non-Rust escape (#374 direction C), emitted into the
/// transformer's `main.rs` only when some hop in the range needs it.
///
/// # It knows no schema, and cannot be given one
///
/// `row` takes a model **name** and a row, one call at a time, and the name
/// arrives as a string literal from the emitted per-model copy loop. There is no
/// model list here, no field table, and no descriptor — which is the difference
/// between generated transport glue and a generic evaluator.
///
/// # Three failure modes, all named rather than hung
///
/// * **EOF before a reply.** The runtime exited early. Reported as exactly that,
///   with the row count and the child's stderr, instead of blocking forever — a
///   buffering bug in an author's script would otherwise present as a hung
///   migration with no output.
/// * **A malformed line.** Reported with the line.
/// * **A non-zero exit.** Reported with the status and stderr.
///
/// Any of them fails the whole hop, which is already safe: the transformer
/// writes a fresh destination and leaves the source dir untouched.
///
/// Stderr is drained on its own thread. Reading it after the exchange would
/// deadlock the moment a script wrote more than a pipe buffer's worth while the
/// parent was blocked writing a row.
///
/// **No timeouts.** A migration that is slow because the author's transform is
/// slow is not a migration to abandon halfway.
fn escape_support() -> TokenStream {
    quote! {
        struct __Escape {
            child: std::process::Child,
            stdin: std::process::ChildStdin,
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
                let stdin = child.stdin.take().expect("stdin was piped");
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
                self.stdin
                    .write_all(line.as_bytes())
                    .and_then(|_| self.stdin.write_all(b"\n"))
                    .and_then(|_| self.stdin.flush())
                    .map_err(|e| self.died(&format!("writing a {} row: {}", model, e)))?;

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
                format!(
                    "the transform runtime `{}` failed while {}.{}",
                    self.program,
                    what,
                    self.drain_stderr()
                )
            }

            fn drain_stderr(&mut self) -> String {
                match self.stderr.take().and_then(|h| h.join().ok()) {
                    Some(s) if !s.trim().is_empty() => format!("\n--- its output ---\n{}", s.trim()),
                    _ => String::new(),
                }
            }

            fn finish(mut self) -> ::std::result::Result<(), String> {
                // Dropping stdin is the EOF that ends the child's read loop.
                drop(self.stdin);
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
