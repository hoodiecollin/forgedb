use colored::Colorize;

/// Print a success message with checkmark
pub fn success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print an error message with X
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

/// Print a warning message
pub fn warning(msg: &str) {
    println!("{} {}", "⚠".yellow().bold(), msg);
}

/// Print an info message
pub fn info(msg: &str) {
    println!("{} {}", "ℹ".blue().bold(), msg);
}

/// Print a step message with emoji
pub fn step(emoji: &str, msg: &str) {
    println!("{} {}", emoji, msg);
}

/// Print a header with emoji
pub fn header(emoji: &str, msg: &str) {
    println!("\n{} {}\n", emoji, msg.bold());
}
