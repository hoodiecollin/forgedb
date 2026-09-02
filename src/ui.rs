use colored::Colorize;
use std::sync::atomic::{AtomicU8, Ordering};

const QUIET: u8 = 0;
const NORMAL: u8 = 1;
const VERBOSE: u8 = 2;
static LEVEL: AtomicU8 = AtomicU8::new(NORMAL);

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

pub fn is_quiet() -> bool {
    level() == QUIET
}

pub fn success(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "✓".green().bold(), msg);
    }
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

pub fn warning(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "⚠".yellow().bold(), msg);
    }
}

pub fn warning_diagnostic(msg: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), msg);
}

pub fn info(msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", "ℹ".blue().bold(), msg);
    }
}

pub fn detail(msg: &str) {
    if level() >= VERBOSE {
        println!("{} {}", "·".dimmed(), msg.dimmed());
    }
}

pub fn step(emoji: &str, msg: &str) {
    if level() >= NORMAL {
        println!("{} {}", emoji, msg);
    }
}

pub fn blank() {
    if level() >= NORMAL {
        println!();
    }
}

pub fn line(msg: &str) {
    if level() >= NORMAL {
        println!("{}", msg);
    }
}

pub fn header(emoji: &str, msg: &str) {
    if level() >= NORMAL {
        println!("\n{} {}\n", emoji, msg.bold());
    }
}
