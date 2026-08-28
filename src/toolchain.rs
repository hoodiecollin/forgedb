use crate::config::{InterpreterConfig, ToolchainConfig};
use crate::error::CliError;
use crate::Result;
use forgedb_migrations::EscapeLanguage;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpreter {
    pub name: &'static str,
    pub program: PathBuf,
    pub version: String,
}

fn candidates(lang: EscapeLanguage) -> &'static [(&'static str, &'static str)] {
    match lang {
        EscapeLanguage::TypeScript => &[("bun", "bun"), ("node", "node")],
        EscapeLanguage::Python => &[("python", "python3")],
        EscapeLanguage::Rust => &[],
    }
}

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

fn probe(program: &Path) -> std::result::Result<String, String> {
    let out = std::process::Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("`--version` exited {}", out.status));
    }
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

pub fn version_at_least(reported: &str, min: &str) -> bool {
    let nums = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let got = nums(reported);
    let want = nums(min);
    for (i, w) in want.iter().enumerate() {
        let g = got.get(i).copied().unwrap_or(0);
        match g.cmp(w) {
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

    #[test]
    fn version_comparison_reads_every_shape_the_runtimes_print() {
        assert!(version_at_least("1.1.34", "1.1"));
        assert!(version_at_least("v20.11.1", "20"));
        assert!(version_at_least("Python 3.12.0", "3.11"));
        assert!(version_at_least("Python 3.11.9", "3.11"));

        assert!(!version_at_least("Python 3.10.14", "3.11"));
        assert!(!version_at_least("v18.20.0", "20"));
        assert!(!version_at_least("1.0.9", "1.1"));

        assert!(!version_at_least("Python 3", "3.11"));
        assert!(version_at_least("Python 3", "3"));
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
