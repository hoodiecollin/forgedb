use std::path::Path;

const SLUG_FALLBACK: &str = "app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneOwner {
    GenerateBuild,
    MigrateBuild,
    MigrateEngine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageKind {
    Core,
    Server,
    Napi,
    Pyo3,
    Ffi,
    Wasm,
    Transform { from: u32, to: u32 },
    Engine { from: u32, to: u32 },
}

impl PackageKind {
    pub fn dir(&self) -> String {
        match self {
            PackageKind::Core => "core".to_string(),
            PackageKind::Server => "server".to_string(),
            PackageKind::Napi => "napi".to_string(),
            PackageKind::Pyo3 => "pyo3".to_string(),
            PackageKind::Ffi => "ffi".to_string(),
            PackageKind::Wasm => "wasm".to_string(),
            PackageKind::Transform { from, to } => format!("transform-{from}-{to}"),
            PackageKind::Engine { from, to } => format!("engine-{from}-{to}"),
        }
    }

    pub fn from_dir(name: &str) -> Option<PackageKind> {
        match name {
            "core" => return Some(PackageKind::Core),
            "server" => return Some(PackageKind::Server),
            "napi" => return Some(PackageKind::Napi),
            "pyo3" => return Some(PackageKind::Pyo3),
            "ffi" => return Some(PackageKind::Ffi),
            "wasm" => return Some(PackageKind::Wasm),
            _ => {}
        }

        let ranged = |rest: &str| -> Option<(u32, u32)> {
            let (from, to) = rest.split_once('-')?;
            if to.contains('-') {
                return None;
            }
            Some((from.parse().ok()?, to.parse().ok()?))
        };

        if let Some(rest) = name.strip_prefix("transform-") {
            let (from, to) = ranged(rest)?;
            return Some(PackageKind::Transform { from, to });
        }
        if let Some(rest) = name.strip_prefix("engine-") {
            let (from, to) = ranged(rest)?;
            return Some(PackageKind::Engine { from, to });
        }
        None
    }

    pub fn owner(&self) -> PruneOwner {
        match self {
            PackageKind::Transform { .. } => PruneOwner::MigrateBuild,
            PackageKind::Engine { .. } => PruneOwner::MigrateEngine,
            _ => PruneOwner::GenerateBuild,
        }
    }

    pub fn is_default_member(&self) -> bool {
        !matches!(
            self,
            PackageKind::Wasm | PackageKind::Transform { .. } | PackageKind::Engine { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolNaming {
    #[default]
    Minimal,
    Uniform,
}

const CONVENTIONAL_STEM: &str = "schema";

pub fn app_segments(rel_schema: &Path) -> Vec<String> {
    let mut segs: Vec<String> = rel_schema
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(sanitize(&s.to_string_lossy())),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let stem = rel_schema
        .file_stem()
        .map(|s| sanitize(&s.to_string_lossy()))
        .unwrap_or_default();
    if !stem.is_empty() && stem != CONVENTIONAL_STEM {
        segs.push(stem);
    }
    segs
}

fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('_');
            }
            pending = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending = true;
        }
    }
    out
}

pub fn app_name(
    project_id: &str,
    rel_schema: &Path,
    siblings: &[std::path::PathBuf],
    mode: SymbolNaming,
) -> String {
    let mut mine = app_segments(rel_schema);
    let others: Vec<Vec<String>> = siblings
        .iter()
        .filter(|s| s.as_path() != rel_schema)
        .map(|s| app_segments(s))
        .collect();

    if others.iter().any(|o| *o == mine) && dropped_conventional_stem(rel_schema) {
        mine.push(CONVENTIONAL_STEM.to_string());
    }

    let take = match mode {
        SymbolNaming::Uniform => mine.len(),
        SymbolNaming::Minimal => {
            let mut n = 1;
            while n < mine.len() && others.iter().any(|o| suffix(o, n) == suffix(&mine, n)) {
                n += 1;
            }
            n.max(1)
        }
    };

    let tail = suffix(&mine, take).join("_");
    let id = sanitize(project_id);
    let joined = match (id.is_empty(), tail.is_empty()) {
        (true, true) => SLUG_FALLBACK.to_string(),
        (true, false) => tail,
        (false, true) => id,
        (false, false) => format!("{id}_{tail}"),
    };

    if joined.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{SLUG_FALLBACK}_{joined}")
    } else {
        joined
    }
}

fn dropped_conventional_stem(rel_schema: &Path) -> bool {
    rel_schema
        .file_stem()
        .map(|s| sanitize(&s.to_string_lossy()) == CONVENTIONAL_STEM)
        .unwrap_or(false)
}

fn suffix(segs: &[String], n: usize) -> &[String] {
    let start = segs.len().saturating_sub(n);
    &segs[start..]
}

pub fn package_name(app_name: &str, kind: &PackageKind) -> String {
    format!("{app_name}-{}", kind.dir())
}

pub fn bin_name(app_name: &str, kind: &PackageKind) -> String {
    package_name(app_name, kind)
}

pub fn symbol_prefix(app_name: &str) -> String {
    format!("{}_", app_name.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sanitize_lowercases_and_collapses_separators() {
        assert_eq!(sanitize("My_App"), "my_app");
        assert_eq!(sanitize("blog cms"), "blog_cms");
        assert_eq!(sanitize("a---b"), "a_b");
        assert_eq!(sanitize("_leading"), "leading");
        assert_eq!(sanitize("trailing_"), "trailing");
        assert_eq!(sanitize("__both__"), "both");
    }

    #[test]
    fn segments_keep_the_directories_the_stem_discards() {
        assert_eq!(
            app_segments(Path::new("apps/api/schema.forge")),
            vec!["apps".to_string(), "api".to_string()]
        );
        assert_eq!(app_segments(Path::new("schema.forge")), Vec::<String>::new());
        assert_eq!(
            app_segments(Path::new("apps/api/orders.forge")),
            vec!["apps".to_string(), "api".to_string(), "orders".to_string()]
        );
    }

    #[test]
    fn a_name_never_starts_with_a_digit() {
        let one = vec![PathBuf::from("2024-orders/schema.forge")];
        let name = app_name("", Path::new("2024-orders/schema.forge"), &one, SymbolNaming::Minimal);
        assert_eq!(name, "app_2024_orders");
        assert!(name.contains("2024_orders"));
    }

    #[test]
    fn a_name_falls_back_when_nothing_survives() {
        let none: Vec<PathBuf> = Vec::new();
        assert_eq!(app_name("", Path::new("___.forge"), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
        assert_eq!(app_name("", Path::new(""), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
        assert_eq!(app_name("", Path::new("\u{65e5}\u{672c}\u{8a9e}.forge"), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
    }

    #[test]
    fn a_dotfile_schema_keeps_its_name() {
        let one = vec![PathBuf::from(".forge")];
        assert_eq!(app_name("", Path::new(".forge"), &one, SymbolNaming::Minimal), "forge");
    }

    #[test]
    fn package_names_are_unique_per_kind_and_per_app() {
        let a = "proj_services_blog";
        let b = "proj_app_blog";

        assert_ne!(
            package_name(a, &PackageKind::Transform { from: 1, to: 2 }),
            package_name(a, &PackageKind::Engine { from: 1, to: 2 }),
        );
        assert_ne!(
            package_name(a, &PackageKind::Core),
            package_name(b, &PackageKind::Core),
        );
        assert_ne!(
            package_name(a, &PackageKind::Transform { from: 1, to: 2 }),
            package_name(a, &PackageKind::Transform { from: 2, to: 3 }),
        );
    }

    #[test]
    fn kind_dir_round_trips() {
        let kinds = [
            PackageKind::Core,
            PackageKind::Server,
            PackageKind::Napi,
            PackageKind::Pyo3,
            PackageKind::Ffi,
            PackageKind::Wasm,
            PackageKind::Transform { from: 1, to: 2 },
            PackageKind::Engine { from: 7, to: 8 },
        ];
        for kind in kinds {
            assert_eq!(
                PackageKind::from_dir(&kind.dir()),
                Some(kind.clone()),
                "round trip failed for {}",
                kind.dir()
            );
        }
    }

    #[test]
    fn unknown_dirs_are_not_ours() {
        for name in [
            "",
            "target",
            "src",
            "Core",
            "transform",
            "transform-",
            "transform-1",
            "transform-1-2-3",
            "transform-a-b",
            "engine-1-",
            "coreish",
        ] {
            assert_eq!(PackageKind::from_dir(name), None, "{name} was claimed");
        }
    }

    #[test]
    fn default_members_excludes_exactly_wasm_transform_engine() {
        assert!(PackageKind::Core.is_default_member());
        assert!(PackageKind::Server.is_default_member());
        assert!(PackageKind::Napi.is_default_member());
        assert!(PackageKind::Pyo3.is_default_member());
        assert!(PackageKind::Ffi.is_default_member());
        assert!(!PackageKind::Wasm.is_default_member());
        assert!(!PackageKind::Transform { from: 1, to: 2 }.is_default_member());
        assert!(!PackageKind::Engine { from: 1, to: 2 }.is_default_member());
    }

    #[test]
    fn prune_ownership_splits_the_three_commands() {
        assert_eq!(PackageKind::Core.owner(), PruneOwner::GenerateBuild);
        assert_eq!(PackageKind::Wasm.owner(), PruneOwner::GenerateBuild);
        assert_eq!(
            PackageKind::Transform { from: 1, to: 2 }.owner(),
            PruneOwner::MigrateBuild
        );
        assert_eq!(
            PackageKind::Engine { from: 1, to: 2 }.owner(),
            PruneOwner::MigrateEngine
        );
    }

    #[test]
    fn symbol_prefix_is_a_valid_c_identifier() {
        for rel in [
            "schema.forge",
            "My_App.forge",
            "2024-orders/schema.forge",
            "___.forge",
            "my services/v1.2/order-book.forge",
        ] {
            let one = vec![PathBuf::from(rel)];
            let name = app_name("my proj", Path::new(rel), &one, SymbolNaming::Minimal);
            let prefix = symbol_prefix(&name);
            assert!(
                prefix.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'),
                "{prefix} does not start a C identifier"
            );
            assert!(
                prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{prefix} contains a character illegal in a C identifier"
            );
        }
    }

    #[test]
    fn symbol_prefix_differs_between_apps() {
        let project = vec![
            PathBuf::from("services/blog/schema.forge"),
            PathBuf::from("app/blog/schema.forge"),
        ];
        let a = app_name("proj", &project[0], &project, SymbolNaming::Minimal);
        let b = app_name("proj", &project[1], &project, SymbolNaming::Minimal);
        assert_ne!(symbol_prefix(&a), symbol_prefix(&b));
    }
}
