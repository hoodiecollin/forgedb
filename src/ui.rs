use colored::Colorize;
use std::sync::atomic::{AtomicU8, Ordering};

/// Output verbosity, set once from the global `-q`/`-v` flags at startup
/// (`set_verbosity`). Gates the print helpers so `--quiet` suppresses everything
/// but errors and `--verbose` unlocks `detail`.
const QUIET: u8 = 0;
const NORMAL: u8 = 1;
const VERBOSE: u8 = 2;
static LEVEL: AtomicU8 = AtomicU8::new(NORMAL);

/// Wire the global `--verbose`/`--quiet` flags into the output level. `--quiet`
/// wins if both are somehow set.
pub fn set_verbosity(verbose: bool, quiet: bool) {
    let level = if quiet {
        QUIET
    } else if verbose {
        VERBOSE
    } else {
        NORMAL
    };
    LEVEL.store(level, Ordering::Relaxed);
}

fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

/// Is output suppressed to errors only?
///
/// Exposed for [`crate::ask`], which needs the *level* rather than a printer:
/// `--quiet` plus a missing answer takes the error path, because a question
/// printed by a process the user asked to be silent is one they will not be
/// looking for. This is the only reason the level is readable at all.
pub fn is_quiet() -> bool {
    level() == QUIET
}

/// Print a success message with checkmark (suppressed by `--quiet`).
pub fn success(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "✓".green().bold(), msg);
    }
}

/// Print an error message with X (always shown, even under `--quiet`).
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

/// Print a warning message (suppressed by `--quiet`).
pub fn warning(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "⚠".yellow().bold(), msg);
    }
}

/// Print a warning to **stderr** (always shown, even under `--quiet`).
///
/// The [`warning`] above goes to stdout, which is right for the advisory lints
/// `validate` prints as part of its report. A *diagnostic* is different: it is the
/// same class of thing as [`error`] and belongs on the same stream, so that
/// `forgedb generate > build.log` still puts a deprecation in front of the person
/// running it instead of burying it in the redirect. Being hard to miss is the
/// entire point of the channel (#237).
pub fn warning_diagnostic(msg: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), msg);
}

/// Print an info message (suppressed by `--quiet`).
pub fn info(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "ℹ".blue().bold(), msg);
    }
}

/// Print a verbose-only detail line (shown only under `--verbose`).
pub fn detail(msg: &str) {
    if level() >= VERBOSE {
        println!("{} {}", "·".dimmed(), msg.dimmed());
    }
}

/// Print a step message with emoji (suppressed by `--quiet`).
pub fn step(emoji: &str, msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", emoji, msg);
    }
}

/// Print a header with emoji (suppressed by `--quiet`).
pub fn header(emoji: &str, msg: &str) {
    if level() >= NORMAL {
        println!("\n{} {}\n", emoji, msg.bold());
    }
}
