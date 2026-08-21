//! Derived names for everything ForgeDB builds (#335 §2, epic #332).
//!
//! **Inside the cache, every name is app-unique. Outside, every delivered name
//! is fixed.** The rename happens exactly once, at delivery.
//!
//! Every name a generated artifact is built under used to be a hardcoded
//! literal, and all three collision modes were live:
//!
//! * `[package] name` — two apps in one workspace is a **hard error that makes
//!   the whole workspace unloadable**, not just the colliding pair.
//! * `[lib] name = "forgedb"` in both the napi and pyo3 scaffolds — cargo
//!   *warns*, **exits 0**, and leaves one `libforgedb.dylib` on disk.
//! * `[[bin]] name = "forgedb-transform"` — one app's own `transform/` and
//!   `engine/` declared the **same bin**, so the CLI could run the wrong hop
//!   over a user's data dir at exit 0. A data-corruption-class failure behind a
//!   warning.
//!
//! This module is the one definition of every such name, so a rename is one
//! edit and a drift is one golden-vector failure.
//!
//! # `[lib] name` is deliberately absent from this API
//!
//! §2 deletes the hardcoded `name = "forgedb"` from the napi and pyo3 scaffolds
//! and lets cargo derive the lib name from the package name. A function here to
//! compute one would be a way to set it again, which is what made the *silent*
//! collision representable in the first place.
//!
//! # Names are derived from the schema's path, not from a hash
//!
//! An app's name is `<project_id>_<path segments…>` — `foo_services_blog` — and
//! uniqueness is **structural**: two distinct relative paths cannot reduce to
//! the same segment list. The earlier scheme disambiguated with
//! [`crate::cache::member_hash`], giving names like `schema-60acb6cba9beb3cf-core`.
//! That was unique but unreadable, and it is what a user reads in `Compiling …`,
//! in `nm` output, and in a debugger.
//!
//! **The file name alone cannot do this job.** [`app_segments`] keeps the
//! *directory* components precisely because `file_stem()` discards them, and
//! every app `forgedb init` scaffolds is called `schema.forge` — all 18 schemas
//! in `examples/` are. A stem-derived name is therefore the same constant for
//! every app in a project, which is the collision this module exists to prevent.
//!
//! # What that trade costs: stability
//!
//! A hash of one path is a function of that path alone. A shortest-unique-suffix
//! name is a function of the **whole project's app set**, so adding an app can
//! rename an existing one — re-keying its cached packages and changing every
//! exported C symbol, which breaks already-linked FFI consumers until they
//! rebuild. Accepted deliberately; [`SymbolNaming::Uniform`] narrows it to
//! renames caused by *moving* a schema, and cannot eliminate it.
//!
//! The member **directory** is still keyed by `member_hash`, and deliberately:
//! it is an internal storage key nobody reads, so it keeps the stability the
//! public names gave up, and the cache layout needs no migration.

use std::path::Path;

/// The fallback name, and the prefix that rescues one starting with a digit.
///
/// **Cargo package names cannot start with a digit** — a hard error that also
/// makes the manifest unreadable to `cargo metadata`, i.e. it takes the whole
/// project down rather than just that package.
const SLUG_FALLBACK: &str = "app";

/// Which command owns pruning a package kind (#335 §3, rule 4).
///
/// Without this split the first `generate` after a `migrate build` deletes the
/// transformer: `generate` would see a `transform-*` directory that no
/// `[generate].targets` value declares and reap it as garbage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneOwner {
    /// `generate` and `build` own `core|server|napi|pyo3|ffi|wasm`.
    GenerateBuild,
    /// `migrate build` owns `transform-*`.
    MigrateBuild,
    /// `migrate engine` owns `engine-*`.
    MigrateEngine,
}

/// One cargo package inside an app's cache container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageKind {
    /// The one `database.rs`, as `src/lib.rs`. Exists iff any Rust package does.
    Core,
    /// `api.rs` + a generated `main.rs`. Emitted when the API target is declared.
    Server,
    /// The `#[napi]` wrapper.
    Napi,
    /// The `#[pymodule]` wrapper.
    Pyo3,
    /// The C-ABI wrapper — cdylib **+ staticlib**, the latter being what Go links.
    Ffi,
    /// The browser read-replica wrapper.
    Wasm,
    /// A data-migration transformer for one lineage range.
    Transform { from: u32, to: u32 },
    /// An engine-generation hop for one range.
    Engine { from: u32, to: u32 },
}

impl PackageKind {
    /// The member subdirectory name: `core`, `transform-3-4`, …
    ///
    /// **`transform` and `engine` are range-stamped.** One `transform/` per app
    /// collides across ranges and `migrate run` gets whichever built last. That
    /// collision pre-exists at today's shared `migrations/transform` default,
    /// but moving it into a directory the user never opens turns a *visible*
    /// collision into an *invisible* one.
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

    /// Parse a member subdirectory name back into a kind.
    ///
    /// **`None` means "not a name ForgeDB emits", and the caller must leave that
    /// directory alone** — never reap it. §3's asymmetry is deliberate and this
    /// is where it is enforced: a wrongly-kept package is *inert* (a package on
    /// disk that `members` does not name is never built — measured), while a
    /// wrongly-deleted one costs a regeneration. Fail toward keeping.
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
            // Reject a third segment rather than ignoring it: `transform-1-2-3`
            // is not a name we emit, so it falls to the leave-it-alone branch.
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

    /// Which command may prune this kind (#335 §3, rule 4).
    pub fn owner(&self) -> PruneOwner {
        match self {
            PackageKind::Transform { .. } => PruneOwner::MigrateBuild,
            PackageKind::Engine { .. } => PruneOwner::MigrateEngine,
            _ => PruneOwner::GenerateBuild,
        }
    }

    /// Whether this kind belongs in `default-members` (#335 §4).
    ///
    /// A `wasm/` member poisons a plain `cargo build` at the root — the replica
    /// imports `forgedb_storage::persist`/`::store`, which exist only on
    /// `wasm32`, so the root build fails with `E0432`. C7 prints the cache path,
    /// so a user *will* eventually `cd` there and type `cargo build`; it must not
    /// explode. `transform`/`engine` are excluded because they are built on
    /// demand by `migrate`, not as part of an app's ordinary build.
    ///
    /// **`core` is never excluded**, and `core` exists whenever any Rust package
    /// does — including for a wasm-only app, since `wasm/` links `core`.
    pub fn is_default_member(&self) -> bool {
        !matches!(
            self,
            PackageKind::Wasm | PackageKind::Transform { .. } | PackageKind::Engine { .. }
        )
    }
}


/// How much of an app's path its name carries when the leaf is already unique.
///
/// Declared as `[project].symbol_naming` on the **root** config, because it is
/// a property of the project's whole app set rather than of any one app: under
/// [`SymbolNaming::Minimal`] whether `blog` needs `services_` in front depends
/// on whether some *other* app is also called `blog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolNaming {
    /// The shortest trailing run of path segments that is unique in the
    /// project. `services/cart/schema.forge` alone in its project is `cart`.
    #[default]
    Minimal,
    /// Every segment, always. Costs legibility, buys one thing `Minimal`
    /// cannot: a name that does not depend on the app's siblings, so adding
    /// one never renames another.
    Uniform,
}

/// The conventional stem, dropped from a name because it carries no
/// information: `forgedb init` writes `schema.forge` for every project it
/// scaffolds, and all 18 schemas in `examples/` are named it.
const CONVENTIONAL_STEM: &str = "schema";

/// The path segments identifying one app: its directory components, plus its
/// file stem when that stem is not the conventional [`CONVENTIONAL_STEM`].
///
/// Each segment is sanitised the way [`slug`] sanitises a stem, so the joined
/// result is a legal cargo package name and a legal C identifier.
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

/// Lowercase, every run of non-alphanumerics collapsed to a single `_`, ends
/// trimmed. Shared by [`app_segments`] and [`slug`] so a segment and a stem can
/// never disagree about what a legal character is.
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

/// One app's legible identity: `<project_id>_<segments…>`.
///
/// **This is a function of the project's whole app set, not of one schema.**
/// That is the price of dropping the hash: `siblings` must list every schema in
/// the project, project-relative, including `rel_schema` itself. Pass the same
/// set for every app in one run, or two apps can pick the same name.
///
/// Uniqueness is structural rather than probabilistic — two distinct relative
/// paths cannot reduce to the same segment list under [`SymbolNaming::Uniform`],
/// and under `Minimal` the suffix is lengthened until it is unique. What it
/// gives up is **stability**: adding `app/blog/schema.forge` to a project that
/// already has `services/blog/schema.forge` renames the latter, which re-keys
/// its cached packages and changes every exported C symbol. Accepted
/// deliberately (see the `symbol_naming` note on `ProjectConfig`).
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

    // Dropping the conventional stem makes a *file* named `api` and a
    // *directory* named `api` reduce to the same segments: `app/blog/api.forge`
    // and `app/blog/api/schema.forge` both give `[app, blog, api]`. Distinct
    // paths, identical name — a silent C-symbol collision, which is the exact
    // failure the hash used to make impossible.
    //
    // In such a pair exactly one side dropped a stem (two paths that both kept
    // theirs and agree on every segment ARE the same path), so restoring it on
    // that side is deterministic and needs no tie-break.
    if others.iter().any(|o| *o == mine) && dropped_conventional_stem(rel_schema) {
        mine.push(CONVENTIONAL_STEM.to_string());
    }

    let take = match mode {
        SymbolNaming::Uniform => mine.len(),
        SymbolNaming::Minimal => {
            // Lengthen the suffix until no sibling shares it. `mine.len()` is
            // always sufficient: distinct paths cannot share their full segment
            // list, so the loop terminates without a unique-suffix fallback.
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

/// Whether this schema's own stem was omitted from its segments because it is
/// the conventional one.
fn dropped_conventional_stem(rel_schema: &Path) -> bool {
    rel_schema
        .file_stem()
        .map(|s| sanitize(&s.to_string_lossy()) == CONVENTIONAL_STEM)
        .unwrap_or(false)
}

/// The last `n` segments of `segs` (all of them when `n` exceeds its length).
fn suffix(segs: &[String], n: usize) -> &[String] {
    let start = segs.len().saturating_sub(n);
    &segs[start..]
}

/// `<app_name>-<kind>` — the cargo `[package] name` for one app's package.
///
/// Uniqueness rests on `<app_name>` + `<kind>`. Two apps in one project cannot
/// collide because [`app_name`] is derived from their distinct relative paths;
/// one app's own packages cannot collide because their kinds do.
pub fn package_name(app_name: &str, kind: &PackageKind) -> String {
    format!("{app_name}-{}", kind.dir())
}

/// The `[[bin]]` name for a class-C package.
///
/// Derived from the same scheme, which is what makes it safe to delete
/// `migrate.rs`'s `const TRANSFORM_BIN`. **A fixed constant and a derived name
/// cannot both be the answer**: the constant is how the CLI came to resolve the
/// transformer by a literal and could therefore run the wrong app's binary — or
/// the engine hop's — over a user's data dir at exit 0.
pub fn bin_name(app_name: &str, kind: &PackageKind) -> String {
    package_name(app_name, kind)
}

/// The per-app prefix on every exported FFI C symbol (#335 §2, decision 9).
///
/// Every exported symbol is already schema-derived, but the `forgedb_` prefix
/// was a **constant**, so two apps in one project that both declare a `Post`
/// exported byte-identical symbols. Cargo never sees it.
///
/// Under the cdylib path that was a load-time collision only if one process
/// loaded both. **Static linking makes it a link-time collision in a single Go
/// binary that imports two ForgeDB packages** — reachable, and silent until
/// late.
///
/// The result is a valid C identifier: `-` becomes `_`, and [`app_name`]
/// already guarantees a leading ASCII letter.
///
/// The discriminating information is the app's **relative path**, which is why
/// the prefix cannot be built from the schema's file name alone: `file_stem()`
/// discards the directory, and every app `forgedb init` scaffolds is called
/// `schema.forge`, so a stem-derived prefix is the same constant for all of
/// them — the exact collision this exists to prevent.
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

    /// The directory components are the whole point: `file_stem()` throws them
    /// away, and every app `forgedb init` scaffolds is called `schema.forge`.
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

    /// Cargo rejects a package name starting with a digit, and the rejection is
    /// project-wide rather than local to that package.
    #[test]
    fn a_name_never_starts_with_a_digit() {
        let one = vec![PathBuf::from("2024-orders/schema.forge")];
        let name = app_name("", Path::new("2024-orders/schema.forge"), &one, SymbolNaming::Minimal);
        assert_eq!(name, "app_2024_orders");
        // Not merely "does not start with a digit" — the fallback must KEEP the
        // information, because legibility is the whole reason the hash went away.
        assert!(name.contains("2024_orders"));
    }

    #[test]
    fn a_name_falls_back_when_nothing_survives() {
        let none: Vec<PathBuf> = Vec::new();
        assert_eq!(app_name("", Path::new("___.forge"), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
        assert_eq!(app_name("", Path::new(""), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
        // Non-ASCII is dropped, not transliterated: a lossy transliteration
        // would be a second thing that can differ between platforms.
        assert_eq!(app_name("", Path::new("\u{65e5}\u{672c}\u{8a9e}.forge"), &none, SymbolNaming::Minimal), SLUG_FALLBACK);
    }

    /// A dotfile has no extension as far as `file_stem` is concerned, so a
    /// schema literally named `.forge` stems to `.forge`. Asserted rather than
    /// assumed — this reads like a fallback case and is not one.
    #[test]
    fn a_dotfile_schema_keeps_its_name() {
        let one = vec![PathBuf::from(".forge")];
        assert_eq!(app_name("", Path::new(".forge"), &one, SymbolNaming::Minimal), "forge");
    }

    #[test]
    fn package_names_are_unique_per_kind_and_per_app() {
        let a = "proj_services_blog";
        let b = "proj_app_blog";

        // One app, two kinds — the transform/engine pair that ships colliding.
        assert_ne!(
            package_name(a, &PackageKind::Transform { from: 1, to: 2 }),
            package_name(a, &PackageKind::Engine { from: 1, to: 2 }),
        );
        // Two apps, same kind — separated by the derived name, which is where
        // the hash used to do the work.
        assert_ne!(
            package_name(a, &PackageKind::Core),
            package_name(b, &PackageKind::Core),
        );
        // One app, two ranges of the same kind.
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

    /// An unrecognised directory is left alone, never reaped.
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

    /// A C identifier is `[A-Za-z_][A-Za-z0-9_]*`; a `-` anywhere would not
    /// compile, and a leading digit would not either.
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

    /// The property mandatory scenario 3 rests on: two apps in one project
    /// never share a symbol prefix, so their exported C symbol sets are
    /// disjoint and a single Go binary can link both.
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
