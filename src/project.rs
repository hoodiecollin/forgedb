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

        Ok(Chain { start, links })
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

/// Resolve the project identity for a chain.
///
/// Order at the **project root** config — not the nearest one: explicit name,
/// then exactly one detectable ecosystem manifest, then a hash of the root's
/// absolute path.
pub fn identify(chain: &Chain) -> Result<ProjectId> {
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
        return Ok(ProjectId {
            name: name.to_string(),
            root,
            source: IdSource::Explicit,
        });
    }

    let detected: Vec<(&'static str, String)> = MANIFESTS
        .iter()
        .filter_map(|m| manifest_name(&root.join(m)).map(|n| (*m, n)))
        .collect();

    match detected.len() {
        1 => {
            let (manifest, name) = &detected[0];
            validate_name(name, manifest)?;
            Ok(ProjectId {
                name: name.clone(),
                root,
                source: IdSource::Manifest(manifest),
            })
        }
        0 => Ok(ProjectId {
            name: path_hash_name(&root),
            root,
            source: IdSource::PathHash,
        }),
        _ => {
            let list = detected
                .iter()
                .map(|(m, n)| format!("  {m} → {n}"))
                .collect::<Vec<_>>()
                .join("\n");
            Err(CliError::ConfigDiagnostic(format!(
                "{}: cannot pick a project name — {} ecosystem manifests name this \
                 directory:\n{}\n\n\
                 Set `[project].name` in {} to say which one ForgeDB should use.",
                root.display(),
                detected.len(),
                list,
                root.join(CONFIG_FILE).display(),
            )))
        }
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
    Conflict { held_by: PathBuf },
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
                Ok(Claim::Conflict {
                    held_by: PathBuf::from(held.trim()),
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
pub fn held_by(name: &str) -> Result<Option<PathBuf>> {
    let path = cache::ledger_root()?.join(format!("{name}.claim"));
    match std::fs::read_to_string(&path) {
        Ok(held) => Ok(Some(PathBuf::from(held.trim()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Resolve an identity and refuse a collision.
///
/// Non-interactive by construction: a taken id is an error naming the remedy,
/// never a silently-picked alternative.  The remedy is written into the
/// project's *own* config, which is why it survives a cache wipe.
pub fn identify_and_claim(chain: &Chain) -> Result<ProjectId> {
    let id = identify(chain)?;
    if let Claim::Conflict { held_by } = claim(&id)? {
        return Err(CliError::ConfigDiagnostic(format!(
            "Project name {:?} is already claimed by {}.\n\n\
             Two projects sharing an id would share one build cache, one lockfile \
             and one target directory.\n\n\
             Set a different `[project].name` in {} — writing it there (rather than \
             in the cache) is what makes the resolution survive `rm -rf ~/.forgedb`.",
            id.name,
            held_by.display(),
            id.root.join(CONFIG_FILE).display(),
        )));
    }
    Ok(id)
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
    let chain = Chain::walk(base)?;
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
