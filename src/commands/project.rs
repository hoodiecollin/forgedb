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
use crate::error::CliError;
use crate::project::{self, Chain, Identified};
use crate::{ui, Result};

pub enum ProjectCommand {
    /// Persist `[project].name` at the project root.
    Name { name: String, force: bool },
    /// Take this project's id over from the root the ledger names.
    Claim { take_over: bool, force: bool },
    /// Drop this project's own claim.
    Release,
    /// Report every fact identity is derived from, deciding nothing.
    Show,
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
        ProjectCommand::Claim { take_over, force } => claim_cmd(&chain, take_over, force),
        ProjectCommand::Release => release_cmd(&chain),
        ProjectCommand::Show => show(&chain),
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

/// `forgedb project claim --take-over` — the answer to the *common* instance of
/// the collision decision.
///
/// Nothing anywhere removes a `.claim`, so a project that was moved, renamed or
/// deleted collides with its own record and today's diagnostic tells it to
/// rename itself. That is the wrong remedy: the right one is to release a dead
/// claim, and the project keeps its name.
fn claim_cmd(chain: &Chain, take_over: bool, force: bool) -> Result<()> {
    let id = project::identify(chain)?;
    if !take_over {
        return Err(CliError::Config(
            "`forgedb project claim` needs --take-over to say what it should do.\n\n\
             Claiming happens on its own as part of `generate`/`build`; this \
             command exists to take an id over from a root that no longer holds \
             it. `forgedb project show` reports who does."
                .to_string(),
        ));
    }
    match project::take_over_claim(&id, force)? {
        None => ui::info(&format!("Project {:?} was already ours.", id.name)),
        Some(previous) => {
            ui::success(&format!("Project {:?} now points at {}", id.name, id.root.display()));
            // Named rather than summarised: displacing a LIVE holder is a real
            // consequence, and "which root did I just displace" is not
            // recoverable afterwards — the ledger holds one path.
            ui::info(&format!(
                "Displaced {}{}",
                previous.path.display(),
                if previous.exists {
                    " (which still exists)"
                } else {
                    " (which no longer exists)"
                }
            ));
        }
    }
    Ok(())
}

fn release_cmd(chain: &Chain) -> Result<()> {
    let id = project::identify(chain)?;
    if project::release_claim(&id)? {
        ui::success(&format!("Released the claim on {:?}", id.name));
    } else {
        ui::info(&format!("Nothing to release: {:?} is unclaimed.", id.name));
    }
    Ok(())
}

/// Report the facts, **without collapsing them to one answer**.
///
/// `show` must work in precisely the cases identity does not, so it reports an
/// ambiguity as a list rather than resolving it — otherwise it errors in the one
/// situation it is most wanted. It is also the only non-mutating way to see the
/// claim holder and its liveness, which is what keeps the scenarios from reading
/// the ledger's file layout directly (a second derivation of something
/// `cache.rs` owns).
fn show(chain: &Chain) -> Result<()> {
    let root = chain.root_dir();
    ui::info(&format!("Project root: {}", root.display()));
    match chain.project_root() {
        Some(link) => ui::info(&format!("Config:       {}", link.path.display())),
        None => ui::info(&format!(
            "Config:       none (would be created at {})",
            root.join(crate::config::CONFIG_FILE).display()
        )),
    }

    let id = match project::identify_or_ask(chain)? {
        Identified::Resolved(id) => {
            ui::info(&format!("Id:           {} ({:?})", id.name, id.source));
            Some(id.name)
        }
        Identified::Ambiguous { candidates, .. } => {
            ui::warning(&format!(
                "Id:           AMBIGUOUS — {} ecosystem manifests name this directory:",
                candidates.len()
            ));
            for (manifest, name) in &candidates {
                ui::info(&format!("                {manifest} -> {name}"));
            }
            ui::info("Record one with: forgedb project name <NAME>");
            None
        }
    };

    let Some(name) = id else { return Ok(()) };
    match project::held_by(&name)? {
        None => ui::info("Claim:        unclaimed"),
        Some(holder) if holder.path == root => ui::info("Claim:        held by this project"),
        Some(holder) if holder.exists => ui::warning(&format!(
            "Claim:        held by {} — which still exists, so this is a real \
             collision. Set a different `[project].name` here.",
            holder.path.display()
        )),
        Some(holder) => ui::warning(&format!(
            "Claim:        held by {} — that path no longer exists. \
             `forgedb project claim --take-over` takes the id back.",
            holder.path.display()
        )),
    }
    Ok(())
}
