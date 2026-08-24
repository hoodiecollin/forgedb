//! Locating the runtime an escape transform runs on (#374 direction C).
//!
//! **ForgeDB embeds no interpreter.** A transform written in TypeScript or
//! Python runs on the runtime the author already has installed, so `migrate
//! build` has to find it, check it, and bake its absolute path into the emitted
//! hop. This module is that resolution, and it is deliberately its own thing:
//! the *diagnostic* is the whole value here, and a missing interpreter
//! discovered inside a code generator produces a message about code generation.

use crate::config::{InterpreterConfig, ToolchainConfig};
use crate::error::CliError;
use crate::Result;
use forgedb_migrations::EscapeLanguage;
use std::path::{Path, PathBuf};

/// A located, version-checked interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpreter {
    /// The config key it came from (`bun` / `node` / `python`).
    pub name: &'static str,
    /// The path the hop will spawn. Absolute whenever the config gave a path;
    /// otherwise the bare name, resolved by the OS on `PATH` at run time.
    pub program: PathBuf,
    /// What `--version` reported, for the message when something is wrong.
    pub version: String,
}

/// The interpreters that can run each language, **in preference order**.
///
/// TypeScript prefers `bun` because that is the runner ForgeDB's own tooling
/// uses; `node` is the fallback so a project that has one and not the other
/// still works without configuring anything.
fn candidates(lang: EscapeLanguage) -> &'static [(&'static str, &'static str)] {
    match lang {
        // (config key, default program name on PATH)
        EscapeLanguage::TypeScript => &[("bun", "bun"), ("node", "node")],
        EscapeLanguage::Python => &[("python", "python3")],
        // Rust is embedded verbatim into the hop and compiled with it — there
        // is no interpreter to locate.
        EscapeLanguage::Rust => &[],
    }
}

/// Locate the interpreter for `lang`, or fail **naming what was expected and
/// what was found**.
///
/// Called at `migrate build` before a line of code is generated: an interpreter
/// that is missing is missing whether or not the crate compiles, and learning
/// it after a cargo build costs a compile to discover something a `--version`
/// could have said.
pub fn resolve(
    toolchain: &ToolchainConfig,
    project_root: &Path,
    lang: EscapeLanguage,
) -> Result<Interpreter> {
    let mut tried: Vec<String> = Vec::new();

    for (key, default_program) in candidates(lang) {
        let declared = match *key {
            "bun" => toolchain.bun.as_ref(),
            "node" => toolchain.node.as_ref(),
            "python" => toolchain.python.as_ref(),
            _ => None,
        };
        let program = program_path(declared, default_program, project_root);

        match probe(&program) {
            Err(why) => tried.push(format!("  • {key}: {} — {why}", program.display())),
            Ok(version) => {
                let min = declared.and_then(|d| d.min_version.as_deref());
                match min {
                    Some(min) if !version_at_least(&version, min) => {
                        // A DECLARED interpreter that is too old is a hard
                        // error rather than a reason to try the next candidate:
                        // the operator said which one to use.
                        return Err(CliError::Config(format!(
                            "`[toolchain].{key}` requires version {min} or newer, but {} \
                             reports {version}.\n\
                             This migration's transform is written in {}, and ForgeDB runs \
                             it on the interpreter you already have — it embeds none.",
                            program.display(),
                            label(lang),
                        )));
                    }
                    _ => {
                        return Ok(Interpreter {
                            name: key,
                            program,
                            version,
                        });
                    }
                }
            }
        }
    }

    Err(CliError::Config(format!(
        "no {} interpreter could be found, and this migration's transform is written in \
         {}.\n\nTried:\n{}\n\n\
         ForgeDB does not embed a {} runtime — it runs the one you already have. Point at \
         it explicitly:\n\n  [toolchain]\n  {} = {{ path = \"/path/to/{}\" }}\n",
        label(lang),
        label(lang),
        if tried.is_empty() {
            "  (nothing — this language needs no interpreter)".to_string()
        } else {
            tried.join("\n")
        },
        label(lang),
        candidates(lang).first().map(|c| c.0).unwrap_or("bun"),
        candidates(lang).first().map(|c| c.1).unwrap_or("bun"),
    )))
}

fn label(lang: EscapeLanguage) -> &'static str {
    match lang {
        EscapeLanguage::Rust => "Rust",
        EscapeLanguage::TypeScript => "TypeScript",
        EscapeLanguage::Python => "Python",
    }
}

/// Where to look: the declared path (resolved against the **project root**, not
/// the CWD — the #333/#361 rule), or the bare program name for `PATH` lookup.
fn program_path(
    declared: Option<&InterpreterConfig>,
    default_program: &str,
    project_root: &Path,
) -> PathBuf {
    match declared.and_then(|d| d.path.as_deref()) {
        Some(p) => {
            let p = Path::new(p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project_root.join(p)
            }
        }
        None => PathBuf::from(default_program),
    }
}

/// Run `--version` and return what it said, or why it could not be run.
fn probe(program: &Path) -> std::result::Result<String, String> {
    let out = std::process::Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("`--version` exited {}", out.status));
    }
    // `python3 --version` historically wrote to stderr; take whichever is
    // non-empty rather than assuming.
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let text = if stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).trim().to_string()
    } else {
        stdout
    };
    if text.is_empty() {
        return Err("`--version` printed nothing".to_string());
    }
    Ok(text)
}

/// Is `reported` at least `min`?
///
/// Compares dotted numeric components, missing components as zero, so
/// `min_version = "20"` accepts `v20.11.1` and `min_version = "3.11"` accepts
/// `Python 3.12.0`. Everything that is not a leading run of digits in a
/// component is ignored, which is what lets `v20.11.1`, `Python 3.12.0` and
/// `1.1.34` all be read without three parsers.
pub fn version_at_least(reported: &str, min: &str) -> bool {
    let nums = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let got = nums(reported);
    let want = nums(min);
    for i in 0..want.len() {
        let g = got.get(i).copied().unwrap_or(0);
        match g.cmp(&want[i]) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The comparison reads the three shapes the real interpreters print,
    /// without three parsers.
    #[test]
    fn version_comparison_reads_every_shape_the_runtimes_print() {
        assert!(version_at_least("1.1.34", "1.1"));
        assert!(version_at_least("v20.11.1", "20"));
        assert!(version_at_least("Python 3.12.0", "3.11"));
        assert!(version_at_least("Python 3.11.9", "3.11"));

        assert!(!version_at_least("Python 3.10.14", "3.11"));
        assert!(!version_at_least("v18.20.0", "20"));
        assert!(!version_at_least("1.0.9", "1.1"));

        // A missing component is zero, not "unknown": `3` is NOT >= `3.11`.
        assert!(!version_at_least("Python 3", "3.11"));
        assert!(version_at_least("Python 3", "3"));
        // No constraint beyond what was asked for.
        assert!(version_at_least("1.2.3", "1"));
    }

    #[test]
    fn rust_needs_no_interpreter_and_says_so() {
        let err = resolve(
            &ToolchainConfig::default(),
            Path::new("/nowhere"),
            EscapeLanguage::Rust,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("needs no interpreter"),
            "a Rust escape is compiled INTO the hop: {err}"
        );
    }

    /// A declared path that is not there names the path — not "command not
    /// found", which would send the operator to their `PATH`.
    #[test]
    fn a_declared_path_that_is_missing_names_the_path() {
        let cfg = ToolchainConfig {
            python: Some(InterpreterConfig {
                path: Some("/definitely/not/here/python".into()),
                min_version: None,
            }),
            ..Default::default()
        };
        let err = resolve(&cfg, Path::new("/tmp"), EscapeLanguage::Python).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/definitely/not/here/python"), "{msg}");
        assert!(msg.contains("[toolchain]"), "{msg}");
    }

    /// A relative declared path resolves against the PROJECT ROOT, not the CWD
    /// (#333/#361) — a `.venv/bin/python` means the project's venv wherever the
    /// operator happens to be standing.
    #[test]
    fn a_relative_path_resolves_against_the_project_root() {
        let cfg = ToolchainConfig {
            python: Some(InterpreterConfig {
                path: Some(".venv/bin/python".into()),
                min_version: None,
            }),
            ..Default::default()
        };
        let err = resolve(&cfg, Path::new("/some/project"), EscapeLanguage::Python).unwrap_err();
        assert!(
            err.to_string().contains("/some/project/.venv/bin/python"),
            "{err}"
        );
    }
}
