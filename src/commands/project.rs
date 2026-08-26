//! `forgedb project show` — report every fact identity is derived from (#367, #479).
//!
//! **Reports, decides nothing.** It must work in precisely the cases identity
//! resolution does *not*, which is why it reads each fact separately rather than
//! collapsing them to one answer: a project whose id cannot be resolved is
//! exactly the project someone is running this against.
//!
//! It is also the only non-mutating window onto the claim ledger, which is what
//! keeps the scenarios from reading the ledger's file layout directly — a second
//! derivation of something `cache.rs` owns.
//!
//! # What used to be here (#479)
//!
//! Three sibling commands recorded identity decisions: `project name` persisted
//! a chosen `[project].name`, `project claim --take-over` displaced a stale
//! ledger holder, and `project release` dropped a claim. All three existed
//! because the id was *derived* from a package name and could therefore collide
//! or go stale. `forgedb init` now mints it, so there is no decision left to
//! record and no ghost left to displace.

use crate::error::CliError;
use crate::project::{self, Chain, IdSource};
use crate::{cache, ui, Result};

pub enum ProjectCommand {
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
        ProjectCommand::Show => show(&chain, &schema),
    }
}

/// Report the facts, **without collapsing them to one answer**.
fn show(chain: &Chain, schema: &std::path::Path) -> Result<()> {
    let root = chain.root_dir();
    ui::info(&format!("Project root: {}", root.display()));
    // Which app answered. `-s/--schema` selects the chain, so two correct-looking
    // reports in one monorepo differ only by this line.
    ui::info(&format!("Schema:       {}", schema.display()));
    match chain.project_root() {
        Some(link) => ui::info(&format!("Config:       {}", link.path.display())),
        None => ui::info(&format!(
            "Config:       none (would be created at {})",
            root.join(crate::config::CONFIG_FILE).display()
        )),
    }

    // `identify` refuses only a nested, non-root `[project].id` — a
    // contradiction this command should report rather than resolve.
    let id = match project::identify(chain) {
        Ok(id) => id,
        Err(CliError::ConfigDiagnostic(msg)) => {
            ui::warning("Id:           UNRESOLVED");
            for line in msg.lines() {
                ui::info(&format!("              {line}"));
            }
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    match id.source {
        IdSource::Declared => ui::info(&format!("Id:           {} (from [project].id)", id.name)),
        IdSource::PathHash => ui::warning(&format!(
            "Id:           {} — derived from the path of {}, because no \
             `[project].id` names it. It changes if the directory moves.",
            id.name,
            id.root.display()
        )),
    }

    // The directory the id actually keys. Without this line the only way to find
    // a project's build cache is to know the layout.
    ui::info(&format!(
        "Cache:        {}",
        cache::project_dir(&id.name)?.display()
    ));

    match project::held_by(&id.name)? {
        None => ui::info("Claim:        unclaimed"),
        Some(holder) if holder.path == root => ui::info("Claim:        held by this project"),
        Some(holder) => ui::warning(&format!(
            "Claim:        held by {}{} — two projects carry this id, which \
             happens when a project directory is copied. Give one of them an id \
             of its own.",
            holder.path.display(),
            if holder.exists {
                ""
            } else {
                ", a path that no longer exists"
            },
        )),
    }
    Ok(())
}
