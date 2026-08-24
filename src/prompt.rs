//! Asking the operator a question (#374, to #370's accepted shape).
//!
//! Three layers, and the boundary between them is the point:
//!
//! 1. [`askable`] — **may I ask at all?** A pure function of three inputs, so
//!    it is testable without a terminal and so every caller agrees.
//! 2. [`Ask`] — **the two questions.** A trait, so the interactive path has
//!    tests: the subprocess harness this repo uses drives `forgedb` with piped
//!    stdio and can therefore only ever exercise the non-interactive branch.
//!    [`Scripted`] is what lets a test walk the branch a real operator walks.
//! 3. [`Tty`] — the rendering, hand-rolled over `std::io`. **Zero new crates.**
//!    A numbered menu over `read_line` satisfies every scenario #374 has;
//!    #370's open question about taking `dialoguer` (+4 crates, +5 on Windows)
//!    is untouched, and if it is later answered yes, the `Tty` implementation
//!    is swapped behind this same trait and nothing else moves.
//!
//! # Questions go to stderr
//!
//! Deliberately unlike [`crate::ui`], which writes to stdout by design. A
//! question is the same class of thing as an error: it is the tool talking to
//! the operator, not the tool producing output. A `forgedb ... > file` that
//! blocks on an invisible prompt is the failure this avoids.
//!
//! # No timeout, and no default-on-timeout
//!
//! A prompt that answers itself after N seconds records an answer nobody gave,
//! into a file whose whole purpose is to say what a human decided. If nobody is
//! there, [`askable`] already said so and the caller errors.

use std::io::{BufRead, IsTerminal, Write};

/// Whether this process may ask a question, and if not, why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Askable {
    Yes,
    /// The reason, phrased to be dropped straight into a caller's error.
    No(&'static str),
}

impl Askable {
    pub fn is_yes(&self) -> bool {
        matches!(self, Askable::Yes)
    }

    /// The reason, or `""` when asking is allowed.
    pub fn reason(&self) -> &'static str {
        match self {
            Askable::Yes => "",
            Askable::No(r) => r,
        }
    }
}

/// May this process ask? Requires **all three** (#370):
///
/// * stdin is a terminal — somebody is there to answer;
/// * stderr is a terminal — the question is visible;
/// * output is not suppressed (`--quiet`).
///
/// All three, because any one of them alone is satisfiable in a session where
/// asking is still wrong: a CI job with a tty on stderr and a pipe on stdin
/// would hang forever on `read_line`.
pub fn askable() -> Askable {
    decide(
        std::io::stdin().is_terminal(),
        std::io::stderr().is_terminal(),
        crate::ui::is_quiet(),
    )
}

/// [`askable`]'s decision, as a pure function — this is what makes the
/// boundary testable without a pty.
pub fn decide(stdin_tty: bool, stderr_tty: bool, quiet: bool) -> Askable {
    if !stdin_tty {
        return Askable::No("stdin is not a terminal, so there is nobody to answer");
    }
    if !stderr_tty {
        return Askable::No("stderr is not a terminal, so the question would not be visible");
    }
    if quiet {
        return Askable::No("--quiet suppresses output, so the question would not be visible");
    }
    Askable::Yes
}

/// What an operator picked from a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The index of one of the offered options.
    Index(usize),
    /// Free text, when the menu offered an escape for it.
    Free(String),
}

/// The two questions ForgeDB asks. Everything interactive goes through this.
pub trait Ask {
    /// Offer `options` by number. `free` is the prompt for a free-text answer
    /// when the menu admits one (`None` means it does not).
    fn select(
        &mut self,
        question: &str,
        options: &[String],
        free: Option<&str>,
    ) -> std::io::Result<Choice>;

    /// A yes/no question. Deliberately has no default: a `[Y/n]` whose Enter
    /// key silently means yes is how an operator agrees to something they did
    /// not read.
    fn confirm(&mut self, question: &str) -> std::io::Result<bool>;
}

/// The real terminal.
pub struct Tty;

impl Tty {
    fn ask_line(prompt: &str) -> std::io::Result<String> {
        let mut err = std::io::stderr();
        write!(err, "{prompt}")?;
        err.flush()?;
        let mut line = String::new();
        let n = std::io::stdin().lock().read_line(&mut line)?;
        if n == 0 {
            // EOF where a terminal was expected. Named rather than looped, or a
            // closed stdin becomes an infinite re-prompt.
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "stdin closed while waiting for an answer",
            ));
        }
        Ok(line.trim().to_string())
    }
}

impl Ask for Tty {
    fn select(
        &mut self,
        question: &str,
        options: &[String],
        free: Option<&str>,
    ) -> std::io::Result<Choice> {
        loop {
            let mut err = std::io::stderr();
            writeln!(err, "\n{question}")?;
            for (i, o) in options.iter().enumerate() {
                writeln!(err, "  {}) {o}", i + 1)?;
            }
            if let Some(hint) = free {
                writeln!(err, "  (or type {hint})")?;
            }
            let answer = Self::ask_line("> ")?;
            if let Ok(n) = answer.parse::<usize>()
                && (1..=options.len()).contains(&n)
            {
                return Ok(Choice::Index(n - 1));
            }
            if free.is_some() && !answer.is_empty() {
                return Ok(Choice::Free(answer));
            }
            writeln!(err, "Please answer with a number from 1 to {}.", options.len())?;
        }
    }

    fn confirm(&mut self, question: &str) -> std::io::Result<bool> {
        loop {
            let answer = Self::ask_line(&format!("\n{question} [y/n] "))?;
            match answer.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => {
                    let mut err = std::io::stderr();
                    writeln!(err, "Please answer y or n.")?;
                }
            }
        }
    }
}

/// A scripted operator, for tests.
///
/// This is why the interactive path has tests at all. The subprocess harness
/// drives `forgedb` with piped stdio, so it can only ever exercise the
/// non-interactive branch; the branch a real operator walks would otherwise be
/// covered by nothing.
///
/// An exhausted script is an **error**, never a default: a test whose script
/// ran out has asked a question it did not expect, and answering it silently
/// would hide exactly that.
pub struct Scripted {
    answers: std::collections::VecDeque<String>,
    /// Every question asked, in order — so a test can assert on what the
    /// operator was shown, not only on what came out the far end.
    pub asked: Vec<String>,
}

impl Scripted {
    pub fn new<I: IntoIterator<Item = S>, S: Into<String>>(answers: I) -> Self {
        Self {
            answers: answers.into_iter().map(Into::into).collect(),
            asked: Vec::new(),
        }
    }

    /// True when every scripted answer was consumed.
    pub fn is_exhausted(&self) -> bool {
        self.answers.is_empty()
    }

    fn next(&mut self, question: &str) -> std::io::Result<String> {
        self.answers.pop_front().ok_or_else(|| {
            std::io::Error::other(format!(
                "the scripted operator ran out of answers at: {question}"
            ))
        })
    }
}

impl Ask for Scripted {
    fn select(
        &mut self,
        question: &str,
        options: &[String],
        free: Option<&str>,
    ) -> std::io::Result<Choice> {
        self.asked.push(question.to_string());
        let answer = self.next(question)?;
        if let Ok(n) = answer.parse::<usize>()
            && (1..=options.len()).contains(&n)
        {
            return Ok(Choice::Index(n - 1));
        }
        if free.is_some() {
            return Ok(Choice::Free(answer));
        }
        Err(std::io::Error::other(format!(
            "scripted answer {answer:?} is not one of the {} options for: {question}",
            options.len()
        )))
    }

    fn confirm(&mut self, question: &str) -> std::io::Result<bool> {
        self.asked.push(question.to_string());
        let answer = self.next(question)?;
        match answer.to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            other => Err(std::io::Error::other(format!(
                "scripted answer {other:?} is not y/n for: {question}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All three conditions are required, and each one's absence is named.
    ///
    /// A table rather than three asserts, because the failure this guards
    /// against is one input being dropped from the conjunction — which a test
    /// of the happy path cannot see.
    #[test]
    fn asking_requires_all_three_conditions() {
        assert_eq!(decide(true, true, false), Askable::Yes);
        for (stdin, stderr, quiet, expect) in [
            (false, true, false, "stdin"),
            (true, false, false, "stderr"),
            (true, true, true, "--quiet"),
            // Absent stdin wins the diagnostic when several are absent: it is
            // the one the operator can act on.
            (false, false, true, "stdin"),
        ] {
            let d = decide(stdin, stderr, quiet);
            assert!(!d.is_yes(), "({stdin},{stderr},{quiet}) must not be askable");
            assert!(
                d.reason().contains(expect),
                "({stdin},{stderr},{quiet}) must blame {expect}: {}",
                d.reason()
            );
        }
    }

    #[test]
    fn a_scripted_index_selects_that_option() {
        let mut s = Scripted::new(["2"]);
        let opts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            s.select("pick", &opts, None).unwrap(),
            Choice::Index(1),
            "the menu is 1-based and the index is 0-based"
        );
        assert_eq!(s.asked, vec!["pick".to_string()]);
        assert!(s.is_exhausted());
    }

    #[test]
    fn free_text_is_only_accepted_where_the_menu_offers_it() {
        let opts = vec!["a".to_string()];
        assert_eq!(
            Scripted::new(["hello"])
                .select("pick", &opts, Some("a value"))
                .unwrap(),
            Choice::Free("hello".to_string())
        );
        assert!(
            Scripted::new(["hello"]).select("pick", &opts, None).is_err(),
            "a menu with no free-text escape must reject free text rather than \
             quietly taking it"
        );
    }

    /// An exhausted script errors. A test whose script ran out asked a question
    /// it did not expect, and a default would hide exactly that.
    #[test]
    fn an_exhausted_script_is_an_error() {
        let mut s = Scripted::new(Vec::<String>::new());
        let err = s.confirm("really?").unwrap_err();
        assert!(err.to_string().contains("really?"), "{err}");
    }

    #[test]
    fn confirm_accepts_only_yes_or_no() {
        assert!(Scripted::new(["y"]).confirm("q").unwrap());
        assert!(Scripted::new(["yes"]).confirm("q").unwrap());
        assert!(!Scripted::new(["n"]).confirm("q").unwrap());
        assert!(!Scripted::new(["NO"]).confirm("q").unwrap());
        assert!(
            Scripted::new([""]).confirm("q").is_err(),
            "Enter must not mean yes — that is how an operator agrees to \
             something they did not read"
        );
    }
}
