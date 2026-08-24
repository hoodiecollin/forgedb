//! `forgedb project` — record the two decisions a flag cannot make stick (#367).
//!
//! **This is the contract; a prompt is sugar over it.** #340 §8 made flags the
//! contract for the one project decision that is complete as a flag
//! (`--isolated`); these two are not, because the id keys
//! `~/.forgedb/projects/<id>/` and an answer living in one `argv` is a
//! *different project* on the next invocation that omits it — and the
//! invocations that omit it are the ones ForgeDB itself scaffolds (the
//! `Dockerfile`, `docker-compose.yml`, the reclose workflows). So the answer has
//! to persist, and a command that persists it buys four things a hidden write
//! does not: it can be named inside the non-interactive error, it can be run in
//! CI, it produces a reviewable diff, and it is testable with no pty.
//!
//! Nothing here calls [`crate::project::identify`]. That function errors in
//! exactly the situation this command exists to resolve, so it would refuse to
//! run at the moment it is most wanted.

use crate::ask::CommandConsent;
use crate::project::{self, Chain};
use crate::{ui, Result};

pub enum ProjectCommand {
    /// Persist `[project].name` at the project root.
    Name { name: String, force: bool },
}

pub struct ProjectOptions {
    pub command: ProjectCommand,
    /// The app this is about.
    ///
    /// Resolved **exactly** as `generate`/`build` resolve it, because identity
    /// is keyed on the schema's chain. Walking from the CWD would let this
    /// command resolve a different root than the `generate` whose error printed
    /// it — silently, and in the monorepo case that motivates the whole issue.
    pub schema: Option<String>,
}

pub fn run(options: ProjectOptions) -> Result<()> {
    let schema = project::find_schema(options.schema.as_deref())?;
    let chain = Chain::walk_from_schema(&schema)?;

    match options.command {
        ProjectCommand::Name { name, force } => name_cmd(&chain, &name, force),
    }
}

fn name_cmd(chain: &Chain, name: &str, force: bool) -> Result<()> {
    // The id the cache is keyed on *right now*, captured before the write, so a
    // `--force` rename can name the directory it orphans. Best-effort: the whole
    // point of this command is that identity may not resolve at all.
    let before = project::identify(chain).ok();

    let recorded = project::record_name(chain, name, force, &CommandConsent)?;

    if recorded.created {
        ui::success(&format!("Created {}", recorded.path.display()));
    } else {
        ui::success(&format!("Updated {}", recorded.path.display()));
    }
    ui::info(&format!("Project: {name}"));

    // C7's rule, applied to a rename: a build cache the user has never seen is
    // exactly the kind of thing that should not be orphaned silently.
    if let Some(old) = before.filter(|b| b.name != name)
        && let Ok(dir) = crate::cache::project_dir(&old.name)
        && dir.exists()
    {
        ui::warning(&format!(
            "The previous id was {:?}; its build cache at {} is now orphaned and \
             can be deleted.",
            old.name,
            dir.display()
        ));
    }
    Ok(())
}
