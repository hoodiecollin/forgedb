//! The ForgeDB build cache directory (#334, epic #332).
//!
//! `~/.forgedb/projects/<project>/` is a cargo workspace **ForgeDB owns**: a
//! virtual manifest, one member per app, one `Cargo.lock` and one `target/`
//! shared by every member.  Sharing the lockfile and target dir is the whole
//! point — it is what makes the substrate compile once per *project* rather
//! than once per *app*.
//!
//! ```text
//! ~/.forgedb/projects/<project>/
//!   Cargo.toml            # the workspace ROOT — virtual, written by us
//!   Cargo.lock            # one resolution shared by every app
//!   target/               # one target dir shared by every app
//!   apps/<hash>/          # one member per app
//! ```
//!
//! # What this module guarantees
//!
//! * **A member path is a pure function of its inputs.**  `member_dir` needs
//!   only a project id and the app's *project-relative* schema path; a lookup
//!   recomputes the path, so the cache needs no index of its own contents.
//! * **The root manifest is derived, never remembered.**  [`write_workspace_root`]
//!   takes the whole member set and rewrites the file; there is deliberately no
//!   API through which it can accumulate a stale entry.
//! * **Nothing here is an input.**  Deleting the tree and regenerating reproduces
//!   identical generated *source*.  It may reproduce a different dependency
//!   *resolution* — that is C1 as scoped by the design gate (#343 §4), and
//!   nothing may depend on the lockfile surviving.
//!
//! # Why the hash is hand-rolled
//!
//! The member hash must be stable **across Rust releases**: `cargo install
//! forgedb` builds the CLI with whatever toolchain the user has, and
//! `rust-toolchain.toml` does not reach them.  `DefaultHasher` is explicitly not
//! stable across releases, so persisting its output silently re-keys every
//! member on a rustup upgrade — which presents as "ForgeDB got slow" and is
//! nearly undiagnosable.  #366 is that exact mistake, already shipped elsewhere
//! in this repo.  FNV-1a is specified here and pinned by golden vectors in the
//! tests, so it cannot drift without a test failing.

use std::path::{Component, Path, PathBuf};

use crate::error::{CliError, Result};

/// Environment override for the ForgeDB home directory.  Relocates the **whole**
/// tree, which is what CI and the substrate-reclose check want in order to get a
/// genuinely cold cache.
pub const HOME_ENV: &str = "FORGEDB_HOME";

/// The workspace resolver the generated root pins.
///
/// This is **not** cosmetic.  A virtual manifest with no `resolver` key silently
/// defaults to resolver 1, which unifies features *more* aggressively than 2/3
/// (it unifies across build-, dev- and target-specific dependencies) — making the
/// cross-app feature coupling C11 exists to contain strictly worse.  Cargo also
/// warns on every invocation, in a directory the user never opened.
const WORKSPACE_RESOLVER: &str = "3";

/// `$FORGEDB_HOME`, else `~/.forgedb`.
///
/// Every path in this module is derived from here, and every caller must come
/// through it: a single code path that reaches `home::home_dir()` directly would
/// pass the test suite (which sets `FORGEDB_HOME` to a tempdir) while writing to
/// the developer's real home.
pub fn forgedb_home() -> Result<PathBuf> {
    // An empty override is a misconfiguration, not an instruction to resolve the
    // cache against a relative empty path — treat it as unset.
    if let Some(explicit) = std::env::var_os(HOME_ENV)
        && !explicit.is_empty()
    {
        return Ok(PathBuf::from(explicit));
    }

    home::home_dir()
        .map(|h| h.join(".forgedb"))
        .ok_or_else(|| {
            CliError::Config(format!(
                "Cannot determine a home directory for the ForgeDB build cache. \
                 Set {} to an explicit path.",
                HOME_ENV
            ))
        })
}

/// `<home>/ledger` — the project-id claim ledger (#333).
///
/// It lives in the cache dir because it is a **detector**, never the record of a
/// resolution: a resolved collision is written into the colliding project's own
/// `forgedb.toml`.  That division is what keeps it safe under C1 — the ledger is
/// a rebuildable cache of currently-claimed ids, so GC may delete it freely, and
/// deleting it cannot resurrect a collision.
pub fn ledger_root() -> Result<PathBuf> {
    Ok(forgedb_home()?.join("ledger"))
}

/// `<home>/projects` — the parent of every per-project workspace.
pub fn projects_root() -> Result<PathBuf> {
    Ok(forgedb_home()?.join("projects"))
}

/// The workspace root for one project.
pub fn project_dir(project_id: &str) -> Result<PathBuf> {
    if project_id.is_empty() {
        return Err(CliError::Config(
            "Empty project id: the build cache cannot be keyed by an unnamed project".to_string(),
        ));
    }
    Ok(projects_root()?.join(project_id))
}

/// Normalize a project-relative schema path into the exact string that gets
/// hashed.
///
/// The normalization is part of the contract, not an implementation detail —
/// the golden vectors pin *this*, not just the digest:
///
/// * separators become `/`, so a path spelled with either separator agrees;
/// * `.` components are dropped, so `./apps/api` == `apps/api`;
/// * a leading root or prefix component is dropped — the input is meant to be
///   project-relative, and tolerating an absolute path silently would key the
///   member by a machine-specific string.
///
/// **Case is deliberately preserved, not folded.**  macOS is case-insensitive but
/// case-preserving, so `Apps/…` and `apps/…` are one file there and two files on
/// Linux.  Folding would make Linux wrong; not folding costs a duplicate member
/// directory on macOS, which is a wasted rebuild rather than a wrong answer.
fn normalize_for_hash(rel_schema: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for component in rel_schema.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            // A project-relative path has no business carrying these; dropping
            // them keeps the hash a function of the path *within* the project.
            Component::RootDir | Component::Prefix(_) => {}
            Component::ParentDir => parts.push("..".to_string()),
        }
    }
    parts.join("/")
}

/// FNV-1a (64-bit) over the normalized path, rendered as 16 lowercase hex digits.
///
/// Specified here rather than taken from a dependency so that it is pinned by
/// our own golden vectors and cannot move underneath a released cache.  The
/// threat model is an accidental collision between two schema paths in one
/// project, not an adversary choosing them.
pub fn member_hash(rel_schema: &Path) -> String {
    path_hash(&normalize_for_hash(rel_schema))
}

/// FNV-1a (64-bit) over a string, rendered as 16 lowercase hex digits.
///
/// The one hash in the CLI, shared by the member hash above and by #333's
/// project-path fallback id, so there is a single place a drift would show up
/// and a single set of golden vectors pinning it.
pub fn path_hash(input: &str) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{:016x}", hash)
}

/// The member directory for one app: `<project>/apps/<hash>`.
///
/// A pure function of its inputs — the same pair yields the same path on another
/// machine, in CI, and after the cache has been wiped and rebuilt.
pub fn member_dir(project_id: &str, rel_schema: &Path) -> Result<PathBuf> {
    Ok(project_dir(project_id)?
        .join("apps")
        .join(member_hash(rel_schema)))
}

/// Render the virtual workspace manifest for a member set.
///
/// Split out from [`write_workspace_root`] so the exact bytes can be asserted
/// without touching a filesystem.
fn render_workspace_root(project: &Path, members: &[PathBuf]) -> Result<String> {
    let mut entries: Vec<String> = Vec::new();

    for member in members {
        let relative = member.strip_prefix(project).map_err(|_| {
            CliError::Config(format!(
                "Workspace member {} is not inside the project directory {}",
                member.display(),
                project.display()
            ))
        })?;

        let mut rendered = String::new();
        for (i, component) in relative.components().enumerate() {
            if let Component::Normal(part) = component {
                if i > 0 {
                    rendered.push('/');
                }
                rendered.push_str(&part.to_string_lossy());
            }
        }
        entries.push(rendered);
    }

    entries.sort();
    entries.dedup();

    let members_block = if entries.is_empty() {
        "members = []\n".to_string()
    } else {
        let listed = entries
            .iter()
            .map(|e| format!("    \"{}\",\n", e))
            .collect::<String>();
        format!("members = [\n{}]\n", listed)
    };

    // C2: [workspace] and members only — no [package], no lib target, no shared
    // crate.  Nothing here is ForgeDB-authored Rust source.
    Ok(format!(
        "# Generated by ForgeDB. Do not edit — this file is rewritten in full on\n\
         # every generate, as a pure function of the apps in this project.\n\
         #\n\
         # This is a build cache. Deleting this whole directory is safe: it holds\n\
         # no input, only derived state.\n\
         [workspace]\n\
         resolver = \"{}\"\n\
         {}",
        WORKSPACE_RESOLVER, members_block
    ))
}

/// Write the workspace root manifest for a project.
///
/// Takes the **whole** member set and rewrites the file.  There is deliberately
/// no "add a member" entry point: a manifest that is patched in place
/// accumulates entries for apps whose schemas were deleted, and that state
/// survives a regenerate — which is the one way to break C1 that nobody sees.
pub fn write_workspace_root(project: &Path, members: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(project)?;
    let manifest = render_workspace_root(project, members)?;
    std::fs::write(project.join("Cargo.toml"), manifest)?;
    Ok(())
}

/// Member directories present on disk that the live set does not name.
///
/// Renaming or moving a schema file changes its hash and orphans the old member
/// directory — the direct consequence of the hash being deterministic, and the
/// reason GC is load-bearing rather than optional.
///
/// **An empty live set is refused rather than treated as "everything is an
/// orphan".** The dangerous case is a GC run reached from an error path that
/// produced no members, which would otherwise delete every app in the project.
pub fn orphans(project: &Path, live: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if live.is_empty() {
        return Err(CliError::Config(format!(
            "Refusing to scan {} for orphans with an empty live member set — \
             this would report every member as garbage.",
            project.display()
        )));
    }

    let apps_dir = project.join("apps");
    if !apps_dir.exists() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(&apps_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if !live.iter().any(|l| l == &path) {
            found.push(path);
        }
    }

    found.sort();
    Ok(found)
}

/// C4: refuse a data root that resolves inside the build cache.
///
/// Nobody writes "put the database in the build cache"; they run something whose
/// working directory is the cache dir, and `TenantConfig::root()`'s **relative**
/// `"data"` default (`config.rs:41-43`) does it for them.  So this checks the
/// *resolved* path — checking the configured value catches nothing, because the
/// dangerous case is the one where nothing was configured at all.
///
/// A build cache that also holds data is an installation, and an installation is
/// where a runtime lives.
pub fn assert_not_in_cache(data_root: &Path) -> Result<()> {
    let home = forgedb_home()?;

    // Compare against the closest existing ancestor: the data root itself
    // usually does not exist yet, so it cannot be canonicalized directly.
    let absolute = if data_root.is_absolute() {
        data_root.to_path_buf()
    } else {
        std::env::current_dir()?.join(data_root)
    };
    let resolved = closest_real_ancestor(&absolute);
    let home_resolved = closest_real_ancestor(&home);

    if resolved.starts_with(&home_resolved) {
        return Err(CliError::Config(format!(
            "Data root {} resolves inside the ForgeDB build cache ({}).\n\
             The build cache is derived state and may be deleted at any time, so it \
             must never hold database data, tenant dirs, WAL, or backups.\n\
             Set an explicit `[tenant] root` outside the cache, or run from a \
             different working directory.",
            data_root.display(),
            home_resolved.display()
        )));
    }

    Ok(())
}

/// Canonicalize as much of a path as exists, keeping the rest verbatim.
///
/// Needed because both sides of the containment check may point at directories
/// that have not been created yet, while symlinked homes (`/tmp` → `/private/tmp`
/// on macOS) make a purely lexical comparison wrong.
fn closest_real_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if let Ok(canonical) = current.canonicalize() {
            let remainder = path.strip_prefix(current).unwrap_or(Path::new(""));
            return canonical.join(remainder);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return path.to_path_buf(),
        }
    }
}
