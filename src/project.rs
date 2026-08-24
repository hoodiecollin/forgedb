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

const MANIFESTS: [&str; 4] = ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"];

/// One `forgedb.toml` found by the walk.
#[derive(Debug)]
pub struct Link {
    /// The directory holding the config.
    pub dir: PathBuf,
    /// The config file itself.
    pub path: PathBuf,
    /// Its parsed contents.
    pub config: ForgeConfig,
    /// 1-based line/column of `[project].name`, when declared.  Kept so the
    /// non-root-`name` contradiction can be reported at the offending key rather
    /// than at the file.
    name_pos: Option<(usize, usize)>,
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
    let name_pos = config
        .project
        .name
        .as_ref()
        .map(|n| config::key_position(&content, n.span().start));
    Ok(Some(Link {
        dir: dir.to_path_buf(),
        path,
        config,
        name_pos,
    }))
}

/// Where a project id came from.  Kept because the three paths warrant different
/// treatment: only the first two can collide, and only the third warns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSource {
    /// `[project].name` at the project root.
    Explicit,
    /// Borrowed from the named ecosystem manifest beside the project root.
    Manifest(&'static str),
    /// Neither was available — a hash of the project root's absolute path.
    PathHash,
}

/// A resolved project identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectId {
    /// The id itself.  Used verbatim as a directory name under
    /// `~/.forgedb/projects/`, hence [`validate_name`].
    pub name: String,
    /// The absolute directory the id is keyed on.
    pub root: PathBuf,
    pub source: IdSource,
}

/// A decision ForgeDB cannot make on its own, put to whoever can (#367).
///
/// Both variants carry the facts ForgeDB *already computed*, structurally
/// rather than pre-formatted, so the prompt and the non-interactive diagnostic
/// render from **one** derivation.  Re-deriving "which manifests name this
/// root" at the prompt is the drift class this repo has been bitten by
/// repeatedly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Two or more ecosystem manifests name the project root.
    WhichName {
        /// The directory identity is keyed on.
        root: PathBuf,
        /// `(manifest, name)` in `MANIFESTS` order.
        candidates: Vec<(&'static str, String)>,
        /// The schema the failing invocation resolved, for the remedy command.
        schema_hint: Option<PathBuf>,
    },
    /// The resolved id is already claimed by another root.
    Collision {
        /// The contested id.
        id: String,
        /// Our project root.
        root: PathBuf,
        /// The root the ledger says holds it.
        held_by: PathBuf,
        /// Whether that root still exists.  **The answer set depends on this**,
        /// which is the strongest reason this decision cannot be a flag: it is a
        /// fact about the filesystem the user cannot know when they type the
        /// command.
        holder_exists: bool,
        /// The schema the failing invocation resolved, for the remedy command.
        schema_hint: Option<PathBuf>,
    },
}

/// What came back.  Deliberately *not* an `Option<String>`: the two answers act
/// on different files — a name is a resolution and goes in the project's own
/// config, a take-over is detection state and goes in the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Use this `[project].name`, and persist it.
    Name(String),
    /// The holding root is gone; take the id over.  Writes the ledger and
    /// **nothing else** — the project keeps its name.
    TakeOverClaim,
}

/// Who answers a [`Question`].
///
/// The seam that makes the interactive path testable without a terminal: the
/// *decision* ([`crate::ask::Askability`]) and the *act* ([`record_name`],
/// [`take_over_claim`]) are on this side, the widget is on the far side.
pub trait Asker {
    /// `Ok(None)` = not answered — the caller takes the **unchanged**
    /// non-interactive error.  "Cannot ask" and "declined" are deliberately the
    /// same path; declining must not be a third behaviour.
    fn ask(&self, q: &Question) -> Result<Option<Answer>>;

    /// Consent to a format-preserving edit of a `forgedb.toml` ForgeDB did not
    /// author.  *Creating* one where none exists needs no consent — there is
    /// nothing to damage — but editing one does.
    fn confirm_edit(&self, path: &Path) -> Result<bool>;
}

/// What [`identify_or_ask`] resolved, or why it could not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identified {
    /// A single answer.
    Resolved(ProjectId),
    /// Two or more manifests name the root and nothing chooses between them.
    Ambiguous {
        /// The directory identity is keyed on.
        root: PathBuf,
        /// `(manifest, name)` in `MANIFESTS` order.
        candidates: Vec<(&'static str, String)>,
    },
}

/// Resolve the project identity for a chain, or report the one case that has no
/// single answer.
///
/// Order at the **project root** config — not the nearest one: explicit name,
/// then exactly one detectable ecosystem manifest, then a hash of the root's
/// absolute path.
///
/// [`identify`] is the thin wrapper that turns [`Identified::Ambiguous`] back
/// into today's error, which keeps that message in exactly one place while
/// letting a prompt reach the candidates **without re-deriving them**.
pub fn identify_or_ask(chain: &Chain) -> Result<Identified> {
    let root_link = chain.project_root();
    let root = chain.root_dir();

    // A nested, non-root config naming a project is a contradiction with a real
    // cost: it reads as authoritative and is not, and the two candidate
    // identities differ.  This replaced the withdrawn "init under a project is an
    // error" rule, and is strictly more valuable — it catches a wrong *belief*
    // rather than a normal action.
    // Only configs BELOW the project root are in this project. One above it
    // belongs to an enclosing project and names that one legitimately — which is
    // exactly the shape of a monorepo whose root is named and one of whose apps
    // has declared `isolated = true`.
    let inside = chain
        .links
        .iter()
        .take_while(|l| !root_link.is_some_and(|r| r.dir == l.dir));

    for link in inside {
        if link.config.project.name.is_none() {
            continue;
        }
        let (line, column) = link.name_pos.unwrap_or((1, 1));
        return Err(CliError::ConfigDiagnostic(format!(
            "{}:{}:{}: `[project].name` is declared at a config that is not the \
             project root.\n\n\
             This config is nested inside the project rooted at {}, so the name it \
             declares is never used — it reads as authoritative and is not.\n\n\
             Either remove the key, or set `isolated = true` here to make these \
             schemas a project of their own.",
            link.path.display(),
            line,
            column,
            root.display(),
        )));
    }

    if let Some(name) = root_link.and_then(|l| l.config.project.name()) {
        validate_name(name, "[project].name")?;
        return Ok(Identified::Resolved(ProjectId {
            name: name.to_string(),
            root,
            source: IdSource::Explicit,
        }));
    }

    let detected = detect_manifest_names(&root);

    match detected.len() {
        1 => {
            let (manifest, name) = &detected[0];
            validate_name(name, manifest)?;
            Ok(Identified::Resolved(ProjectId {
                name: name.clone(),
                root,
                source: IdSource::Manifest(manifest),
            }))
        }
        0 => Ok(Identified::Resolved(ProjectId {
            name: path_hash_name(&root),
            root,
            source: IdSource::PathHash,
        })),
        _ => Ok(Identified::Ambiguous {
            root,
            candidates: detected,
        }),
    }
}

/// Every ecosystem manifest beside `root` that declares a name, in `MANIFESTS`
/// order.
///
/// One definition, reached by both the prompt payload and the diagnostic.
fn detect_manifest_names(root: &Path) -> Vec<(&'static str, String)> {
    MANIFESTS
        .iter()
        .filter_map(|m| manifest_name(&root.join(m)).map(|n| (*m, n)))
        .collect()
}

/// Resolve the project identity, refusing an ambiguous root.
///
/// Signature and message unchanged from #333; the ambiguity branch now formats
/// from [`identify_or_ask`]'s value rather than deriving the candidates a second
/// time.
pub fn identify(chain: &Chain) -> Result<ProjectId> {
    match identify_or_ask(chain)? {
        Identified::Resolved(id) => Ok(id),
        Identified::Ambiguous { root, candidates } => {
            Err(ambiguity_error(&root, &candidates, chain.schema()))
        }
    }
}

/// Today's ambiguity diagnostic, plus the **command** that records the answer.
///
/// The command is the whole difference between "here is a remedy you must apply
/// by hand" and "here is a remedy, and here is how to apply it in your CI
/// script" — and it carries the resolved `--schema`, because in a monorepo the
/// same command run from elsewhere resolves a different project.
fn ambiguity_error(
    root: &Path,
    candidates: &[(&'static str, String)],
    schema_hint: Option<&Path>,
) -> CliError {
    let list = candidates
        .iter()
        .map(|(m, n)| format!("  {m} → {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    CliError::ConfigDiagnostic(format!(
        "{}: cannot pick a project name — {} ecosystem manifests name this \
         directory:\n{}\n\n\
         Set `[project].name` in {} to say which one ForgeDB should use.\n\n\
         Or record it with:\n  {}",
        root.display(),
        candidates.len(),
        list,
        root.join(CONFIG_FILE).display(),
        name_command(candidates.first().map(|(_, n)| n.as_str()), schema_hint),
    ))
}

/// The copy-pasteable `forgedb project name …` line a diagnostic prints.
fn name_command(example: Option<&str>, schema_hint: Option<&Path>) -> String {
    let name = example.unwrap_or("<NAME>");
    match schema_hint {
        Some(s) => format!("forgedb project name {name} --schema {}", s.display()),
        None => format!("forgedb project name {name}"),
    }
}

/// A project id is used verbatim as a directory name under
/// `~/.forgedb/projects/`, so a name carrying a separator or a `..` would escape
/// the cache rather than key it.
fn validate_name(name: &str, source: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.chars().any(|c| c.is_control());
    if bad {
        return Err(CliError::Config(format!(
            "Invalid project name {name:?} from {source}: a project name is used \
             verbatim as a directory name, so it cannot be empty, contain a path \
             separator, or be `.` or `..`."
        )));
    }
    Ok(())
}

/// Read a package name out of an ecosystem manifest, if it declares one.
///
/// Deliberately shallow: a `[workspace]`-only `Cargo.toml` names nothing, and
/// reporting no name is the correct answer there.
fn manifest_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let file_name = path.file_name()?.to_str()?;
    let name = match file_name {
        "Cargo.toml" => toml::from_str::<toml::Value>(&content)
            .ok()?
            .get("package")?
            .get("name")?
            .as_str()?
            .to_string(),
        "pyproject.toml" => toml::from_str::<toml::Value>(&content)
            .ok()?
            .get("project")?
            .get("name")?
            .as_str()?
            .to_string(),
        "package.json" => serde_json::from_str::<serde_json::Value>(&content)
            .ok()?
            .get("name")?
            .as_str()?
            .to_string(),
        // `module example.com/foo/bar` — the last segment is the package name.
        "go.mod" => content
            .lines()
            .find_map(|l| l.strip_prefix("module "))?
            .trim()
            .rsplit('/')
            .next()?
            .to_string(),
        _ => return None,
    };
    (!name.is_empty()).then_some(name)
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

/// Take an id over from whoever the ledger says holds it.
///
/// **Ledger only — no config is touched.** The ledger records *who currently
/// holds an id*, which is detection state and legitimately lives in a directory
/// GC may empty at any time. A chosen *name* is a resolution and goes in the
/// project's own `forgedb.toml`; recording one here would resurrect a resolved
/// collision as a silent merge of two projects the moment the cache is wiped
/// (the C1 line, and `scenario_14`'s standing guard).
///
/// `force` is required when the holding root still exists. Gate 1 forbids
/// *automatic* reaping — a path can be absent because a network mount is not
/// mounted, which is exactly when taking the id over is wrong — but an explicit
/// human act over a live holder is a different thing, and it prints what it
/// displaced.
///
/// **Not atomic, unlike [`claim`].** Claiming is `O_EXCL` on a file that must
/// not exist; a take-over is read-then-write and cannot have that guarantee.
/// The holder is re-read immediately before the write and the write is a
/// temp-file rename, which is as close as this gets. Saying so here rather than
/// implying an atomicity the code does not have: the ledger is documented as
/// GC-able derived state, so a lock around it would be pretending.
pub fn take_over_claim(id: &ProjectId, force: bool) -> Result<Option<Holder>> {
    if id.source == IdSource::PathHash {
        return Err(CliError::Config(format!(
            "Project id {:?} is derived from a path hash, so it is never claimed \
             and there is nothing to take over. Two different absolute paths hash \
             differently, so this id cannot collide with itself.",
            id.name
        )));
    }

    let dir = cache::ledger_root()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.claim", id.name));
    let ours = id.root.to_string_lossy().to_string();

    // Re-read immediately before the write: the gap between the read that
    // produced the diagnostic and this one is where another process could have
    // claimed it.
    let previous = held_by(&id.name)?;
    match &previous {
        Some(h) if h.path == id.root => return Ok(None),
        Some(h) if h.exists && !force => {
            return Err(CliError::ConfigDiagnostic(format!(
                "Project name {:?} is held by {}, and that path still \
                 exists.\n\n\
                 This is a real collision, not a stale claim: two projects sharing \
                 an id would share one build cache, one lockfile and one target \
                 directory. The usual answer is a different `[project].name` \
                 here.\n\n\
                 If you are certain that root is no longer a ForgeDB project, \
                 pass --force to displace it.",
                id.name,
                h.path.display(),
            )));
        }
        _ => {}
    }

    let temp = dir.join(format!(".{}.claim.forgedb-tmp", id.name));
    std::fs::write(&temp, ours.as_bytes())?;
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(previous)
}

/// Drop our own claim on an id.
///
/// Refuses to release a claim held by another root: the ledger entry is that
/// root's, and deleting it from here would hand the id to whoever ran next
/// rather than resolve anything. Returns whether a claim was actually removed.
pub fn release_claim(id: &ProjectId) -> Result<bool> {
    if id.source == IdSource::PathHash {
        return Ok(false);
    }
    let path = cache::ledger_root()?.join(format!("{}.claim", id.name));
    match held_by(&id.name)? {
        None => Ok(false),
        Some(h) if h.path == id.root => {
            std::fs::remove_file(&path)?;
            Ok(true)
        }
        Some(h) => Err(CliError::ConfigDiagnostic(format!(
            "Project name {:?} is claimed by {}, not by {}.\n\n\
             `release` drops this project's own claim. Releasing another root's \
             would not resolve anything — it would hand the id to whichever \
             project ran next.",
            id.name,
            h.path.display(),
            id.root.display(),
        ))),
    }
}

/// Resolve an identity and refuse a collision, asking nothing.
///
/// The non-interactive contract, unchanged: a taken id is an error naming the
/// remedy, never a silently-picked alternative.  The remedy is written into the
/// project's *own* config, which is why it survives a cache wipe.
pub fn identify_and_claim(chain: &Chain) -> Result<ProjectId> {
    identify_and_claim_with(chain, &crate::ask::NeverAsk)
}

/// Resolve an identity, putting each undecidable case to `asker` first.
///
/// Every decline — and every context that cannot ask at all — falls through to
/// exactly the errors [`identify_and_claim`] produces.  A prompt only ever fills
/// an answer that is otherwise absent.
pub fn identify_and_claim_with(chain: &Chain, asker: &dyn Asker) -> Result<ProjectId> {
    // Step 5 of #367 consults `asker` here, once there is an act to perform with
    // the answer (`record_name` lands in step 2, `take_over_claim` in step 3).
    // Threaded now so the boundary, the vocabulary and the call sites land as
    // one reviewable change that alters no behaviour.
    let _ = asker;
    let id = identify(chain)?;
    if let Claim::Conflict {
        held_by,
        holder_exists,
    } = claim(&id)?
    {
        return Err(collision_error(
            &id,
            &Holder {
                path: held_by,
                exists: holder_exists,
            },
            chain.schema(),
        ));
    }
    Ok(id)
}

/// The collision diagnostic — **two messages, because there are two remedies.**
///
/// A live holder is a real collision and the answer is a different name.  A dead
/// holder is a project colliding with its own ghost, and the answer is to take
/// the claim over; telling that user to rename themselves is the bug #367 fixes,
/// so the dead-holder branch deliberately does **not** mention
/// `[project].name`.
fn collision_error(id: &ProjectId, holder: &Holder, schema_hint: Option<&Path>) -> CliError {
    let schema = schema_hint
        .map(|s| format!(" --schema {}", s.display()))
        .unwrap_or_default();
    if holder.exists {
        CliError::ConfigDiagnostic(format!(
            "Project name {:?} is already claimed by {}.\n\n\
             Two projects sharing an id would share one build cache, one lockfile \
             and one target directory.\n\n\
             Set a different `[project].name` in {} — writing it there (rather than \
             in the cache) is what makes the resolution survive `rm -rf ~/.forgedb`.\n\n\
             Or record it with:\n  forgedb project name <NAME>{schema}",
            id.name,
            holder.path.display(),
            id.root.join(CONFIG_FILE).display(),
        ))
    } else {
        CliError::ConfigDiagnostic(format!(
            "Project name {:?} is held in the ledger by {}, which no longer \
             exists.\n\n\
             Nothing removes a claim, so a project that was moved, renamed or \
             deleted collides with its own record. This is very likely that — but \
             a missing path can also mean an unmounted volume or an unplugged \
             disk, which is exactly when taking the id over would be wrong, so \
             ForgeDB will not do it for you.\n\n\
             If that path is gone for good, take the id over with:\n  \
             forgedb project claim --take-over{schema}",
            id.name,
            holder.path.display(),
        ))
    }
}
/// Where a name was recorded, and whether the file had to be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// The `forgedb.toml` that now declares the name.
    pub path: PathBuf,
    /// Whether ForgeDB created that file.
    pub created: bool,
}

/// **THE PERSISTING ACT.**  Record `[project].name` for this chain's project.
///
/// Writes at [`Chain::root_dir`] — *never* at [`Chain::nearest`].  "One walk,
/// two answers" means the knob config and the identity config are usually
/// different directories in a monorepo, and recording a name at the nearest one
/// produces [`identify`]'s "declared at a config that is not the project root"
/// error on the very next run: a failure that appears one invocation later, in a
/// different message, reading as a user mistake.
///
/// Two shapes, per the accepted split:
///
/// * **Create**, unconditionally, when the chain holds no config at all.  There
///   is nothing to damage, nothing to preserve and no name to clobber — and this
///   is the common instance, because a project ForgeDB scaffolded already has a
///   name and never reaches either decision.
/// * **Edit**, format-preserving, only with `asker.confirm_edit`.  Typing
///   `forgedb project name` IS that consent
///   ([`crate::ask::CommandConsent`]); a `generate` that merely wanted the answer
///   is not ([`crate::ask::NeverAsk`]), and takes an error naming the file and
///   the key instead.
pub fn record_name(
    chain: &Chain,
    name: &str,
    overwrite: bool,
    asker: &dyn Asker,
) -> Result<Recorded> {
    validate_name(name, "the requested project name")?;
    let root = chain.root_dir();

    let Some(link) = chain.project_root() else {
        // The premise that makes an unconditional create safe: with no config
        // anywhere, `root_dir()` is the schema's own directory and `isolated`
        // takes its `true` default, so the created file regroups nothing. If a
        // config existed in the chain, `root_dir()` would BE that config's
        // directory and this would be an edit.
        debug_assert!(
            chain.links().is_empty(),
            "a chain with links always has a project root"
        );
        let path = config::create_project_config(&root, name)?;
        return Ok(Recorded {
            path,
            created: true,
        });
    };

    let path = link.path.clone();
    if !asker.confirm_edit(&path)? {
        return Err(CliError::ConfigDiagnostic(format!(
            "{} already exists, and ForgeDB does not edit a config it did not \
             author without being asked.\n\n\
             Add this under `[project]`:\n  name = \"{name}\"\n\n\
             Or let ForgeDB write it:\n  forgedb project name {name}{}",
            path.display(),
            chain
                .schema()
                .map(|s| format!(" --schema {}", s.display()))
                .unwrap_or_default(),
        )));
    }
    config::set_project_name(&path, name, overwrite)?;
    Ok(Recorded {
        path,
        created: false,
    })
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

    /// The project this app belongs to, with its id claimed.
    pub fn identify(&self, asker: &dyn Asker) -> Result<ProjectId> {
        identify_and_claim_with(&self.chain, asker)
    }

    /// Resolve the project, report it, and warn when the id had to be invented.
    ///
    /// The warning is on the path-hash fallback only, and it fires at most once
    /// per invocation: an id nobody chose is fine as a default and bad as a
    /// surprise, since it is what a `projects/` listing will be full of.
    ///
    /// `asker` decides what happens at the two points identity cannot decide
    /// alone (#367).  Callers pass `&*crate::ask::asker()`, which is
    /// [`crate::ask::NeverAsk`] in every context that must not block.
    pub fn identify_reported(&self, asker: &dyn Asker) -> Result<ProjectId> {
        let id = self.identify(asker)?;
        match &id.source {
            IdSource::Explicit => {
                crate::ui::detail(&format!("Project: {} (from [project].name)", id.name))
            }
            IdSource::Manifest(m) => {
                crate::ui::detail(&format!("Project: {} (from {})", id.name, m))
            }
            IdSource::PathHash => crate::ui::warning(&format!(
                "Project: {} — derived from the path of {}, because no \
                 `[project].name` and no ecosystem manifest name it. Set \
                 `[project].name` to give it a stable name.",
                id.name,
                id.root.display()
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
