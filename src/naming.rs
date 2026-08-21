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
//! # Why the slug exists, and why it is load-bearing
//!
//! Uniqueness rests **entirely** on `<hash> + <kind>`; the slug is legibility
//! only, so that `Compiling blog-3f2a…-core` tells the user which app cargo is
//! on. But it cannot be dropped: **cargo package names cannot start with a
//! digit**, and six of sixteen hex digits are digits, so a bare `<hash>-<kind>`
//! scheme breaks for roughly three apps in eight — while passing whatever
//! schema the implementer happened to test on.
//!
//! # Why the hash is reused rather than re-invented
//!
//! `<hash>` is [`crate::cache::member_hash`], unchanged and untruncated. Reusing
//! it rather than inventing a second key means package uniqueness and
//! member-directory uniqueness are *the same fact* — there is no second thing
//! to keep in step, and the golden vectors that pin one pin both.

use std::path::Path;

/// The fallback slug, and the prefix that rescues one starting with a digit.
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

/// A legible, cargo-legal stem derived from the schema's file name.
///
/// Lowercased, with every run of non-alphanumerics collapsed to a single `-`
/// and leading/trailing separators trimmed. **The result always starts with an
/// ASCII letter**, because cargo rejects a package name starting with a digit —
/// a hard error that also makes the manifest unreadable to `cargo metadata`,
/// i.e. it takes the *whole project* down rather than just that package.
///
/// An empty result becomes `app`; one starting with a digit is *prefixed* with
/// `app-` rather than replaced by it, because the slug's only job is legibility
/// and `app-2024-orders` says more than `app` does.
pub fn slug(schema: &Path) -> String {
    let stem = schema
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut out = String::with_capacity(stem.len());
    let mut pending_sep = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            // Non-ASCII is dropped rather than transliterated: the slug is
            // legibility only, and a lossy transliteration would be a second
            // thing that can differ between platforms.
            pending_sep = true;
        }
    }

    if out.is_empty() {
        return SLUG_FALLBACK.to_string();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("{SLUG_FALLBACK}-{out}");
    }
    out
}

/// `<slug>-<hash>-<kind>` — the cargo `[package] name` for one app's package.
///
/// Uniqueness rests entirely on `<hash>` + `<kind>`. Two apps in one project
/// cannot collide because their hashes differ; one app's own packages cannot
/// collide because their kinds do.
pub fn package_name(slug: &str, member_hash: &str, kind: &PackageKind) -> String {
    format!("{slug}-{member_hash}-{}", kind.dir())
}

/// The `[[bin]]` name for a class-C package.
///
/// Derived from the same scheme, which is what makes it safe to delete
/// `migrate.rs`'s `const TRANSFORM_BIN`. **A fixed constant and a derived name
/// cannot both be the answer**: the constant is how the CLI came to resolve the
/// transformer by a literal and could therefore run the wrong app's binary — or
/// the engine hop's — over a user's data dir at exit 0.
pub fn bin_name(slug: &str, member_hash: &str, kind: &PackageKind) -> String {
    package_name(slug, member_hash, kind)
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
/// The result is a valid C identifier: `-` becomes `_`, and the slug already
/// guarantees a leading ASCII letter.
pub fn symbol_prefix(slug: &str, member_hash: &str) -> String {
    format!("{}_{}_", slug.replace('-', "_"), member_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn slug_lowercases_and_collapses_separators() {
        assert_eq!(slug(Path::new("schema.forge")), "schema");
        assert_eq!(slug(Path::new("My_App.forge")), "my-app");
        assert_eq!(slug(Path::new("blog cms.forge")), "blog-cms");
        assert_eq!(slug(Path::new("a---b.forge")), "a-b");
        assert_eq!(slug(Path::new("apps/api/schema.forge")), "schema");
    }

    #[test]
    fn slug_trims_leading_and_trailing_separators() {
        assert_eq!(slug(Path::new("_leading.forge")), "leading");
        assert_eq!(slug(Path::new("trailing_.forge")), "trailing");
        assert_eq!(slug(Path::new("__both__.forge")), "both");
    }

    /// Cargo rejects a package name starting with a digit, and the rejection is
    /// project-wide rather than local to that package.
    #[test]
    fn slug_never_starts_with_a_digit() {
        assert_eq!(slug(Path::new("2024-orders.forge")), "app-2024-orders");
        assert_eq!(slug(Path::new("1.forge")), "app-1");
        // Not merely "does not start with a digit" — the fallback must keep the
        // information, because legibility is the slug's only job.
        assert!(slug(Path::new("2024-orders.forge")).contains("2024-orders"));
    }

    #[test]
    fn slug_falls_back_when_nothing_survives() {
        assert_eq!(slug(Path::new("___.forge")), SLUG_FALLBACK);
        assert_eq!(slug(&PathBuf::new()), SLUG_FALLBACK);
        // Non-ASCII is dropped, not transliterated.
        assert_eq!(slug(Path::new("日本語.forge")), SLUG_FALLBACK);
    }

    /// A dotfile has no extension as far as `file_stem` is concerned, so a
    /// schema literally named `.forge` stems to `.forge` and slugs to `forge`.
    /// Asserted rather than assumed — this reads like a fallback case and is
    /// not one.
    #[test]
    fn a_dotfile_schema_keeps_its_name() {
        assert_eq!(slug(Path::new(".forge")), "forge");
    }

    #[test]
    fn package_names_are_unique_per_kind_and_per_app() {
        let a = "aaaaaaaaaaaaaaaa";
        let b = "bbbbbbbbbbbbbbbb";

        // One app, two kinds — the transform/engine pair that ships colliding.
        assert_ne!(
            package_name("s", a, &PackageKind::Transform { from: 1, to: 2 }),
            package_name("s", a, &PackageKind::Engine { from: 1, to: 2 }),
        );
        // Two apps, same kind, same slug — separated by the hash alone.
        assert_ne!(
            package_name("schema", a, &PackageKind::Core),
            package_name("schema", b, &PackageKind::Core),
        );
        // One app, two ranges of the same kind.
        assert_ne!(
            package_name("s", a, &PackageKind::Transform { from: 1, to: 2 }),
            package_name("s", a, &PackageKind::Transform { from: 2, to: 3 }),
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
        for stem in ["schema.forge", "My_App.forge", "2024-orders.forge", "___.forge"] {
            let s = slug(Path::new(stem));
            let prefix = symbol_prefix(&s, "0123456789abcdef");
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
        assert_ne!(
            symbol_prefix("schema", "aaaaaaaaaaaaaaaa"),
            symbol_prefix("schema", "bbbbbbbbbbbbbbbb"),
        );
    }
}
