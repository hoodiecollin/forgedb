//! Engine-format migration generator (#254, Resolution A).
//!
//! Emits the offline **engine hop bin** — `forgedb migrate engine` — that carries
//! a data dir from one ForgeDB *byte-format generation* to the next. It is the
//! symmetric sibling of [`crate::transform`]: same operator contract (offline,
//! exclusive writer, `src` untouched for rollback, `dest` published by an atomic
//! rename), same identity red lines, one axis over.
//!
//! ## Why this is generated rather than a schema-blind column pass
//!
//! The obvious shape — walk every `manifest.json`, find the columns whose
//! `ColumnType` is `Timestamp`, rescale their bytes — cannot see a third of the
//! data. `storage_column_type_tokens` maps only a **bare** `FieldType::Timestamp`
//! to `ColumnType::Timestamp`; every shape that merely *contains* a timestamp —
//! `timestamp?`, `[timestamp; N]`, a struct field — falls through to
//! `ColumnType::FixedBytes(width)` and is written as a `repr(Rust)` transmute of
//! the Rust value. `Option<i64>` has no niche and `repr(Rust)` guarantees nothing
//! about field order or padding, so a schema-blind reader may not decode it at
//! all. Measured on the example corpus: **81 of 247 timestamp fields are
//! nullable**. A schema-blind pass would leave every one of them in the old unit
//! while the regenerated code read the new one — the silent-wrong-date failure
//! this migration exists to prevent, reintroduced by the migration itself.
//!
//! Which leaves are timestamps, and where they sit inside an `Option` / array /
//! struct, is *schema* knowledge. So it belongs in generated code — which is also
//! the identity-correct answer.
//!
//! ## Identity (the same red lines [`crate::transform`] holds — DV-1)
//!
//! - **No schema at runtime (C1).** The emitted crate parses no `.forge` and links
//!   neither `forgedb-parser` nor `forgedb-migrations`. Both engine generations
//!   are baked-in generated typed code.
//! - **Straight-line, no interpreter (C2).** `migrate` is a fixed sequence of
//!   per-model copy loops with a per-leaf rescale frozen at generation time. There
//!   is no descriptor list and no runtime branch on field type.
//! - **Provider-free (C4).** Only the schema-agnostic substrate the generated app
//!   already links.
//!
//! ## Shape
//!
//! The crate embeds **two** [`RustGenerator`] emissions of the *same* schema at
//! the *same* schema serial, differing only in the baked
//! `EXPECTED_ENGINE_VERSION`. The on-disk *layout* is identical across the pair —
//! only the *meaning* of the timestamp columns changed — so the reader half opens
//! the stale dir legally and the writer half stamps the new generation. The
//! existing open-guard interlock does the version enforcement for free, exactly as
//! it does for `vN`/`vM` in the schema transformer. No escape hatch is needed.
//!
//! The hop itself is a per-field, generate-time-fixed rescale of every timestamp
//! leaf: `as_micros()` on the reader side returns the raw stored `i64`, which for
//! a generation-1 dir *is* seconds, so the multiply is the whole transform.

use crate::rust::RustGenerator;
use crate::transform::TransformCrate;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, Model, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// The one engine-format generation hop that exists: generation 1 stored
/// timestamps as **seconds**, generation 2 stores them as **microseconds**.
pub const SECONDS_TO_MICROS: i64 = 1_000_000;

/// What the CLI hands the generator.
pub struct EngineHopPlan<'a> {
    /// The app's current schema — the SAME schema on both sides of the hop. An
    /// engine bump changes no `.forge`.
    pub schema: &'a Schema,
    /// The data dir's schema-migration serial, baked into both modules so each
    /// one's open-guard still enforces the schema interlock.
    pub schema_version: u32,
    /// The generation the source dir is stamped at.
    pub from_engine: u32,
    /// The generation to write.
    pub to_engine: u32,
}

/// Generates the offline engine-hop crate.
pub struct EngineMigrationGenerator;

impl EngineMigrationGenerator {
    /// Emit the engine-hop crate for `plan`.
    pub fn generate(plan: &EngineHopPlan, crate_name: &str) -> Result<TransformCrate> {
        use crate::CodegenError;

        // Only one generation hop has ever existed. Refusing anything else is not
        // a stub: a second hop would need its own per-leaf rescale, and silently
        // emitting the seconds→micros multiply for it would corrupt the data it
        // claimed to migrate.
        if plan.from_engine != 1 || plan.to_engine != 2 {
            return Err(CodegenError::GenerationFailed(format!(
                "no engine hop is defined for generation {} → {} (the only hop is 1 → 2, \
                 timestamps seconds → microseconds)",
                plan.from_engine, plan.to_engine
            )));
        }

        let mut sources = Vec::new();
        for engine in [plan.from_engine, plan.to_engine] {
            let code =
                RustGenerator::generate_with_versions(plan.schema, plan.schema_version, engine)?
                    .code;
            sources.push((format!("src/e{engine}.rs"), code));
        }
        sources.push(("src/main.rs".to_string(), Self::generate_main(plan)?));

        Ok(TransformCrate {
            crate_name: crate_name.to_string(),
            cargo_toml: crate::transform::TransformGenerator::cargo_toml(crate_name),
            sources,
        })
    }

    /// Just `src/main.rs`, for the codegen guard tests.
    pub fn generate_main_code(plan: &EngineHopPlan) -> Result<GeneratedCode> {
        Ok(GeneratedCode {
            code: Self::generate_main(plan)?,
            description: "ForgeDB engine-format migration entrypoint".to_string(),
        })
    }

    fn generate_main(plan: &EngineHopPlan) -> Result<String> {
        let from = plan.from_engine;
        let to = plan.to_engine;
        let efrom = format_ident!("e{}", from);
        let eto = format_ident!("e{}", to);

        let model_loops: Vec<TokenStream> = plan
            .schema
            .models
            .iter()
            .filter(|m| is_transactable(m))
            .map(|model| Self::model_loop(plan.schema, model, &eto))
            .collect();

        // M2M junction pairs copy verbatim: a junction column holds identity keys,
        // and no generation-1 dir can hold a timestamp identity (a timestamp
        // identity is exactly what this issue introduces).
        let junction_loops: Vec<TokenStream> = RustGenerator::valid_m2m(plan.schema)
            .iter()
            .map(|m| {
                let jf = RustGenerator::junction_field_ident(m);
                quote! { for (__l, __r) in __src.#jf.pairs() { __dst.#jf.link(__l, __r); } }
            })
            .collect();

        let usage = format!(
            "ForgeDB engine-format migration (engine generation {from} -> {to}: \
             timestamps seconds -> microseconds)\n\
             usage: <this-bin> <src-data-dir> <dest-data-dir>"
        );
        let ratio = proc_macro2::Literal::i64_unsuffixed(SECONDS_TO_MICROS);

        let tokens = quote! {
            #![allow(warnings)]

            mod #efrom;
            mod #eto;

            /// Rescale one stored instant across the generation boundary.
            ///
            /// The reader half is generated by CURRENT codegen, so `as_micros()`
            /// hands back the raw stored `i64` verbatim — which in a generation-1
            /// dir is a count of seconds. Multiplying is the entire hop.
            ///
            /// `checked_mul` rather than `*`: a stored second count beyond roughly
            /// year 9999 would overflow into a nonsensical instant, and a
            /// migration that silently produced one would be the exact failure
            /// this whole exercise exists to prevent. The sentinel carries through
            /// untouched for free — `0 * #ratio == 0`.
            fn __rescale(
                __t: forgedb_types::Timestamp,
                __model: &str,
                __field: &str,
            ) -> ::std::result::Result<forgedb_types::Timestamp, String> {
                match __t.as_micros().checked_mul(#ratio) {
                    Some(__u) => Ok(forgedb_types::Timestamp::from_micros(__u)),
                    None => Err(format!(
                        "{}.{}: stored value {} seconds overflows the microsecond \
                         representation; this row cannot be migrated automatically",
                        __model, __field, __t.as_micros(),
                    )),
                }
            }

            /// Carry one data dir across the engine-format generation boundary.
            ///
            /// Both dirs are opened through the generated `Database`, so the #89
            /// `DirLock` makes the migration an exclusive writer and the baked
            /// open-guard refuses a source dir that is not stamped at the source
            /// generation — the interlock, for free.
            fn migrate(
                __src_dir: &std::path::Path,
                __dst_dir: &std::path::Path,
            ) -> ::std::result::Result<(), String> {
                std::fs::create_dir_all(__dst_dir).map_err(|e| format!("mkdir dst: {}", e))?;
                let __src = #efrom::Database::open_at(__src_dir.to_path_buf());
                let mut __dst = #eto::Database::open_at(__dst_dir.to_path_buf());
                #(#model_loops)*
                #(#junction_loops)*
                __dst.commit().map_err(|e| format!("commit: {}", e))?;
                Ok(())
            }

            /// Write the destination under a work dir and publish it with a single
            /// rename, so a crash leaves either the pristine source or an
            /// unpublished partial — never a half-migrated dir the app would open.
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
                let __work = __parent.join(".forgedb-engine-work");
                let _ = std::fs::remove_dir_all(&__work);
                std::fs::create_dir_all(&__work).map_err(|e| format!("mkdir work: {}", e))?;
                let __staged = __work.join("staged");
                migrate(__src, &__staged)?;
                if let Some(__p) = __final_dst.parent() {
                    if !__p.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(__p);
                    }
                }
                std::fs::rename(&__staged, __final_dst)
                    .map_err(|e| format!("atomic publish rename failed: {}", e))?;
                let _ = std::fs::remove_dir_all(&__work);
                Ok(())
            }

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
                            "\u{2713} migrated {:?} -> {:?} (engine generation {} -> {})",
                            __src, __dst, #from, #to
                        );
                    }
                    Err(__e) => {
                        eprintln!("engine migration failed: {}", __e);
                        std::process::exit(1);
                    }
                }
            }
        };

        let file = syn::parse2::<syn::File>(tokens).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!("engine-hop main.rs parse: {e}"))
        })?;
        Ok(prettyplease::unparse(&file))
    }

    /// One model's copy loop: read the row through the source generation's typed
    /// struct, rescale every timestamp leaf, then hand it to the destination
    /// generation's writer.
    ///
    /// The two generations' structs are the *same shape* (same schema, same
    /// generator) and differ only in a baked constant, so JSON is a faithful
    /// transport between them — and after the rescale each instant is a real
    /// instant, so the RFC 3339 round trip is exact at microsecond resolution.
    fn model_loop(schema: &Schema, model: &Model, eto: &proc_macro2::Ident) -> TokenStream {
        let field = format_ident!("{}", RustGenerator::to_snake_case(&model.name));
        let ty = format_ident!("{}", model.name);
        let model_str = model.name.as_str();

        let mut rescales = Vec::new();
        for f in &model.fields {
            let fident = format_ident!("{}", f.name);
            Self::rescale_walk(
                schema,
                &f.field_type,
                quote! { __row.#fident },
                model_str,
                &f.name,
                &mut rescales,
                0,
            );
        }

        quote! {
            for mut __row in __src.#field.all() {
                #(#rescales)*
                let __j = serde_json::to_value(&__row)
                    .map_err(|e| format!("serialize {}: {}", #model_str, e))?;
                let __rec: #eto::#ty = serde_json::from_value(__j)
                    .map_err(|e| format!("decode {}: {}", #model_str, e))?;
                __dst.#field.insert(__rec)
                    .map_err(|e| format!("insert {}: {:?}", #model_str, e))?;
            }
        }
    }

    /// Emit the rescale for one leaf, recursing through `?`, `[T; N]` and structs
    /// so no timestamp is missed — the whole reason this migration is generated.
    fn rescale_walk(
        schema: &Schema,
        ty: &FieldType,
        place: TokenStream,
        model: &str,
        path: &str,
        out: &mut Vec<TokenStream>,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        match ty {
            FieldType::Timestamp(_) => {
                // Precision is irrelevant to the hop: the *storage* unit changed,
                // and every declared precision stored micros afterwards.
                let path_str = path.to_string();
                out.push(quote! { #place = __rescale(#place, #model, #path_str)?; });
            }
            FieldType::Nullable(inner) => {
                let mut nested = Vec::new();
                Self::rescale_walk(
                    schema,
                    inner,
                    quote! { (*__ts_opt) },
                    model,
                    path,
                    &mut nested,
                    depth + 1,
                );
                if !nested.is_empty() {
                    out.push(quote! {
                        if let Some(__ts_opt) = &mut #place { #(#nested)* }
                    });
                }
            }
            FieldType::FixedArray(inner, _) => {
                let mut nested = Vec::new();
                Self::rescale_walk(
                    schema,
                    inner,
                    quote! { (*__ts_elem) },
                    model,
                    path,
                    &mut nested,
                    depth + 1,
                );
                if !nested.is_empty() {
                    out.push(quote! {
                        for __ts_elem in #place.iter_mut() { #(#nested)* }
                    });
                }
            }
            FieldType::StructType(name) => {
                let Some(def) = schema.find_struct(name) else {
                    return;
                };
                for f in &def.fields {
                    let fident = format_ident!("{}", f.name);
                    Self::rescale_walk(
                        schema,
                        &f.field_type,
                        quote! { #place.#fident },
                        model,
                        &format!("{path}.{}", f.name),
                        out,
                        depth + 1,
                    );
                }
            }
            FieldType::OptionalStructType(name) => {
                let Some(def) = schema.find_struct(name) else {
                    return;
                };
                let mut nested = Vec::new();
                for f in &def.fields {
                    let fident = format_ident!("{}", f.name);
                    Self::rescale_walk(
                        schema,
                        &f.field_type,
                        quote! { __ts_struct.#fident },
                        model,
                        &format!("{path}.{}", f.name),
                        &mut nested,
                        depth + 1,
                    );
                }
                if !nested.is_empty() {
                    out.push(quote! {
                        if let Some(__ts_struct) = &mut #place { #(#nested)* }
                    });
                }
            }
            _ => {}
        }
    }
}

/// A model participates in the copy loop only if its rows are addressed by id
/// (`all()` / `insert`) — the same predicate the schema transformer uses.
fn is_transactable(model: &Model) -> bool {
    model.identity_field().is_some()
}
