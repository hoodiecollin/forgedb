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
//!   apps/<hash>/          # one CONTAINER per app — no manifest of its own
//!     schema-path         #   which schema this container belongs to
//!     app-name            #   its legible derived identity (foo_services_blog)
//!     core/               #   the workspace MEMBERS are one level deeper
//!     server/
//!     ffi/  napi/  pyo3/  wasm/
//!     transform-<a>-<b>/  engine-<a>-<b>/
//! ```
//!
//! **`apps/<hash>/` is a container, not a member** (#335 §1).  It holds the
//! marker and the per-kind package directories and has no `Cargo.toml`, because
//! a `members` entry naming a directory with no manifest is **project-wide
//! fatal** — `cargo metadata`, `build`, and even `build -p <some other member>`
//! all exit 101.  That is what the shape shipped by #334 did, which is why
//! nothing has ever built in this cache.
//!
//! # What this module guarantees
//!
//! * **A container path is a pure function of its inputs.**  `member_dir` needs
//!   only a project id and the app's *project-relative* schema path; a lookup
//!   recomputes the path, so the cache needs no index of its own contents.
//! * **The root manifest is derived, never remembered.**  [`write_workspace_root`]
//!   takes the whole member set and rewrites the file; there is deliberately no
//!   API through which it can accumulate a stale entry.  [`sync_root`] derives
//!   that set by scanning, so no command declares it and nothing records it.
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
use crate::naming::PackageKind;

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

    // `default-members` is derived by FILTERING the vector rendered just above —
    // never by a second scan.  Two lists from two derivations is how a skew
    // happens, and the skew here is not survivable: a `default-members` that is
    // not a subset of `members` is **project-wide fatal**, breaking `build`,
    // `build -p <a valid member>` *and* `metadata` (measured, cargo 1.96.1).
    // One list and a filter makes that unrepresentable.
    let default_entries: Vec<&String> = entries
        .iter()
        .filter(|e| {
            e.rsplit('/')
                .next()
                .and_then(PackageKind::from_dir)
                .is_some_and(|k| k.is_default_member())
        })
        .collect();

    // Emit the key only when the filtered set is a non-empty PROPER subset;
    // otherwise omit it entirely, which restores build-all-members.  Both edges
    // are correct that way: equal sets are exactly equivalent, and an empty
    // filtered set — reachable for a REST-only app with a migration lineage,
    // whose only packages are `transform-*`/`engine-*` — leaves host bins that
    // build fine.  Writing `default-members = []` instead produces cargo's
    // misleading "the workspace has no members".
    let default_block =
        if default_entries.is_empty() || default_entries.len() == entries.len() {
            String::new()
        } else {
            let listed = default_entries
                .iter()
                .map(|e| format!("    \"{}\",\n", e))
                .collect::<String>();
            format!("default-members = [\n{}]\n", listed)
        };

    // C2: [workspace], members and default-members only — no [package], no lib
    // target, no shared crate.  No HAND-AUTHORED Rust source lives here; the
    // per-app generated source under `apps/` is the point of the epic.
    //
    // There is deliberately NO [profile.*] table.  Cargo's config.toml overrides
    // a manifest profile (measured), so keys written here would read as applied
    // while a machine-wide `$CARGO_HOME/config.toml` silently beat them — which
    // is exactly the decoration-that-reads-as-applied this design deletes.  The
    // profile floor is enforced by the build driver via `--config`.
    Ok(format!(
        "# Generated by ForgeDB. Do not edit — this file is rewritten in full on\n\
         # every generate, as a pure function of the apps in this project.\n\
         #\n\
         # This is a build cache. Deleting this whole directory is safe: it holds\n\
         # no input, only derived state.\n\
         #\n\
         # The release profile floor (panic=unwind, and opt-level for wasm) is\n\
         # applied by `forgedb build` on the cargo invocation, NOT by this file.\n\
         # A bare `cargo build` run here does not get it.\n\
         [workspace]\n\
         resolver = \"{}\"\n\
         {}{}",
        WORKSPACE_RESOLVER, members_block, default_block
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

/// Records, inside a member directory, which schema it was generated from.
///
/// The member *path* is deliberately keyed by a project-RELATIVE path so it
/// resolves the same on another machine; liveness needs the absolute one, which
/// is a machine-local fact and therefore belongs in the cache rather than in the
/// key.  Without it a member directory is an opaque hash that cannot be checked
/// against anything — the epic's "stale members are dropped by a `stat` per
/// member rather than found by a scan" has nothing to `stat`.
const MEMBER_SCHEMA_FILE: &str = "schema-path";

/// Note which schema a member directory belongs to.
pub fn record_member(member: &Path, schema: &Path) -> Result<()> {
    std::fs::create_dir_all(member)?;
    std::fs::write(member.join(MEMBER_SCHEMA_FILE), schema.to_string_lossy().as_bytes())?;
    Ok(())
}

/// The schema a member directory belongs to, if it recorded one.
pub fn member_schema(member: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(member.join(MEMBER_SCHEMA_FILE)).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// Records the member's derived app name — the legible identity that replaced
/// hash-based disambiguation.
///
/// It is **persisted rather than re-derived** because deriving it needs the
/// project's whole app set (see [`crate::naming::app_name`]), and the three
/// consumers — `generate`, `build`, `migrate` — hold only this container path.
/// Re-deriving it in each would be three chances to disagree, and a disagreement
/// between the emitter of a C symbol and the emitter of its header is a link
/// error at the far end of the build.
const MEMBER_APP_NAME_FILE: &str = "app-name";

/// Note the derived app name for a member directory.
pub fn record_app_name(member: &Path, app_name: &str) -> Result<()> {
    std::fs::create_dir_all(member)?;
    std::fs::write(member.join(MEMBER_APP_NAME_FILE), app_name.as_bytes())?;
    Ok(())
}

/// The derived app name a member directory recorded, if any.
///
/// `None` means a member written before this marker existed, or an unreadable
/// one. Callers fall back to the container's own directory name, which is the
/// pre-existing hash — legible names are an improvement, not a load-bearing
/// invariant, and a cache from an older CLI must still build.
pub fn member_app_name(member: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(member.join(MEMBER_APP_NAME_FILE)).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Every member directory in a project whose schema still exists, plus `keep`.
///
/// This is how the member set **accretes**: generating one app does not know
/// about its siblings, so the set is rebuilt from what is on disk each time
/// rather than remembered.  A member whose schema is gone is simply not returned,
/// which is what drops a renamed or deleted app without a subtree scan of the
/// user's repo.
///
/// A member that recorded nothing is **kept**, not dropped: an unreadable marker
/// means "written by a version that did not record one", and guessing that such a
/// directory is garbage is the one mistake here that destroys work.
pub fn live_members(project: &Path, keep: &Path) -> Result<Vec<PathBuf>> {
    let mut live = vec![keep.to_path_buf()];

    let apps_dir = project.join("apps");
    if apps_dir.exists() {
        for entry in std::fs::read_dir(&apps_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            if path == keep {
                continue;
            }
            match member_schema(&path) {
                Some(schema) if !schema.exists() => continue,
                _ => live.push(path),
            }
        }
    }

    live.sort();
    live.dedup();
    Ok(live)
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
    if let Some(home_resolved) = home_containing(data_root)? {
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

/// The **resolved** `$FORGEDB_HOME` when `path` lands inside it, else `None`.
///
/// The half of [`assert_not_in_cache`] that is a question rather than a refusal,
/// extracted so #338's in-tree placement guard can ask it and write its own
/// diagnostic. Two callers, two messages, one resolution — and the resolution is
/// the part with the subtleties in it.
///
/// Two of them. The path is made **absolute** first, because a relative
/// configured value is exactly the dangerous case (nobody writes "put it in the
/// build cache"; they run somewhere whose CWD is inside it). And the comparison
/// is against the closest *existing* ancestor of each side, because neither the
/// candidate nor the home necessarily exists yet — and `~/.forgedb` behind a
/// symlink would otherwise compare unequal to the same directory reached
/// literally.
pub fn home_containing(path: &Path) -> Result<Option<PathBuf>> {
    let home = forgedb_home()?;

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let resolved = closest_real_ancestor(&absolute);
    let home_resolved = closest_real_ancestor(&home);

    Ok(resolved
        .starts_with(&home_resolved)
        .then_some(home_resolved))
}

/// One app's container directory, and the project root above it.
#[derive(Debug)]
pub struct Reserved {
    /// The ForgeDB-owned workspace root for this project.
    pub project: PathBuf,
    /// This app's container beneath it.  Holds the `schema-path` marker and the
    /// per-kind package directories — and **no manifest of its own**.
    pub container: PathBuf,
    /// The app's legible derived identity, e.g. `foo_services_blog`.
    ///
    /// Every package name, `[[bin]]` name and exported C symbol for this app is
    /// built from it, so it is computed once here and persisted in the container
    /// rather than re-derived by each consumer.
    pub app_name: String,
}

/// Ensure an app's container exists and record what it belongs to.
///
/// **Runs BEFORE generation writes anything**, because emission needs the path.
/// This is the half of the old `place()` that is a pure function of its inputs,
/// exactly as this module's doc claims of `member_dir`.
///
/// It deliberately does **not** touch the workspace root.  Rendering a
/// scan-derived root at this point would list the packages of the *previous*
/// run, so an app's very first `generate` would write a root without its own
/// packages and `cargo build -p <its core>` would fail with `did not match any
/// packages` — a failure invisible on a warm cache and reachable only on the
/// first generate for an app.  [`sync_root`] is the after half.
pub fn reserve(
    project_id: &str,
    project_root: &Path,
    schema: &Path,
    mode: crate::naming::SymbolNaming,
) -> Result<Reserved> {
    let project = project_dir(project_id)?;
    let schema = absolutize(schema);

    // The project root always comes from the schema's own ancestor chain, so it
    // is an ancestor of the schema — but a caller that resolved them separately
    // could hand over a pair that is not, and hashing an absolute path there is
    // still deterministic, merely not portable. Better than silently colliding on
    // a bare file name.
    let relative = schema
        .strip_prefix(project_root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| schema.clone());

    let member = member_dir(project_id, &relative)?;
    record_member(&member, &schema)?;

    // The app's legible identity, computed HERE because this is the one place
    // that holds all three inputs at once: the project id, the project-relative
    // schema path, and (via the project root) its siblings.
    let siblings = crate::project::discover_schemas(project_root);
    let app_name = crate::naming::app_name(project_id, &relative, &siblings, mode);
    record_app_name(&member, &app_name)?;

    Ok(Reserved {
        project,
        container: member,
        app_name,
    })
}

/// What one [`sync_root`] pass found and wrote.
#[derive(Debug)]
pub struct Synced {
    /// Every package directory now listed in the root manifest.
    pub members: Vec<PathBuf>,
    /// Containers whose schema no longer exists.
    pub orphans: Vec<PathBuf>,
    /// C9 fired: the CLI version changed, so `Cargo.lock` was deleted.  The
    /// caller reports this — it is a thing that happened to the user's project,
    /// in a directory they never open.
    pub lock_dropped: bool,
}

/// Records which CLI version last wrote a project's workspace root.
///
/// A plain file beside the manifest rather than a key inside it, so that reading
/// it costs no TOML parse and a corrupt value degrades to "mismatch" — which is
/// the safe direction, since the remedy is dropping a lockfile that is
/// reproducible by definition.
const CLI_VERSION_FILE: &str = "cli-version";

/// Drop `Cargo.lock` when the CLI version that last wrote this project differs
/// from the running one (C9).
///
/// # Why this is unconditional
///
/// C9's wording is unconditional, and the case the epic names — an upgraded CLI
/// regenerating an *unchanged* target set — creates, deletes and prunes nothing.
/// Gating the check on a package-set change would skip exactly it.  So this runs
/// from [`sync_root`], which runs on every invocation that touches the project.
///
/// # Why dropping the lock is the right remedy, and why it is safe
///
/// It is the half a manifest rewrite alone cannot do.  A rewritten pin
/// re-resolves; a **newly published patch under an unchanged pin** does not.
/// C1 explicitly scopes this as permitted — the cache "may reproduce a different
/// dependency RESOLUTION; nothing may depend on the lockfile surviving".
///
/// #335 is the first change that produces a real, long-lived `Cargo.lock` per
/// project, which is what the epic means by turning #290 "from rare into
/// routine".  Before it, the cache workspace had no members and recorded no
/// meaningful resolution at all.
fn enforce_cli_version(project: &Path) -> Result<bool> {
    let running = env!("CARGO_PKG_VERSION");
    let marker = project.join(CLI_VERSION_FILE);

    let recorded = std::fs::read_to_string(&marker).ok();
    let matches = recorded.as_deref().map(str::trim) == Some(running);
    if matches {
        return Ok(false);
    }

    // An absent marker is a mismatch, not a fresh start: the lockfile may have
    // been written by a version that did not record one.
    let lock = project.join("Cargo.lock");
    let dropped = lock.is_file();
    if dropped {
        std::fs::remove_file(&lock)?;
    }

    std::fs::create_dir_all(project)?;
    std::fs::write(&marker, running.as_bytes())?;
    Ok(dropped)
}

/// Rebuild the workspace root from what is on disk.
///
/// Scan the live containers, expand each into the package directories beneath
/// it, and rewrite the root manifest around the result.  **The filesystem is the
/// single source of truth for which packages exist**: no command declares the
/// member set, nothing records it, and no invocation needs to know a sibling
/// app's configuration.
///
/// Runs **after** every write that creates a package and **before** every
/// deletion — see the ordering rule on [`prunable`].
///
/// # Why a scan, when the epic rejects scanning
///
/// The epic rejects a downward walk of *the user's tree*, "whose cost scales
/// with the user's repo rather than their ForgeDB usage".  This does not touch
/// the user's repo: [`live_members`] already does one `read_dir` of the cache
/// plus a marker read per container, and this adds one `read_dir` per live
/// container and a `stat` per subdirectory.  Same order, same axis.
///
/// The alternatives fail on mechanics rather than taste.  A **declared** set
/// cannot work — generating app A cannot know app B's declared targets, so A's
/// invocation would render B's members wrong.  A **recorded** set is a second
/// record that drifts, which is what [`write_workspace_root`]'s own doc refuses
/// to have.  A **glob** (`members = ["apps/*/*"]`) correctly skips the plain
/// `schema-path` file, but a single stray *directory* under it is a hard error
/// that takes the whole project down.
pub fn sync_root(project: &Path, keep: &Path) -> Result<Synced> {
    sync_root_excluding(project, keep, &[])
}

/// [`sync_root`], minus a set of package directories that are about to be
/// deleted — the **de-list** half of a prune.
///
/// Excluding here rather than deleting first is #335 §3 rule 1: the root must
/// stop naming a package *before* that package's directory is removed.  A root
/// naming a member that does not exist is **project-wide fatal** — `cargo
/// metadata`/`build` exit 101 for *every* app in the project, not just the one
/// pruned — while a directory present but unlisted is inert.  So the safe
/// intermediate state is the one this function alone produces, and it is the
/// state a kill between the two steps leaves behind.
///
/// Public because it is safe by construction: it removes nothing.  Deletion is
/// reachable only through [`prune`], which calls this first.
pub fn sync_root_excluding(project: &Path, keep: &Path, excluded: &[PathBuf]) -> Result<Synced> {
    let live = live_members(project, keep)?;

    let mut members = Vec::new();
    for container in &live {
        members.extend(packages_in(container)?);
    }
    members.retain(|m| !excluded.iter().any(|e| same_dir(e, m)));
    members.sort();
    members.dedup();

    write_workspace_root(project, &members)?;
    let lock_dropped = enforce_cli_version(project)?;
    let orphans = orphans(project, &live)?;

    Ok(Synced {
        members,
        orphans,
        lock_dropped,
    })
}

/// What one [`prune`] pass de-listed and removed.
#[derive(Debug)]
pub struct Pruned {
    /// The root rewrite that de-listed the removed packages — done FIRST.
    pub synced: Synced,
    /// The package directories actually removed from disk.
    pub removed: Vec<PathBuf>,
}

/// De-list, then delete — **the only function that removes a package
/// directory** (#335 §3 rule 1).
///
/// The order is the whole content of this function, and it is not
/// reversible by a caller: the root is rewritten without `doomed` first, and
/// only then is anything unlinked.  Inverted, a kill (or a failing `rmdir`, or
/// a full disk) in between leaves the root naming a member that does not
/// exist, which breaks every app in the project rather than the one being
/// pruned.
///
/// `doomed` must come from [`prunable`], which decides *what* may be removed;
/// this decides only *in what order* — plus one refusal [`prunable`] cannot
/// express, because it is about the caller rather than the container: a path
/// outside `keep` is never deleted, so a wrong container or a wrong declared
/// set can damage at most the app it was handed.
///
/// The de-list matches on [`same_dir`] rather than on `Path` equality: a doomed
/// path spelled with a `..` component, or relatively, or through a symlink,
/// must still be removed from the member list before it is unlinked.
pub fn prune(project: &Path, keep: &Path, doomed: &[PathBuf]) -> Result<Pruned> {
    prune_with(project, keep, doomed, &mut |dir| std::fs::remove_dir_all(dir))
}

/// [`prune`] with the removal itself injected, so a test can observe the state
/// at the instant between the de-list and the delete — the state a kill leaves.
///
/// An end-state assertion cannot tell the two orders apart (both end with the
/// directory gone and unlisted); only the intermediate state can, so it has to
/// be reachable.
fn prune_with(
    project: &Path,
    keep: &Path,
    doomed: &[PathBuf],
    remove: &mut dyn FnMut(&Path) -> std::io::Result<()>,
) -> Result<Pruned> {
    // Nothing outside the container being acted on is ever deletable (#335 §3:
    // "only the container being acted on is judged"). A sibling app's package
    // reaching this list would be a bug in the CALLER — a wrong `declared` set,
    // the wrong container — and the damage is another app's build, so it is
    // refused here rather than trusted.
    for dir in doomed {
        let inside = closest_real_ancestor(dir).starts_with(closest_real_ancestor(keep));
        if !inside {
            return Err(CliError::Config(format!(
                "Refusing to prune {}: it is outside the app container {} this prune \
                 was given. A prune never reaches beyond one app.",
                dir.display(),
                keep.display()
            )));
        }
    }

    // 1. De-list.
    let synced = sync_root_excluding(project, keep, doomed)?;

    // 2. Delete.
    let mut removed = Vec::new();
    for dir in doomed {
        match remove(dir) {
            Ok(()) => removed.push(dir.clone()),
            // Already gone: the de-list is what mattered and it happened.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(Pruned { synced, removed })
}

/// Package directories in a container that the given command may remove.
///
/// **Returns them; does NOT delete.** That asymmetry is the API shape enforcing
/// #335 §3 rule 1: the caller must rewrite the workspace root against the
/// post-prune set *before* removing anything. Inverted, the root names a member
/// that does not exist, which is **project-wide fatal for every app in the
/// project** rather than just the one being pruned — while a directory that is
/// present but unlisted is inert (measured). A function that cannot delete
/// cannot get that order wrong.
///
/// Three further rules, all from §3:
///
/// * **Only the container being acted on is judged.** Sibling apps are never
///   touched, mirroring `orphans()`, which reports rather than deletes.
/// * **Pruned against the DECLARED set, never the invocation's selected set.**
///   `declared` is `[generate].targets`, now required and explicit, so it is
///   always stated. Pruning against *selected* would make `forgedb generate
///   rust` delete `napi/`.
/// * **Each command prunes only the kinds it owns.** Without this the first
///   `generate` after a `migrate build` deletes the transformer.
///
/// A directory whose name ForgeDB does not emit is never returned — fail toward
/// keeping, because a wrongly-kept package costs nothing and a wrongly-deleted
/// one costs a regeneration.
pub fn prunable(
    container: &Path,
    declared_kinds: &[PackageKind],
    owner: crate::naming::PruneOwner,
) -> Result<Vec<PathBuf>> {
    let mut doomed = Vec::new();
    let entries = match std::fs::read_dir(container) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(doomed),
        Err(e) => return Err(e.into()),
    };

    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(kind) = PackageKind::from_dir(name) else {
            continue;
        };
        if kind.owner() != owner {
            continue;
        }
        if declared_kinds.contains(&kind) {
            continue;
        }
        doomed.push(entry.path());
    }

    doomed.sort();
    Ok(doomed)
}

/// The package directories inside one container.
///
/// A subdirectory counts when it holds a `Cargo.toml` **and** its name is one
/// ForgeDB emits ([`PackageKind::from_dir`]).
///
/// # Why the kind filter, when the design says "subdirectories that hold a
/// `Cargo.toml`"
///
/// The asymmetry runs the other way for *membership* than it does for pruning.
/// A package on disk that `members` does not name is **inert** — root `metadata`
/// and `build` both exit 0 and it is simply never built (measured).  But a
/// member ForgeDB lists and does not own is a manifest ForgeDB has made itself
/// responsible for, and a broken one is **project-wide fatal** for every app in
/// the project.  So an unrecognised directory is left unlisted rather than
/// admitted.  In practice the two rules coincide, because every directory in a
/// container is one ForgeDB wrote.
///
/// The container itself is never a member: it holds no manifest, and a member
/// with no manifest is the project-wide-fatal failure mode this whole change
/// exists to fix.  The `schema-path` marker is a plain file and is invisible
/// here.
fn packages_in(container: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(container) {
        Ok(entries) => entries,
        // A container that vanished between the scan and here is not an error:
        // it simply contributes nothing.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(e.into()),
    };

    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if PackageKind::from_dir(name).is_none() {
            continue;
        }
        if path.join("Cargo.toml").is_file() {
            found.push(path);
        }
    }

    found.sort();
    Ok(found)
}

/// Whether two paths name the same directory, independent of spelling.
///
/// `Path` equality is component-based, so it already sees through a trailing
/// separator and a `.` component — but not through `..`, a relative spelling, or
/// a symlink. The de-list half of a prune has to see through all of them: a
/// doomed path that fails to match its member entry would be deleted while the
/// root still named it, which is the project-wide-fatal state the ordering rule
/// exists to prevent.
fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || closest_real_ancestor(a) == closest_real_ancestor(b)
}

fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(p),
        Err(_) => p.to_path_buf(),
    }
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

#[cfg(test)]
mod prune_order_tests {
    //! **Plan #347 scenario 12 (★) — a prune de-lists before it deletes.**
    //!
    //! These live here rather than in `tests/cache_dir_test.rs` because the
    //! thing under test is an *order*, and an order is only observable from
    //! inside: both orders end with the directory gone and unlisted, so no
    //! end-state assertion can tell them apart. [`prune_with`] is the seam —
    //! the removal is injected, so a test can stand exactly where a `kill -9`
    //! would and read what is on disk at that instant.
    //!
    //! What the interrupted state must be: the root manifest already excludes
    //! the doomed package, and the doomed directory is still there. The reverse
    //! — directory gone, root still naming it — is **project-wide fatal**:
    //! `cargo metadata`/`build` exit 101 for every app in the project.

    use super::*;
    use crate::naming::PruneOwner;

    fn stub_package(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
    }

    /// A project with one app whose container holds `core` and `napi`, plus a
    /// sibling app holding `core` — so the "every app in the project" half of
    /// the scenario has a second app to be true of.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("projects/p");
        let container = project.join("apps/aaaaaaaaaaaaaaaa");
        let sibling = project.join("apps/bbbbbbbbbbbbbbbb");

        stub_package(&container.join("core"), "a-core");
        stub_package(&container.join("napi"), "a-napi");
        stub_package(&sibling.join("core"), "b-core");
        // No `schema-path` marker: a container that recorded nothing is KEPT,
        // so the sibling stays live without a schema file on disk.

        // The root as an ordinary prior invocation left it — naming all three
        // packages. Without this the manifest does not exist at the moment of
        // the kill, and the scenario-12 assertion below would panic on a missing
        // file instead of evaluating: red, but for the wrong reason.
        sync_root(&project, &container).unwrap();
        assert!(
            members_in_manifest(&project).contains("/napi"),
            "the fixture starts with the doomed package LISTED"
        );

        (tmp, project, container)
    }

    fn members_in_manifest(project: &Path) -> String {
        std::fs::read_to_string(project.join("Cargo.toml")).unwrap()
    }

    /// The mandatory one: at the instant of the kill, the root must already
    /// have stopped naming the package whose directory is still on disk.
    #[test]
    fn scenario_12_the_root_is_rewritten_before_anything_is_unlinked() {
        let (_tmp, project, container) = fixture();
        let doomed = prunable(&container, &[PackageKind::Core], PruneOwner::GenerateBuild).unwrap();
        assert_eq!(doomed, vec![container.join("napi")], "napi is the doomed one");

        let mut observed = 0;
        let killed = prune_with(&project, &container, &doomed, &mut |dir| {
            observed += 1;

            // Stand where the kill would land.
            let manifest = members_in_manifest(&project);
            assert!(
                !manifest.contains("/napi"),
                "the root still names the package about to be deleted — every app \
                 in this project is about to become unbuildable:\n{manifest}"
            );
            assert!(manifest.contains("/core"), "the survivors are still listed:\n{manifest}");
            assert!(dir.is_dir(), "the delete has not happened yet");

            Err(std::io::Error::other("killed between the de-list and the delete"))
        });

        assert_eq!(observed, 1, "the removal seam ran exactly once");
        assert!(killed.is_err(), "the injected failure propagates");

        // And the state the interrupted process left behind is the SAFE one:
        // an unlisted directory is inert, so every app still builds.
        let manifest = members_in_manifest(&project);
        assert!(!manifest.contains("/napi"), "{manifest}");
        assert!(manifest.contains("apps/aaaaaaaaaaaaaaaa/core"), "{manifest}");
        assert!(manifest.contains("apps/bbbbbbbbbbbbbbbb/core"), "{manifest}");
        assert!(
            container.join("napi/Cargo.toml").is_file(),
            "the directory is still there — that is what makes this the interrupted state"
        );
    }

    /// The completed prune: same root, and the directory is gone.
    #[test]
    fn scenario_12_a_completed_prune_removes_the_directory_and_the_member() {
        let (_tmp, project, container) = fixture();
        let doomed = prunable(&container, &[PackageKind::Core], PruneOwner::GenerateBuild).unwrap();

        let pruned = prune(&project, &container, &doomed).unwrap();

        assert_eq!(pruned.removed, doomed);
        assert!(!container.join("napi").exists());
        let manifest = members_in_manifest(&project);
        assert!(!manifest.contains("/napi"), "{manifest}");
        assert!(manifest.contains("apps/aaaaaaaaaaaaaaaa/core"), "{manifest}");
        assert!(manifest.contains("apps/bbbbbbbbbbbbbbbb/core"), "{manifest}");
    }

    /// A doomed path spelled differently from its member entry is still
    /// de-listed before it is unlinked.
    ///
    /// `Path` equality is component-based, so it already sees through a `.`
    /// component and a trailing separator (checked: `a/napi/.` compares equal to
    /// `a/napi`), but NOT through `..`, a relative spelling, or a symlink. A
    /// doomed path that failed to match its member entry would be deleted while
    /// the root still named it — the project-wide-fatal state — and it would
    /// happen silently, on a caller that looked correct.
    #[test]
    fn a_differently_spelled_doomed_path_is_still_de_listed_first() {
        let (_tmp, project, container) = fixture();

        // The same directory on disk, spelled so that `Path` equality misses it.
        let hash = container.file_name().unwrap().to_owned();
        let mistyped = vec![container.join("..").join(hash).join("napi")];
        assert!(mistyped[0].is_dir(), "the mis-spelling names a real directory");
        assert_ne!(mistyped[0], container.join("napi"), "…and is not Path-equal to it");

        let pruned = prune(&project, &container, &mistyped).unwrap();

        assert_eq!(pruned.removed, mistyped, "it was deleted");
        assert!(!container.join("napi").exists());
        let manifest = members_in_manifest(&project);
        assert!(
            !manifest.contains("/napi"),
            "the root still names a directory that is now GONE — every app in this \
             project is unbuildable:\n{manifest}"
        );
    }

    /// A prune never reaches outside the container it was given (#335 §3), and
    /// the refusal is here rather than in the caller because the damage is
    /// another app's build.
    #[test]
    fn a_doomed_path_outside_the_container_is_refused() {
        let (_tmp, project, container) = fixture();
        let sibling = project.join("apps/bbbbbbbbbbbbbbbb/core");

        let mut removals = 0;
        let err = prune_with(&project, &container, std::slice::from_ref(&sibling), &mut |_| {
            removals += 1;
            Ok(())
        })
        .expect_err("must refuse");

        assert_eq!(removals, 0, "nothing was deleted");
        assert!(err.to_string().contains("outside the app container"), "{err}");
        assert!(sibling.join("Cargo.toml").is_file(), "the sibling is untouched");
        let manifest = members_in_manifest(&project);
        assert!(
            manifest.contains("apps/bbbbbbbbbbbbbbbb/core"),
            "and is still a member:\n{manifest}"
        );
    }

    /// An empty prune is a plain [`sync_root`] — the path every ordinary
    /// invocation takes.
    #[test]
    fn an_empty_prune_deletes_nothing_and_still_rewrites_the_root() {
        let (_tmp, project, container) = fixture();

        let pruned = prune(&project, &container, &[]).unwrap();

        assert!(pruned.removed.is_empty());
        assert_eq!(pruned.synced.members.len(), 3, "{:?}", pruned.synced.members);
        assert!(container.join("napi/Cargo.toml").is_file());
    }
}
