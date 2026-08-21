//! The generate-target vocabulary (#335 §12, maintainer decision 10).
//!
//! # The problem this module exists to end
//!
//! There were **two** vocabularies for "which target", and they disagreed:
//!
//! | | Recognised names |
//! |---|---|
//! | Config (`[generate].targets`) | `rust`, `typescript`, `api`, `openapi`, `stubs`, `wasm`, `ffi`, `rust-sdk`, `python-sdk`, `go-sdk` |
//! | CLI (`resolve_target`) | `all`, `api`, `openapi`, `stubs`, `ffi`, `transform`, `rust` (+`--sdk`), and runtime × mode |
//!
//! They did not merely differ — they **conflicted**.  `typescript` was legal in
//! a config file and a **hard error** on the command line (*"`generate
//! typescript` was renamed — use `generate node --sdk`"*), while `node --sdk`,
//! the spelling the CLI and every doc teach, had no config spelling at all
//! except the retired one.  One word meaning two different things in two files
//! is the kind of split that only surfaces when somebody copies a value out of
//! a doc into the wrong place.
//!
//! **Decision 10: the config speaks the CLI's #122 runtime × mode vocabulary**,
//! hyphen-joined where a mode is required.  The old spellings are accepted as
//! deprecated aliases that **warn** — not silently, because a deprecated value
//! that behaves identically and says nothing is how the two vocabularies drifted
//! apart in the first place.
//!
//! # Why this landed with the key becoming required
//!
//! `[generate].targets` becomes required in this same release, which is already
//! a breaking change every project must edit.  Fixing the vocabulary in that
//! same edit costs those users nothing extra; deferring it spends a *second*
//! breaking change later, on a key they have just finished migrating.

use crate::error::{CliError, Result};

/// The value the built-in default declares when no config file governs an app.
///
/// **A built-in default is a value like any other.**  It is stated here rather
/// than represented by an absence, because an absent value whose meaning is
/// "everything" is precisely the defect decision 10 removes: it reads as the
/// inverse of what an empty list normally means, and the package prune (§3) is
/// defined against the *declared* set.
pub const DEFAULT_TARGETS: &str = "all";

/// One row of the vocabulary: what a user writes, what it means internally, and
/// the command that produces the same thing.
///
/// The CLI column is not decoration — [`tests::config_and_cli_vocabularies_agree`]
/// asserts every row against `resolve_target`, so the two cannot drift apart
/// again without a test failing.
pub struct TargetName {
    /// What the user writes in `[generate].targets`.
    pub config: &'static str,
    /// The canonical internal name `generate_all`'s filter matches on.
    pub internal: &'static str,
    /// The equivalent command line.
    pub cli: &'static str,
}

/// Every legal value of `[generate].targets`, in the order the error message
/// lists them.
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

/// Pre-#122 spellings, still accepted so an upgrade is not a cliff — but they
/// warn, naming the replacement.
const DEPRECATED: &[(&str, &str)] = &[
    ("typescript", "node-sdk"),
    ("wasm", "browser-replica"),
];

/// One resolved target plus, when the spelling was a retired one, the warning to
/// print.
#[derive(Debug)]
pub struct Resolved {
    pub internal: &'static str,
    pub deprecation: Option<String>,
}

/// Resolve one `[generate].targets` value to its canonical internal name.
pub fn resolve(value: &str) -> Result<Resolved> {
    if let Some(row) = VOCABULARY.iter().find(|r| r.config == value) {
        return Ok(Resolved {
            internal: row.internal,
            deprecation: None,
        });
    }

    if let Some((old, replacement)) = DEPRECATED.iter().find(|(old, _)| *old == value) {
        // Unwrap-free: every replacement above is itself a row.
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

/// The legal set, rendered for a diagnostic — one line per value with the
/// command it corresponds to, because the whole point of decision 10 is that
/// those are the same vocabulary.
pub fn legal_list() -> String {
    VOCABULARY
        .iter()
        .map(|r| format!("  {:<16} ({})\n", r.config, r.cli))
        .collect()
}

/// Every internal target name `all` stands for.
///
/// Derived from [`VOCABULARY`] rather than written out again, so a row added
/// there is reachable from `all` without a second edit — the two-lists problem
/// this module exists to end applies to itself.
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

/// Resolve a whole declared list to canonical internal names, collecting any
/// deprecation warnings.
///
/// **`all` EXPANDS rather than short-circuiting to a marker.** It has to: the
/// opt-in arms of `generate_all` are gated on the filter *naming* them, so a
/// filter that merely said "all" would emit the five always-on targets and skip
/// `ffi`, the replica, the three REST SDKs and the three native bindings.
///
/// That is the shape shipped today, where "absent means everything" and the
/// opt-in targets were nonetheless unreachable from `generate all`. #335 §12
/// calls it the reciprocal hole. Expanding here closes it in one place instead
/// of at each of the eleven call sites that ask the filter a question.
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

            // ...and it means exactly what the replacement means.
            assert_eq!(r.internal, resolve(replacement).unwrap().internal);
        }
    }

    /// **The anti-drift guard.** Every config spelling must name the same
    /// internal target that its documented command line produces.
    ///
    /// Without this the two vocabularies can separate again exactly as they did
    /// before — silently, because each side is internally consistent.
    #[test]
    fn config_and_cli_vocabularies_agree() {
        for row in VOCABULARY {
            // Parse the documented CLI form back into (target, mode).
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

    /// `all` must reach the OPT-IN targets, which is precisely what it does not
    /// do today: the opt-in arms gate on the filter naming them, so "everything"
    /// expressed as an absence emitted only the five always-on generators.
    #[test]
    fn all_expands_to_reach_the_opt_in_targets() {
        let (internal, warnings) = resolve_all(&["all".into()]).expect("resolves");
        assert!(warnings.is_empty());

        for expected in [
            "rust", "typescript", "api", "openapi", "stubs", // always-on
            "ffi", "wasm", "napi", "pyo3", "go", // opt-in / previously unreachable
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
            "bun-sdk".into(), // same internal target
            "typescript".into(), // deprecated, same again
            "rust".into(),
        ])
        .expect("resolves");

        assert_eq!(internal, vec!["typescript".to_string(), "rust".to_string()]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }
}
