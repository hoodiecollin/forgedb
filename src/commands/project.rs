use crate::error::CliError;
use crate::project::{self, Chain, IdSource};
use crate::{cache, ui, Result};

pub enum ProjectCommand {
    Show,
}

pub struct ProjectOptions {
    pub command: ProjectCommand,
    pub schema: Option<String>,
}

pub fn run(options: ProjectOptions) -> Result<()> {
    let schema = project::find_schema(options.schema.as_deref())?;
    let chain = Chain::walk_from_schema(&schema)?;

    match options.command {
        ProjectCommand::Show => show(&chain, &schema),
    }
}

fn show(chain: &Chain, schema: &std::path::Path) -> Result<()> {
    let root = chain.root_dir();
    ui::info(&format!("Project root: {}", root.display()));
    ui::info(&format!("Schema:       {}", schema.display()));
    match chain.project_root() {
        Some(link) => ui::info(&format!("Config:       {}", link.path.display())),
        None => ui::info(&format!(
            "Config:       none (would be created at {})",
            root.join(crate::config::CONFIG_FILE).display()
        )),
    }

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
