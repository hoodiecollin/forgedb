use std::path::{Component, Path, PathBuf};

use crate::cache;
use crate::config::{self, ForgeConfig, CONFIG_FILE};
use crate::error::{CliError, Result};

pub const SCHEMA_CANDIDATES: [&str; 3] = ["schema.forge", "schema.lang", "schema.forgedb"];

const DEFAULT_OUTPUT: &str = "generated";

#[derive(Debug)]
pub struct Link {
    pub dir: PathBuf,
    pub path: PathBuf,
    pub config: ForgeConfig,
    id_pos: Option<(usize, usize)>,
}

#[derive(Debug)]
pub struct Chain {
    pub start: PathBuf,
    links: Vec<Link>,
    schema: Option<PathBuf>,
}

impl Chain {
    pub fn walk(from: &Path) -> Result<Chain> {
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

    pub fn walk_from_schema(schema: &Path) -> Result<Chain> {
        let mut chain = Chain::walk(schema_dir(schema))?;
        chain.schema = Some(schema.to_path_buf());
        Ok(chain)
    }

    pub fn schema(&self) -> Option<&Path> {
        self.schema.as_deref()
    }

    pub fn nearest(&self) -> Option<&Link> {
        self.links.first()
    }

    pub fn into_nearest_config(mut self) -> Option<ForgeConfig> {
        if self.links.is_empty() {
            None
        } else {
            Some(self.links.remove(0).config)
        }
    }

    pub fn project_root(&self) -> Option<&Link> {
        self.links
            .iter()
            .find(|l| l.config.project.isolated)
            .or_else(|| self.links.last())
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSource {
    Declared,
    PathHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectId {
    pub name: String,
    pub root: PathBuf,
    pub source: IdSource,
}

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

pub fn identify(chain: &Chain) -> Result<ProjectId> {
    let root_link = chain.project_root();
    let root = chain.root_dir();

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Fresh,
    Ours,
    Conflict {
        held_by: PathBuf,
        holder_exists: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub path: PathBuf,
    pub exists: bool,
}

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

pub fn schema_dir(schema: &Path) -> &Path {
    match schema.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

pub fn migrations_dir(schema: &Path) -> PathBuf {
    schema_dir(schema).join("migrations")
}

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

#[derive(Debug)]
pub struct Governing {
    pub base: PathBuf,
    pub chain: Chain,
    explicit: Option<ForgeConfig>,
    fallback: ForgeConfig,
}

impl Governing {
    pub fn config(&self) -> &ForgeConfig {
        self.explicit
            .as_ref()
            .or_else(|| self.chain.nearest().map(|l| &l.config))
            .unwrap_or(&self.fallback)
    }

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

    pub fn symbol_naming(&self) -> crate::naming::SymbolNaming {
        self.chain
            .project_root()
            .map(|l| l.config.project.symbol_naming)
            .unwrap_or_default()
    }

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

    pub fn rust_package(&self) -> Option<PathBuf> {
        self.config()
            .placement
            .rust_package
            .as_deref()
            .map(|declared| self.resolve_path(declared))
    }

    pub fn identify(&self) -> Result<ProjectId> {
        identify_and_claim(&self.chain)
    }

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

pub fn govern(explicit_config: Option<&str>, base: &Path) -> Result<Governing> {
    govern_chain(explicit_config, base, Chain::walk(base)?)
}

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

const NOT_A_SCHEMA_DIR: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".forgedb",
    ".venv",
    "vendor",
];

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
