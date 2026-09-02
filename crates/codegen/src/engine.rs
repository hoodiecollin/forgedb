use crate::rust::RustGenerator;
use crate::transform::TransformCrate;
use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, Model, Schema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub const SECONDS_TO_MICROS: i64 = 1_000_000;

pub struct EngineHopPlan<'a> {
    pub schema: &'a Schema,
    pub schema_version: u32,
    pub from_engine: u32,
    pub to_engine: u32,
}

pub struct EngineMigrationGenerator;

impl EngineMigrationGenerator {
    pub fn generate(plan: &EngineHopPlan, crate_name: &str) -> Result<TransformCrate> {
        use crate::CodegenError;

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
            cargo_toml: Self::cargo_toml(crate_name),
            sources,
        })
    }

    pub fn cargo_toml(crate_name: &str) -> String {
        format!(
            r#"[package]
name = "{crate_name}"
version = "0.0.0"
edition = "2024"

# A ForgeDB ENGINE-generation hop (#254): the byte-format migration, orthogonal
# to the app's own schema-version lineage. Compiled for one fixed generation
# range and run against data-at-rest with the app stopped.
[[bin]]
name = "{crate_name}"
path = "src/main.rs"

{CLASS_C_SUBSTRATE_DEPS}"#,
            CLASS_C_SUBSTRATE_DEPS = crate::transform::CLASS_C_SUBSTRATE_DEPS,
        )
    }

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

fn is_transactable(model: &Model) -> bool {
    model.identity_field().is_some()
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn transform_and_engine_declare_different_bins() {
        let t = crate::transform::TransformGenerator::cargo_toml("blog-abcd-transform-1-2");
        let e = EngineMigrationGenerator::cargo_toml("blog-abcd-engine-1-2");

        assert!(t.contains("name = \"blog-abcd-transform-1-2\""), "{t}");
        assert!(e.contains("name = \"blog-abcd-engine-1-2\""), "{e}");
        assert!(!t.contains("forgedb-transform"), "{t}");
        assert!(!e.contains("forgedb-transform"), "{e}");
    }

    #[test]
    fn the_bin_section_carries_the_range() {
        for (pkg, kind) in [
            ("app-0-transform-1-2", "transform"),
            ("app-0-engine-1-2", "engine"),
        ] {
            let toml = if kind == "transform" {
                crate::transform::TransformGenerator::cargo_toml(pkg)
            } else {
                EngineMigrationGenerator::cargo_toml(pkg)
            };
            let bin_section = toml
                .split("[[bin]]")
                .nth(1)
                .unwrap_or_else(|| panic!("no [[bin]] section in {toml}"));
            assert!(
                bin_section.contains(&format!("name = \"{pkg}\"")),
                "the [[bin]] section must name the range-stamped package: {toml}"
            );
        }
    }

    #[test]
    fn neither_class_c_manifest_carries_a_profile() {
        for toml in [
            crate::transform::TransformGenerator::cargo_toml("x-transform-1-2"),
            EngineMigrationGenerator::cargo_toml("x-engine-1-2"),
        ] {
            assert!(!toml.contains("[profile"), "{toml}");
            assert!(!toml.contains("opt-level"), "{toml}");
        }
    }

    #[test]
    fn neither_class_c_manifest_declares_its_own_workspace() {
        for toml in [
            crate::transform::TransformGenerator::cargo_toml("x-transform-1-2"),
            EngineMigrationGenerator::cargo_toml("x-engine-1-2"),
        ] {
            assert!(!toml.contains("[workspace]"), "{toml}");
        }
    }

    #[test]
    fn both_link_the_same_provider_free_substrate() {
        let t = crate::transform::TransformGenerator::cargo_toml("x-transform-1-2");
        let e = EngineMigrationGenerator::cargo_toml("x-engine-1-2");
        for toml in [&t, &e] {
            assert!(toml.contains("forgedb-storage ="), "{toml}");
            assert!(toml.contains("forgedb-types ="), "{toml}");
            assert!(!toml.contains("forgedb-parser ="), "{toml}");
            assert!(!toml.contains("forgedb-migrations ="), "{toml}");
        }
        let deps = |s: &str| s.split("[dependencies]").nth(1).unwrap().to_string();
        assert_eq!(deps(&t), deps(&e));
    }
}
