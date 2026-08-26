//! Which project is this? (#333, epic #332)
//!
//! A `forgedb.toml` does **not** declare an app — a `.forge` schema does.  A
//! config carries knobs that tweak the apps generated from schema files in its
//! descendant directories, and its only say in *grouping* is whether the schemas
//! beneath it form a project of their own.
//!
//! Everything here follows from that one premise:
//!
//! * **The walk starts at the schema, never at the CWD.**  `forgedb generate
//!   --schema apps/api/schema.forge` must answer the same way from the repo root
//!   as from `apps/api`.  Starting at the CWD lets `cd` change an app's project
//!   id, and therefore its build-cache key.
//! * **One walk, two answers.**  Knobs come from the *nearest* config in the
//!   chain; identity comes from the *project root* — the nearest `isolated`
//!   config, else the outermost one.  In any monorepo those are different
//!   directories, so code that resolves "the project config" once and uses it for
//!   both compiles, runs, and silently mis-keys the cache.  [`Chain`] therefore
//!   exposes [`Chain::nearest`] and [`Chain::project_root`] and no way to ask for
//!   "the" config.
//! * **The ledger detects; the config records.**  A resolved collision is written
//!   into the colliding project's own `forgedb.toml`, never into the ledger — if
//!   the ledger held the resolution, deleting `~/.forgedb` would resurrect the
//!   collision as a silent merge of two projects (a C1 violation).
//!
//! This module is also the **single entry point for reading config**.  #361
//! established that `load_config` had exactly one call site, and that is a
//! greppable property rather than an intention; this work turns one read per
//! invocation into *N*, so they all funnel through [`Chain::walk`] to keep the
//! grep meaning something.

use std::path::{Component, Path, PathBuf};

use crate::cache;
use crate::config::{self, ForgeConfig, CONFIG_FILE};
use crate::error::{CliError, Result};

/// Schema file names looked for when `--schema` is absent.
///
/// One definition, because there were three: `commands/generate/mod.rs`,
/// `commands/validate.rs` and `commands/migrate.rs` each carried their own copy
/// of this list, so adding a name meant finding all three.
pub const SCHEMA_CANDIDATES: [&str; 3] = ["schema.forge", "schema.lang", "schema.forgedb"];

/// Ecosystem manifests a project name can be borrowed from, in the order they
/// are reported.  Read **only** to suggest a name: ForgeDB has no opinion about,
/// and no write access to, any of these ecosystems' workspace membership.
/// Where generated code goes when neither a flag nor a config names a directory.
const DEFAULT_OUTPUT: &str = "generated";

/// One `forgedb.toml` found by the walk.
#[derive(Debug)]
pub struct Link {
    /// The directory holding the config.
    pub dir: PathBuf,
    /// The config file itself.
    pub path: PathBuf,
    /// Its parsed contents.
    pub config: ForgeConfig,
    /// 1-based line/column of `[project].id`, when declared.  Kept so the
    /// non-root-`id` contradiction can be reported at the offending key rather
    /// than at the file.
    id_pos: Option<(usize, usize)>,
}

/// Every `forgedb.toml` at or above a starting directory, nearest first.
#[derive(Debug)]
pub struct Chain {
    /// The directory the walk started from — the schema's directory, or the CWD
    /// for commands that take no schema.
    pub start: PathBuf,
    links: Vec<Link>,
    /// The schema this walk was started *for*, when there was one.
    ///
    /// Carried only so a diagnostic can print the remedy command with the
    /// `--schema` the failing invocation already resolved (#367).  In a monorepo
    /// `forgedb project name X` run from another directory walks a **different
    /// chain**, so a remedy printed without the schema is copy-pasteable and
    /// subtly wrong — it would name a different project.
    schema: Option<PathBuf>,
}

impl Chain {
    /// Walk up from `from`, collecting configs, stopping at the boundary.
    ///
    /// The boundary matters: without one a stray `~/forgedb.toml` captures every
    /// project on the machine.  It is `$HOME` (**exclusive** — that stray file is
    /// exactly the hazard), a repository root (**inclusive** — a monorepo root
    /// config is the config we most want), or the filesystem root.
    pub fn walk(from: &Path) -> Result<Chain> {
        // Canonical, not merely absolute: `Link::dir` is canonical because it
        // comes from the walk, and `root_dir()` falls back to `start` when no
        // config exists anywhere. Two spellings of one directory there would make
        // an identity depend on how the path was typed.
        let start = canonical_or(&absolutize(from));
        let home = home::home_dir().map(|h| canonical_or(&h));
        let mut links = Vec::new();
        let mut dir = start.clone();

        loop {
            if home.as_deref() == Some(dir.as_path()) {
                break;
            }
            if let Some(link) = load_link(&dir)? {
                links.push(link);
            }
            // Checked after loading, so the repository root's own config is part
            // of the chain rather than just outside it.
            if dir.join(".git").exists() {
                break;
            }
            match dir.parent() {
                Some(parent) if parent != dir => dir = parent.to_path_buf(),
                _ => break,
            }
        }

        Ok(Chain {
            start,
            links,
            schema: None,
        })
    }

    /// The walk a schema-taking command runs: up from the schema's directory,
    /// remembering which schema it was for.
    ///
    /// Identical to [`Chain::walk`] in every answer it gives.  The only
    /// difference is [`Chain::schema`], which diagnostics use to print a remedy
    /// that resolves the same project the failing invocation did.
    pub fn walk_from_schema(schema: &Path) -> Result<Chain> {
        let mut chain = Chain::walk(schema_dir(schema))?;
        chain.schema = Some(schema.to_path_buf());
        Ok(chain)
    }

    /// The schema this walk was started for, when there was one.
    pub fn schema(&self) -> Option<&Path> {
        self.schema.as_deref()
    }

    /// The config whose knobs apply: the nearest one.  Knobs do **not** layer or
    /// merge — nearest wins entirely, which is what today's single-file behavior
    /// extends to honestly.
    pub fn nearest(&self) -> Option<&Link> {
        self.links.first()
    }

    /// [`Chain::nearest`]'s config, taken by value.
    pub fn into_nearest_config(mut self) -> Option<ForgeConfig> {
        if self.links.is_empty() {
            None
        } else {
            Some(self.links.remove(0).config)
        }
    }

    /// The config that decides identity: the nearest `isolated` one, else the
    /// outermost.  Frequently a different directory from [`Chain::nearest`].
    pub fn project_root(&self) -> Option<&Link> {
        self.links
            .iter()
            .find(|l| l.config.project.isolated)
            .or_else(|| self.links.last())
    }

    /// Every link, nearest first.
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The directory identity is keyed on, whether or not a config declares it.
    /// With no config anywhere, a bare schema still belongs to *something*, and
    /// that something is its own directory.
    pub fn root_dir(&self) -> PathBuf {
        self.project_root()
            .map(|l| l.dir.clone())
            .unwrap_or_else(|| self.start.clone())
    }
}

fn load_link(dir: &Path) -> Result<Option<Link>> {
    let path = dir.join(CONFIG_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| {
        CliError::Config(format!("Cannot read config file '{}': {}", path.display(), e))
    })?;
    let config = config::parse_config(&content, &path)?;
    let id_pos = config
        .project
        .id
        .as_ref()
        .map(|n| config::key_position(&content, n.span().start));
    Ok(Some(Link {
        dir: dir.to_path_buf(),
        path,
        config,
        id_pos,
    }))
}

/// Where a project id came from.  Two paths, and neither can collide by
/// accident — which is the point of #479 and the reason there is no third.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSource {
    /// `[project].id` at the project root, minted by `forgedb init`.
    Declared,
    /// No id was declared — a hash of the project root's absolute path.
    PathHash,
}

/// A resolved project identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectId {
    /// The id itself.  Used verbatim as a directory name under
    /// `~/.forgedb/projects/`, hence [`validate_id`].
    pub name: String,
    /// The absolute directory the id is keyed on.
    pub root: PathBuf,
    pub source: IdSource,
}

/// A project id is used verbatim as a directory name under
/// `~/.forgedb/projects/`, so an id carrying a separator or a `..` would escape
/// the cache rather than key it.
fn validate_id(name: &str, source: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.chars().any(|c| c.is_control());
    if bad {
        return Err(CliError::Config(format!(
            "Invalid project id {name:?} from {source}: a project id is used \
             verbatim as a directory name, so it cannot be empty, contain a path \
             separator, or be `.` or `..`."
        )));
    }
    Ok(())
}

/// Resolve the project identity for a chain.
///
/// **Two branches, and both are collision-free by construction** (#479):
///
/// 1. `[project].id` declared at the project root — minted once by
///    `forgedb init` and committed, so every clone, teammate and CI run
///    resolves the same one.
/// 2. otherwise, a hash of the root's **absolute** path — two different
///    absolute paths hash differently, so it cannot collide with itself.
///
/// What is deliberately *not* here is a third branch deriving the id from an
/// ecosystem manifest's package name. That derivation is what made two
/// unrelated projects able to want one id, and every remedy that existed —
/// the ambiguity prompt, the take-over command, its release inverse — was a
/// consequence of it rather than of the ledger.
pub fn identify(chain: &Chain) -> Result<ProjectId> {
    let root_link = chain.project_root();
    let root = chain.root_dir();

    // A nested, non-root config declaring an id is a contradiction with a real
    // cost: it reads as authoritative and is not, and the two candidate
    // identities differ.  Only configs BELOW the project root are in this
    // project — one above it belongs to an enclosing project and names that one
    // legitimately, which is exactly the shape of a monorepo whose root is named
    // and one of whose apps has declared `isolated = true`.
    let inside = chain
        .links
        .iter()
        .take_while(|l| !root_link.is_some_and(|r| r.dir == l.dir));

    for link in inside {
        if link.config.project.id.is_none() {
            continue;
        }
        let (line, column) = link.id_pos.unwrap_or((1, 1));
        return Err(CliError::ConfigDiagnostic(format!(
            "{}:{}:{}: `[project].id` is declared at a config that is not the \
             project root.\n\n\
             This config is nested inside the project rooted at {}, so the id it \
             declares is never used — it reads as authoritative and is not.\n\n\
             Either remove the key, or set `isolated = true` here to make these \
             schemas a project of their own.",
            link.path.display(),
            line,
            column,
            root.display(),
        )));
    }

    if let Some(id) = root_link.and_then(|l| l.config.project.id()) {
        validate_id(id, "[project].id")?;
        return Ok(ProjectId {
            name: id.to_string(),
            root,
            source: IdSource::Declared,
        });
    }

    Ok(ProjectId {
        name: path_hash_name(&root),
        root,
        source: IdSource::PathHash,
    })
}

/// Resolve an identity and refuse a collision.
///
/// A minted id collides in exactly one realistic way — a project directory was
/// copied and both halves carry the same committed `[project].id` — so this
/// reports and refuses. There is no remedy *command*, because the remedy is a
/// one-key edit in a file the user owns, and a command to perform it would be a
/// generic config editor whose flagship use is hand-setting an id that should
/// never be hand-set.
pub fn identify_and_claim(chain: &Chain) -> Result<ProjectId> {
    let id = identify(chain)?;
    match claim(&id)? {
        Claim::Conflict {
            held_by,
            holder_exists,
        } => Err(collision_error(
            &id,
            &Holder {
                path: held_by,
                exists: holder_exists,
            },
            chain,
        )),
        _ => Ok(id),
    }
}

/// The collision diagnostic — **one message, because there is one cause.**
///
/// The live/dead-holder split this replaced existed because a *derived* id
/// could collide two ways: with an unrelated project that happened to share a
/// package name, or with the project's own ghost after a move. A minted id does
/// neither. What is left is a copy — `cp -r`, a template fork, a directory
/// duplicated to try something — carrying an id that was already spoken for.
///
/// Liveness is still reported, because "that path is gone" and "that path is
/// right there" tell the reader different things about which copy is which. It
/// no longer selects a different remedy.
fn collision_error(id: &ProjectId, holder: &Holder, chain: &Chain) -> CliError {
    let config = chain
        .project_root()
        .map(|l| l.path.display().to_string())
        .unwrap_or_else(|| id.root.join(crate::config::CONFIG_FILE).display().to_string());
    CliError::ConfigDiagnostic(format!(
        "Project id {:?} is already held by {}{}.\n\n\
         Ids are generated at `forgedb init` and committed, so two projects \
         carry the same one only when a project directory was copied — the copy \
         inherited the original's `[project].id`.\n\n\
         Two projects sharing an id would share one build cache, one lockfile \
         and one target directory.\n\n\
         Give this one an id of its own in {}:\n  [project]\n  id = {:?}",
        id.name,
        holder.path.display(),
        if holder.exists {
            ""
        } else {
            ", a path that no longer exists"
        },
        config,
        mint_id(&id.root),
    ))
}

/// Mint a project id: the root's directory name, plus entropy.
///
/// **No new dependency.** The root crate carries neither `rand` nor `uuid`, and
/// pulling one in to produce a value once per `forgedb init` is not worth the
/// graph. The entropy is `RandomState`, which the standard library seeds from
/// the OS per process — the same property that made #457's `HashMap` iteration
/// order vary across runs, used deliberately here — mixed with the wall clock
/// and the pid so two `init`s inside one process still differ.
///
/// The slug is cosmetic and the hex carries the uniqueness: `~/.forgedb/projects/`
/// is a directory a human goes looking through, and `storefront-7f3a9c2e` is
/// findable there in a way a bare uuid is not.
pub fn mint_id(root: &Path) -> String {
    use std::hash::{BuildHasher, Hash, Hasher};

    let slug: String = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_lowercase();
    let slug = if slug.is_empty() { "project" } else { &slug };

    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    root.hash(&mut h);
    std::process::id().hash(&mut h);
    if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        d.as_nanos().hash(&mut h);
    }
    format!("{slug}-{:08x}", h.finish() as u32)
}

/// The fallback id: the root's directory name plus a hash of its **absolute**
/// path.
///
/// Absolute is deliberate, and asymmetric with #334's member hash on purpose.
/// The member hash is over a *project-relative* path so a clone or a CI runner
/// resolves to the same member directory; this one only has to be unique on this
/// machine, and hashing the *root* rather than the CWD (as epic #332 originally
/// said) is what stops `cd` from changing the answer.
fn path_hash_name(root: &Path) -> String {
    let slug: String = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let hash = cache::path_hash(&root.to_string_lossy());
    if slug.is_empty() {
        format!("project-{hash}")
    } else {
        format!("{slug}-{hash}")
    }
}

/// The outcome of claiming an id in the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Nobody held this id.
    Fresh,
    /// We already hold it.
    Ours,
    /// Another root holds it.
    Conflict {
        /// The root the ledger names.
        held_by: PathBuf,
        /// Whether that root still exists.
        ///
        /// The ledger is **append-only** — nothing anywhere removes a `.claim` —
        /// so a project that was moved, renamed or deleted collides with its own
        /// ghost, and the remedy is to release a dead claim rather than to
        /// rename a project that has no actual conflict (#367).
        holder_exists: bool,
    },
}

/// Who holds an id, and whether that root is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    /// The absolute root path recorded in the ledger.
    pub path: PathBuf,
    /// Whether that path exists **right now**.
    ///
    /// Deliberately shallow: a directory that still exists but was emptied
    /// counts as live.  Probing further ("does it still resolve to this id?")
    /// would be a second identity derivation over a tree we are not walking, and
    /// an absent path can mean an unmounted volume — which is precisely when
    /// taking the id over is wrong.  Detect and offer; never reap.
    pub exists: bool,
}

/// Claim a project id, or report who already holds it.
///
/// A claim is one small file, so create-if-absent is atomic on its own and needs
/// no lock — two `forgedb generate` runs claiming simultaneously is exactly the
/// case `O_EXCL` was made for.
///
/// [`IdSource::PathHash`] ids are **not** claimed: two different absolute paths
/// hash differently, so that path cannot collide with itself, and writing a
/// claim for it would present the ledger as a general uniqueness mechanism it is
/// not.
pub fn claim(id: &ProjectId) -> Result<Claim> {
    if id.source == IdSource::PathHash {
        return Ok(Claim::Ours);
    }

    let dir = cache::ledger_root()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.claim", id.name));
    let ours = id.root.to_string_lossy().to_string();

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(ours.as_bytes())?;
            Ok(Claim::Fresh)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let held = std::fs::read_to_string(&path)?;
            if held.trim() == ours {
                Ok(Claim::Ours)
            } else {
                let held_by = PathBuf::from(held.trim());
                Ok(Claim::Conflict {
                    holder_exists: held_by.exists(),
                    held_by,
                })
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Who holds a project id, without claiming it.
///
/// C12 asks `init` to report a conflict at the point the name is chosen, which is
/// before anything should be reserved: a scaffold that claims an id it may never
/// generate leaves a stale claim behind for every abandoned `init`.
pub fn held_by(name: &str) -> Result<Option<Holder>> {
    let path = cache::ledger_root()?.join(format!("{name}.claim"));
    match std::fs::read_to_string(&path) {
        Ok(held) => {
            let path = PathBuf::from(held.trim());
            Ok(Some(Holder {
                exists: path.exists(),
                path,
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The directory a schema's governing config is walked up from, and the directory its
/// `migrations/` sits beside.
///
/// A bare `schema.forge` has no parent component, and walking from `""` would resolve
/// against the filesystem **root** rather than the CWD — the difference between "the
/// config beside my schema" and "any config on the machine".
///
/// This lived in three private copies (`main.rs`, `commands/migrate.rs`, and open-coded
/// in `commands/validate.rs` in a form that missed the empty-parent case). #333/#361 made
/// this module the one place a path question is answered; the copies predate that, and
/// their existence is what let #437 happen — `generate` had no `project::` spelling to
/// reach for, so it reached for a bare relative string instead.
pub fn schema_dir(schema: &Path) -> &Path {
    match schema.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

/// Where a schema's migration lineage lives: `<the schema's directory>/migrations`.
///
/// `migrate` appends to this and `generate` bakes its serial into the generated app's
/// `EXPECTED_SCHEMA_VERSION`. If the two disagree about where it is, the interlock guards
/// nothing — **silently**, because both halves still compile and both still produce a
/// number (#437).
///
/// The failure is not merely "reads nothing and falls back to baseline". It reads whatever
/// `migrations/` the *current directory* happens to have, so generating app B from app A's
/// directory bakes A's lineage into B.
pub fn migrations_dir(schema: &Path) -> PathBuf {
    schema_dir(schema).join("migrations")
}


/// Which schema an invocation is about: `--schema` if given, else the first
/// candidate name found beside the caller.
///
/// **No config participates.** A config governs every schema beneath it, so it
/// cannot name one — see `[generate].schema`'s removal in #333 §10.
pub fn find_schema(explicit: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(PathBuf::from(p));
    }
    SCHEMA_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        .ok_or_else(|| {
            CliError::SchemaNotFound(format!(
                "No schema file found. Expected one of: {} (or pass --schema)",
                SCHEMA_CANDIDATES.join(", ")
            ))
        })
}

/// The config governing an app, plus the chain it was found in.
#[derive(Debug)]
pub struct Governing {
    /// The directory relative paths in the config resolve against — the
    /// schema's directory, not the config's.
    pub base: PathBuf,
    /// Every config at or above `base`.
    pub chain: Chain,
    explicit: Option<ForgeConfig>,
    fallback: ForgeConfig,
}

impl Governing {
    /// The knobs that apply.
    pub fn config(&self) -> &ForgeConfig {
        self.explicit
            .as_ref()
            .or_else(|| self.chain.nearest().map(|l| &l.config))
            .unwrap_or(&self.fallback)
    }

    /// Take the governing config by value.
    ///
    /// `config()` borrows from `self`, so a caller that wants to keep the knobs
    /// past the chain's lifetime has to clone the chain. This hands over the one
    /// `ForgeConfig` that applies, so a later step reads the same answer rather
    /// than re-walking and possibly getting a different one.
    pub fn into_config(self) -> ForgeConfig {
        let Governing {
            chain,
            explicit,
            fallback,
            ..
        } = self;
        explicit
            .or_else(|| chain.into_nearest_config())
            .unwrap_or(fallback)
    }

    /// How this project derives its app names — read from the **project root**
    /// config, never the nearest one.
    ///
    /// This is the same split identity already takes: knobs come from
    /// `Chain::nearest()`, project-wide facts from `Chain::project_root()`.
    /// `symbol_naming` is the second kind, and reading it from the nearest
    /// config would be worse than merely wrong — two apps in one project could
    /// then disagree about the mode, so one would compute its siblings' names
    /// under different rules than they compute their own, and the "shortest
    /// unique suffix" would stop being unique.
    pub fn symbol_naming(&self) -> crate::naming::SymbolNaming {
        self.chain
            .project_root()
            .map(|l| l.config.project.symbol_naming)
            .unwrap_or_default()
    }

    /// Resolve a config-declared relative path against the schema's directory,
    /// then express it relative to the CWD again when it sits beneath it — so the
    /// common single-app case prints exactly the path it always printed.
    pub fn resolve_path(&self, declared: &str) -> PathBuf {
        let joined = self.base.join(declared);
        match std::env::current_dir() {
            Ok(cwd) => joined
                .strip_prefix(&cwd)
                .map(Path::to_path_buf)
                .unwrap_or(joined),
            Err(_) => joined,
        }
    }

    /// Where this app's code is emitted: `--output` verbatim, else the config's
    /// `[generate].output`, else the built-in `generated` — the last two both
    /// resolved against **the schema's** directory.
    ///
    /// The built-in default is re-based for the same reason the config value is,
    /// and leaving it out was a real bug: under one root config with no `output`
    /// key at all, every app in the project emitted into the *same* `./generated`
    /// and overwrote its siblings. A CLI flag is the invocation's own word and
    /// stays verbatim.
    pub fn output(&self, flag: Option<&str>) -> String {
        if let Some(explicit) = flag {
            return explicit.to_string();
        }
        let declared = self
            .config()
            .generate
            .output
            .as_deref()
            .unwrap_or(DEFAULT_OUTPUT);
        self.resolve_path(declared).display().to_string()
    }

    /// Where the in-tree Rust package goes (#338), or `None` when the knob is
    /// absent — which is the opt-out.
    ///
    /// **A knob, not a project-wide fact**, so it comes from `Chain::nearest()`
    /// via [`Self::config`] rather than from `Chain::project_root()`: two apps
    /// under one root may legitimately place differently, exactly as they may
    /// set `output` differently.
    ///
    /// Resolved through [`Self::resolve_path`] — against the **schema's**
    /// directory, never the CWD. A root config's `rust_package = "generated/core"`
    /// is therefore a per-app pattern; the CWD-relative reading is what had every
    /// app in a project overwriting its siblings' `output`.
    ///
    /// This is the ONE reader of `[placement].rust_package`. Config is reached
    /// from this module and nowhere else (#361's one-loader invariant), so a
    /// `config.placement` access anywhere in the tree is the bug this placement
    /// prevents.
    pub fn rust_package(&self) -> Option<PathBuf> {
        self.config()
            .placement
            .rust_package
            .as_deref()
            .map(|declared| self.resolve_path(declared))
    }

    /// The project this app belongs to, with its id claimed.
    pub fn identify(&self) -> Result<ProjectId> {
        identify_and_claim(&self.chain)
    }

    /// Resolve the project, report it, and warn when the id had to be invented.
    ///
    /// The warning is on the path-hash fallback only, and it fires at most once
    /// per invocation: an id nobody chose is fine as a default and bad as a
    /// surprise, since it is what a `projects/` listing will be full of.
    pub fn identify_reported(&self) -> Result<ProjectId> {
        let id = self.identify()?;
        match &id.source {
            IdSource::Declared => {
                crate::ui::detail(&format!("Project: {} (from [project].id)", id.name))
            }
            IdSource::PathHash => crate::ui::warning(&format!(
                "Project: {} — derived from the path of {}, because no \
                 `[project].id` names it. A path-derived id changes if the \
                 directory moves, which re-keys the build cache. Add one to \
                 pin it:\n  [project]\n  id = {:?}",
                id.name,
                id.root.display(),
                mint_id(&id.root),
            )),
        }
        Ok(id)
    }
}

/// Resolve the governing config for a directory.
///
/// An explicit `--config` overrides the *knobs* outright — it is the escape
/// hatch and stays one — but never identity: the walk still happens, because
/// which project a schema belongs to is a fact about the tree, not about the
/// invocation.
pub fn govern(explicit_config: Option<&str>, base: &Path) -> Result<Governing> {
    govern_chain(explicit_config, base, Chain::walk(base)?)
}

/// [`govern`] for a command that names a schema — the walk still starts at the
/// schema's directory, and the chain remembers which schema it was for.
///
/// The remembering is what lets a diagnostic print `--schema <resolved path>`
/// alongside its remedy command.  Without it, the remedy is copy-pasteable and
/// resolves a different project when run from another directory in a monorepo.
pub fn govern_for_schema(explicit_config: Option<&str>, schema: &Path) -> Result<Governing> {
    let base = schema_dir(schema);
    govern_chain(explicit_config, base, Chain::walk_from_schema(schema)?)
}

fn govern_chain(explicit_config: Option<&str>, base: &Path, chain: Chain) -> Result<Governing> {
    let explicit = match explicit_config {
        Some(p) => Some(config::load_config_file(Path::new(p))?),
        None => None,
    };
    Ok(Governing {
        base: canonical_or(&absolutize(base)),
        chain,
        explicit,
        fallback: ForgeConfig::default(),
    })
}

/// The governing config for a command that takes no schema, walked from the CWD.
///
/// The CWD is the honest analogue when there is no schema to start from, and it
/// reproduces today's CWD-only lookup as a special case of the walk.
pub fn govern_cwd(explicit_config: Option<&str>) -> Result<Governing> {
    let cwd = std::env::current_dir()?;
    govern(explicit_config, &cwd)
}

fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return normalize(p);
    }
    match std::env::current_dir() {
        Ok(cwd) => normalize(&cwd.join(p)),
        Err(_) => p.to_path_buf(),
    }
}

/// Lexical normalization — enough to make `a/./b/../c` walk the same chain as
/// `a/c` without requiring either to exist.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn canonical_or(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Directories a schema never lives in, skipped so the walk stays cheap and
/// cannot pick up a fixture as if it were one of the project's apps.
const NOT_A_SCHEMA_DIR: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".forgedb",
    ".venv",
    "vendor",
];

/// Every `.forge` schema in the project, **project-relative and sorted**.
///
/// This is the sibling set [`crate::naming::app_name`] needs: dropping the hash
/// made a name a function of the project's whole app set, so one app cannot be
/// named without enumerating the others.
///
/// Sorted because the order is an *input* to nothing today but would be a
/// silent source of name drift the moment it were — two runs that disagree
/// about the app set must at least not disagree about its order.  Unreadable
/// directories are skipped rather than raised: a project with one unreadable
/// subtree should still build its other apps, and a name that changes because a
/// permission changed would be worse than one derived from what is visible.
pub fn discover_schemas(project_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_for_schemas(project_root, project_root, &mut out);
    out.sort();
    out
}

fn walk_for_schemas(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if kind.is_dir() {
            // Symlinked directories are not followed: a link pointing at an
            // ancestor makes the walk unbounded, and a link pointing outside
            // the project would admit a schema that is not one of its apps.
            if kind.is_symlink() || name.starts_with('.') || NOT_A_SCHEMA_DIR.contains(&&*name) {
                continue;
            }
            walk_for_schemas(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "forge")
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push(rel.to_path_buf());
        }
    }
}
