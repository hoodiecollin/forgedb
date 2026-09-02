use std::path::{Path, PathBuf};

use crate::naming::PackageKind;
use crate::{Result, error::CliError, ui};

use super::driver::{BuildReport, DeliveredArtifact, TargetKind};

pub struct Destination {
    pub kind: PackageKind,
    pub target_kind: TargetKind,
    pub dir: &'static str,
    pub file: String,
}

pub fn destinations_for(kind: &PackageKind) -> Vec<Destination> {
    match kind {
        PackageKind::Napi => vec![Destination {
            kind: PackageKind::Napi,
            target_kind: TargetKind::Cdylib,
            dir: "napi",
            file: "forgedb.node".to_string(),
        }],
        PackageKind::Pyo3 => vec![Destination {
            kind: PackageKind::Pyo3,
            target_kind: TargetKind::Cdylib,
            dir: "pyo3",
            file: forgedb_codegen::PyO3Generator::extension_file(),
        }],
        PackageKind::Ffi => vec![
            Destination {
                kind: PackageKind::Ffi,
                target_kind: TargetKind::Staticlib,
                dir: "ffi",
                file: GO_STATICLIB.to_string(),
            },
            Destination {
                kind: PackageKind::Ffi,
                target_kind: TargetKind::Staticlib,
                dir: "go",
                file: GO_STATICLIB.to_string(),
            },
        ],
        PackageKind::Core | PackageKind::Server | PackageKind::Wasm => Vec::new(),
        PackageKind::Transform { .. } | PackageKind::Engine { .. } => Vec::new(),
    }
}

pub const GO_STATICLIB: &str = "libforgedb.a";

fn all_destinations() -> Vec<Destination> {
    [
        PackageKind::Core,
        PackageKind::Server,
        PackageKind::Napi,
        PackageKind::Pyo3,
        PackageKind::Ffi,
        PackageKind::Wasm,
    ]
    .iter()
    .flat_map(destinations_for)
    .collect()
}

pub fn run(output: &Path, report: &BuildReport) -> Result<Vec<DeliveredArtifact>> {
    let mut delivered = Vec::new();

    for dest in all_destinations() {
        let dir = output.join(dest.dir);
        if !dir.is_dir() {
            continue;
        }

        let kind_dir = dest.kind.dir();
        let hits: Vec<_> = report
            .artifacts
            .iter()
            .filter(|a| a.kind == kind_dir && a.target_kind == dest.target_kind)
            .collect();

        let source = match hits.as_slice() {
            [one] => *one,
            [] => {
                return Err(CliError::Build(format!(
                    "{} exists but this build produced no `{}` {} to deliver into it.\n\n\
                     It produced:\n{}\n\n\
                     Either the project stopped declaring that target while its output \
                     directory remained, or this is a ForgeDB bug; please report it.",
                    dir.display(),
                    kind_dir,
                    dest.target_kind.as_str(),
                    report.render_inventory()
                )));
            }
            many => {
                return Err(CliError::Build(format!(
                    "{} — {} artifacts match `{}` {}, so which one to deliver is a guess:\n{}",
                    dir.display(),
                    many.len(),
                    kind_dir,
                    dest.target_kind.as_str(),
                    many.iter()
                        .map(|a| format!("  {} ({})", a.path.display(), a.package))
                        .collect::<Vec<_>>()
                        .join("\n")
                )));
            }
        };

        if !source.path.is_file() {
            return Err(CliError::Build(format!(
                "the build reported `{}` at {}, and that file is not there now.\n\
                 ForgeDB does not reconstruct artifact paths — it reads every one from \
                 the report — so this is a file that moved or was deleted after cargo \
                 wrote it, not a path ForgeDB guessed.",
                kind_dir,
                source.path.display()
            )));
        }

        let to = dir.join(&dest.file);
        std::fs::copy(&source.path, &to).map_err(|e| {
            CliError::Build(format!(
                "failed to deliver {} to {}: {e}",
                source.path.display(),
                to.display()
            ))
        })?;

        ui::info(&format!(
            "{} ({}): {}",
            dest.dir,
            dest.target_kind.as_str(),
            to.display()
        ));

        delivered.push(DeliveredArtifact {
            kind: kind_dir,
            target_kind: dest.target_kind,
            from: source.path.clone(),
            to,
        });
    }

    Ok(delivered)
}

pub fn run_if_output(output: Option<&str>, report: &BuildReport) -> Result<Vec<DeliveredArtifact>> {
    let Some(output) = output else {
        return Ok(Vec::new());
    };
    run(&PathBuf::from(output), report)
}
