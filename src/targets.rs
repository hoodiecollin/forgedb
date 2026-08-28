use crate::error::{CliError, Result};

pub const DEFAULT_TARGETS: &str = "all";

pub struct TargetName {
    pub config: &'static str,
    pub internal: &'static str,
    pub cli: &'static str,
}

pub const VOCABULARY: &[TargetName] = &[
    TargetName { config: "all", internal: "all", cli: "generate all" },
    TargetName { config: "rust", internal: "rust", cli: "generate rust" },
    TargetName { config: "api", internal: "api", cli: "generate api" },
    TargetName { config: "openapi", internal: "openapi", cli: "generate openapi" },
    TargetName { config: "stubs", internal: "stubs", cli: "generate stubs" },
    TargetName { config: "ffi", internal: "ffi", cli: "generate ffi" },
    TargetName { config: "node-sdk", internal: "typescript", cli: "generate node --sdk" },
    TargetName { config: "bun-sdk", internal: "typescript", cli: "generate bun --sdk" },
    TargetName { config: "node-runtime", internal: "napi", cli: "generate node --runtime" },
    TargetName { config: "bun-runtime", internal: "napi", cli: "generate bun --runtime" },
    TargetName { config: "python-runtime", internal: "pyo3", cli: "generate python --runtime" },
    TargetName { config: "python-sdk", internal: "python-sdk", cli: "generate python --sdk" },
    TargetName { config: "go-runtime", internal: "go", cli: "generate go --runtime" },
    TargetName { config: "go-sdk", internal: "go-sdk", cli: "generate go --sdk" },
    TargetName { config: "rust-sdk", internal: "rust-sdk", cli: "generate rust --sdk" },
    TargetName { config: "browser-replica", internal: "wasm", cli: "generate browser --replica" },
];

pub const DEPRECATED: &[(&str, &str)] = &[
    ("typescript", "node-sdk"),
    ("wasm", "browser-replica"),
];

#[derive(Debug)]
pub struct Resolved {
    pub internal: &'static str,
    pub deprecation: Option<String>,
}

pub fn resolve(value: &str) -> Result<Resolved> {
    if let Some(row) = VOCABULARY.iter().find(|r| r.config == value) {
        return Ok(Resolved {
            internal: row.internal,
            deprecation: None,
        });
    }

    if let Some((old, replacement)) = DEPRECATED.iter().find(|(old, _)| *old == value) {
        let row = VOCABULARY
            .iter()
            .find(|r| r.config == *replacement)
            .expect("a deprecated alias must name a real replacement");
        return Ok(Resolved {
            internal: row.internal,
            deprecation: Some(format!(
                "`[generate].targets` value `{old}` was renamed to `{replacement}` \
                 (the spelling `{}` uses). It still works; update it when convenient.",
                row.cli
            )),
        });
    }

    Err(CliError::Config(format!(
        "Unknown `[generate].targets` value `{value}`.\n\nLegal values:\n{}",
        legal_list()
    )))
}

pub fn legal_list() -> String {
    VOCABULARY
        .iter()
        .map(|r| format!("  {:<16} ({})\n", r.config, r.cli))
        .collect()
}

pub fn all_internal() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for row in VOCABULARY {
        if row.internal == "all" {
            continue;
        }
        let name = row.internal.to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

pub fn resolve_all(values: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    let mut internal = Vec::new();
    let mut warnings = Vec::new();

    for value in values {
        let resolved = resolve(value)?;
        if let Some(w) = resolved.deprecation {
            warnings.push(w);
        }
        if resolved.internal == "all" {
            for name in all_internal() {
                if !internal.contains(&name) {
                    internal.push(name);
                }
            }
            continue;
        }
        let name = resolved.internal.to_string();
        if !internal.contains(&name) {
            internal.push(name);
        }
    }

    Ok((internal, warnings))
}

pub fn declared_packages(internal: &[String]) -> Vec<crate::naming::PackageKind> {
    use crate::naming::PackageKind;

    let has = |name: &str| internal.iter().any(|t| t == name);

    let rust_side = ["rust", "api", "napi", "pyo3", "ffi", "wasm", "go"];

    let mut kinds = Vec::new();
    if rust_side.iter().any(|t| has(t)) {
        kinds.push(PackageKind::Core);
    }
    if has("api") {
        kinds.push(PackageKind::Server);
    }
    if has("napi") {
        kinds.push(PackageKind::Napi);
    }
    if has("pyo3") {
        kinds.push(PackageKind::Pyo3);
    }
    if has("ffi") || has("go") {
        kinds.push(PackageKind::Ffi);
    }
    if has("wasm") {
        kinds.push(PackageKind::Wasm);
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::generate::GenerateMode;

    #[test]
    fn every_row_resolves_to_itself() {
        for row in VOCABULARY {
            let r = resolve(row.config).expect(row.config);
            assert_eq!(r.internal, row.internal);
            assert!(r.deprecation.is_none(), "{} should not warn", row.config);
        }
    }

    #[test]
    fn deprecated_spellings_work_and_warn() {
        for (old, replacement) in DEPRECATED {
            let r = resolve(old).unwrap_or_else(|e| panic!("{old}: {e}"));
            let w = r
                .deprecation
                .unwrap_or_else(|| panic!("{old} resolved without a warning"));
            assert!(w.contains(replacement), "{w} does not name {replacement}");

            assert_eq!(r.internal, resolve(replacement).unwrap().internal);
        }
    }

    #[test]
    fn config_and_cli_vocabularies_agree() {
        for row in VOCABULARY {
            let words: Vec<&str> = row.cli.split_whitespace().collect();
            assert_eq!(words[0], "generate", "{}", row.cli);
            let target = words[1];
            let mode = match words.get(2) {
                Some(&"--sdk") => Some(GenerateMode::Sdk),
                Some(&"--runtime") => Some(GenerateMode::Runtime),
                Some(&"--replica") => Some(GenerateMode::Replica),
                None => None,
                other => panic!("unrecognised mode flag in {:?}: {other:?}", row.cli),
            };

            let via_cli = crate::commands::generate::resolve_target_for_test(target, mode)
                .unwrap_or_else(|e| panic!("`{}` does not resolve: {e}", row.cli));

            assert_eq!(
                via_cli, row.internal,
                "config `{}` means `{}` but `{}` produces `{}`",
                row.config, row.internal, row.cli, via_cli
            );
        }
    }

    #[test]
    fn an_unknown_value_names_the_legal_set() {
        let err = resolve("napi").expect_err("`napi` is an internal name, not a config value");
        let msg = err.to_string();
        assert!(msg.contains("node-runtime"), "{msg}");
        assert!(msg.contains("generate node --runtime"), "{msg}");
    }

    #[test]
    fn all_expands_to_reach_the_opt_in_targets() {
        let (internal, warnings) = resolve_all(&["all".into()]).expect("resolves");
        assert!(warnings.is_empty());

        for expected in [
            "rust", "typescript", "api", "openapi", "stubs",
            "ffi", "wasm", "napi", "pyo3", "go",
            "rust-sdk", "python-sdk", "go-sdk",
        ] {
            assert!(
                internal.iter().any(|t| t == expected),
                "`all` does not reach `{expected}`: {internal:?}"
            );
        }
        assert!(!internal.iter().any(|t| t == "all"), "`all` leaked as a target");
    }

    #[test]
    fn resolve_all_dedupes_and_collects_warnings() {
        let (internal, warnings) = resolve_all(&[
            "node-sdk".into(),
            "bun-sdk".into(),
            "typescript".into(),
            "rust".into(),
        ])
        .expect("resolves");

        assert_eq!(internal, vec!["typescript".to_string(), "rust".to_string()]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    use crate::naming::{PackageKind, PruneOwner};

    #[test]
    fn all_declares_every_generate_owned_package() {
        let (internal, _) = resolve_all(&["all".into()]).unwrap();
        let declared = declared_packages(&internal);

        for kind in [
            PackageKind::Core,
            PackageKind::Server,
            PackageKind::Napi,
            PackageKind::Pyo3,
            PackageKind::Ffi,
            PackageKind::Wasm,
        ] {
            assert!(declared.contains(&kind), "`all` does not declare {}", kind.dir());
        }
    }

    #[test]
    fn no_target_value_declares_a_migrate_owned_package() {
        let (internal, _) = resolve_all(&["all".into()]).unwrap();
        for kind in declared_packages(&internal) {
            assert_eq!(
                kind.owner(),
                PruneOwner::GenerateBuild,
                "{} is not generate's to declare",
                kind.dir()
            );
        }
    }

    #[test]
    fn a_rust_only_app_declares_only_core() {
        let (internal, _) = resolve_all(&["rust".into()]).unwrap();
        assert_eq!(declared_packages(&internal), vec![PackageKind::Core]);
    }

    #[test]
    fn the_go_runtime_declares_the_ffi_package_it_links() {
        let (internal, _) = resolve_all(&["go-runtime".into()]).unwrap();
        let declared = declared_packages(&internal);
        assert!(declared.contains(&PackageKind::Ffi), "{declared:?}");
        assert!(declared.contains(&PackageKind::Core), "{declared:?}");
    }

    #[test]
    fn an_api_app_declares_core_and_server() {
        let (internal, _) = resolve_all(&["api".into()]).unwrap();
        let declared = declared_packages(&internal);
        assert!(declared.contains(&PackageKind::Core), "{declared:?}");
        assert!(declared.contains(&PackageKind::Server), "{declared:?}");
    }


    #[test]
    fn an_sdk_only_app_declares_nothing() {
        let (internal, _) = resolve_all(&["node-sdk".into(), "python-sdk".into()]).unwrap();
        assert!(declared_packages(&internal).is_empty());
    }
}
