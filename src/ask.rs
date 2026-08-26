//! May ForgeDB ask a question, and who answers it? (#367, epic #332)
//!
//! `migrate create` reaches changes the schema differ cannot prove a value for —
//! a required field added with no `@default`, a field whose values have to come
//! from somewhere. Only the operator knows the answer, so ForgeDB has to ask,
//! and it has to ask *safely*: a prompt fired from a CI job or a watch loop is a
//! hang, not a question.
//!
//! This module owns that, in two strictly separated halves:
//!
//! * **The decision** — [`Askability`], a pure predicate over four booleans
//!   (stdin is a terminal, stderr is a terminal, not `--quiet`, not [`forbid`]-ed).
//!   Its truth table is a unit test, not something only a terminal can run.
//! * **The widget** — [`TerminalPrompt`], reached *only* through [`prompt`], and
//!   only past [`Askability::may_ask`].
//!
//! [`Prompt`] sits between them. That seam is why the interactive path has a
//! test at all: the existing harness drives `forgedb` as a subprocess with piped
//! stdio, so by construction it can never execute a branch that needs a
//! terminal. [`ScriptedPrompt`] executes the *decision* and the *act* with no
//! pty anywhere.
//!
//! **Questions render to stderr**, deliberately unlike [`crate::ui`], which
//! writes to stdout by design (its warnings are part of `validate`'s report).
//! A question is the same class of thing as an error: it must not enter a
//! captured stdout, and `forgedb generate > build.log` must still show it.
//!
//! [`Askability`] is the ONE definition of "may ForgeDB ask", and
//! `s19b_terminal_detection_has_one_definition` enforces that by refusing a
//! second `is_terminal()` anywhere in `src/`.
//!
//! # What used to be here (#479)
//!
//! A second trait, `Asker`, asked two questions about **project identity**:
//! which ecosystem manifest names an ambiguous root, and what to do when the
//! resolved id is already claimed. Both existed because the id was *derived*.
//! Once `forgedb init` mints it instead, neither question has a subject — there
//! is nothing to disambiguate and nothing to contest — so the trait, its three
//! implementations and the `asker()` boundary went with them. The predicate
//! stayed; only one of its two consumers did.

use std::io::IsTerminal;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Result;


/// Set once, by a command that knows its own stdout or its own control flow
/// makes a question wrong regardless of what the terminal looks like.
static FORBIDDEN: AtomicBool = AtomicBool::new(false);

/// Append the reason [`asker`] decided on to `$FORGEDB_ASK_TRACE`.
///
/// A file rather than a stream, and shipped rather than test-only, because it
/// is the **only** mechanism by which a piped-stdio harness can tell "did not
/// ask because forbidden" from "did not ask because piped".  Without it, a test
/// asserting `forbid()`'s effect passes with the `forbid()` *call* deleted —
/// the `Build` arm's pre-existing `set_verbosity(false, true)` already
/// satisfies the quiet clause on its own, so the outcome is identical and only
/// the reason differs.  Proving the function works is not proving the call site
/// runs.  Precedent: `FORGEDB_HOME`.
const TRACE_ENV: &str = "FORGEDB_ASK_TRACE";

/// Whether a question may be asked, decomposed so the answer is a **pure
/// function** of four observations rather than an `IsTerminal` call scattered
/// through every call site.
///
/// All four clauses are needed and they are needed for *different* reasons; a
/// later simplification must not collapse them:
///
/// * `stdin_tty` — there is someone to answer.  This is the clause that stops
///   a `docker build` from blocking forever: `--print-artifact` deadlocks on a
///   prompt *reading stdin*, not on one writing stderr.
/// * `stderr_tty` — the question is visible.  A question nobody can see is
///   worse than an error, because it hangs instead of failing.
/// * `quiet` — `--quiet` plus a missing answer takes the error path.
/// * `forbidden` — machine-readable stdout, a watch loop, or a protocol host,
///   any of which is wrong to ask from *however* the process was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Askability {
    /// Is stdin a terminal — is there someone to answer?
    pub stdin_tty: bool,
    /// Is stderr a terminal — can the question be seen?
    pub stderr_tty: bool,
    /// Is `--quiet` set?
    pub quiet: bool,
    /// Has a command latched [`forbid`]?
    pub forbidden: bool,
}

impl Askability {
    /// The whole predicate.  Every clause is a veto.
    pub fn may_ask(&self) -> bool {
        self.stdin_tty && self.stderr_tty && !self.quiet && !self.forbidden
    }

    /// Observe the process.  **The only [`IsTerminal`] call in the tree.**
    pub fn detect() -> Askability {
        Askability {
            stdin_tty: std::io::stdin().is_terminal(),
            stderr_tty: std::io::stderr().is_terminal(),
            quiet: crate::ui::is_quiet(),
            forbidden: is_forbidden(),
        }
    }

    /// Why it decided what it decided — the first failing clause, in a fixed
    /// order, or `"terminal"` when nothing failed.
    ///
    /// The order is `forbidden` → `quiet` → `stdin` → `stderr`, most
    /// deliberate cause first: a forbidden context is a decision ForgeDB made
    /// about itself and is the more useful thing to report when several clauses
    /// happen to fail together.
    pub fn reason(&self) -> &'static str {
        if self.forbidden {
            "forbidden"
        } else if self.quiet {
            "quiet"
        } else if !self.stdin_tty {
            "no-stdin-tty"
        } else if !self.stderr_tty {
            "no-stderr-tty"
        } else {
            "terminal"
        }
    }
}

/// Latch "never ask, whatever the terminal looks like" for this process.
///
/// Idempotent, and one-way on purpose: every caller is a context that stays
/// wrong to ask from for the rest of the invocation — a `$(…)`-captured stdout,
/// a watch loop whose terminal is showing watch output, a JSON-RPC host that
/// owns stdin.  An un-forbid would be a way to reintroduce exactly those hangs.
pub fn forbid() {
    FORBIDDEN.store(true, Ordering::Relaxed);
    // Traced as its own event, so a harness can see the CALL happen and not
    // merely infer it from a decision that another clause would have reached
    // anyway.
    trace("forbid");
}

/// Whether [`forbid`] has been called.
pub fn is_forbidden() -> bool {
    FORBIDDEN.load(Ordering::Relaxed)
}

/// Append one reason line to `$FORGEDB_ASK_TRACE`, when it is set.
///
/// Failure is deliberately silent: a trace is an observation channel, and a
/// broken one must not fail an invocation that would otherwise succeed.
fn trace(reason: &str) {
    let Some(path) = std::env::var_os(TRACE_ENV) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{reason}");
    }
}

/// A cancelled prompt is a **decline**, not a distinct outcome.
///
/// `ESC` returns `Ok(None)`; `^C` returns an `Interrupted` io error after
/// dialoguer has restored the terminal. Both mean "no answer", which is the same
/// path a pipe takes: the unchanged diagnostic and the unchanged exit status.
/// A real I/O failure is not swallowed.
fn cancelled<T>(r: std::result::Result<Option<T>, dialoguer::Error>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(v),
        Err(dialoguer::Error::IO(e)) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(dialoguer::Error::IO(e)) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// Asking: pick one of N, or yes/no (#374)
// ---------------------------------------------------------------------------
//
// #479 removed the other question kind. Identity used to ask two — "which of
// these manifest names is the project?" and "the holder is gone, take the id
// over?" — and both stopped existing when the id became something `init` mints
// rather than something ForgeDB derives and then has to negotiate. What remains
// is the migration prompt, which asks about values only the operator knows.

/// What an operator picked from a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The index of one of the offered options.
    Index(usize),
    /// Free text, when the menu offered an escape for it.
    Free(String),
}

/// A menu and a yes/no, for the questions `migrate create` asks (#374).
///
/// The one question kind, since #479 removed the identity questions. It was the
/// *second* of two, deliberately separate because the other trait's question was
/// a closed enum about project identity and widening it would have put migration
/// questions in `project.rs`. What the two shared, and what still matters, is the
/// *boundary* — see [`prompt`].
///
/// The seam is also what gives the interactive path a test at all: the migrate
/// harness drives `forgedb` as a subprocess with piped stdio, so by
/// construction it can only ever walk the non-interactive branch.
/// [`ScriptedPrompt`] executes the other one with no pty anywhere.
pub trait Prompt {
    /// Offer `options` by number. `free` is the label for a free-text answer
    /// when the menu admits one (`None` means it does not).
    fn select(&mut self, question: &str, options: &[String], free: Option<&str>)
    -> Result<Choice>;

    /// A yes/no question. Deliberately has **no default**: a `[Y/n]` whose
    /// Enter key silently means yes is how an operator agrees to something they
    /// did not read.
    fn confirm(&mut self, question: &str) -> Result<bool>;
}

/// The prompt for this invocation, or `None` when asking is not allowed.
///
/// The **only constructor of [`TerminalPrompt`]**, and it is gated by
/// [`Askability`] — the one definition of whether ForgeDB may ask at all.
/// `None` rather than a never-asking implementation, because #374's caller has
/// to say *why* it could not ask, in the error it raises instead.
pub fn prompt() -> Option<Box<dyn Prompt>> {
    let askability = Askability::detect();
    trace(askability.reason());
    askability
        .may_ask()
        .then(|| Box::new(TerminalPrompt) as Box<dyn Prompt>)
}

/// The terminal menu. Everything renders on **stderr**, never stdout.
///
/// Reached only through [`prompt`], and only past [`Askability::may_ask`] — so
/// nothing here re-checks whether asking is allowed, and nothing here may be
/// constructed by a caller that skipped the check.
pub struct TerminalPrompt;

impl Prompt for TerminalPrompt {
    fn select(
        &mut self,
        question: &str,
        options: &[String],
        free: Option<&str>,
    ) -> Result<Choice> {
        let term = dialoguer::console::Term::stderr();
        let theme = dialoguer::theme::ColorfulTheme::default();

        // A menu with no options is the free-text form: there is nothing to
        // pick between, only something to type.
        if options.is_empty() {
            let label = free.unwrap_or("value");
            let typed: String = dialoguer::Input::with_theme(&theme)
                .with_prompt(format!("{question}\n{label}"))
                .interact_text_on(&term)
                .unwrap_or_default();
            return Ok(Choice::Free(typed.trim().to_string()));
        }

        let mut items: Vec<String> = options.to_vec();
        if free.is_some() {
            items.push(FREE_TEXT.to_string());
        }
        let picked = cancelled(
            dialoguer::Select::with_theme(&theme)
                .with_prompt(question)
                .items(&items)
                .default(0)
                .interact_on_opt(&term),
        )?
        // A cancelled menu is the first option rather than a third outcome:
        // every menu #374 raises is answered before anything is written, and a
        // "no answer" here would be indistinguishable from a non-interactive
        // session that is supposed to have errored already.
        .unwrap_or(0);

        if free.is_some() && picked == items.len() - 1 {
            let label = free.unwrap_or("value");
            let typed: String = dialoguer::Input::with_theme(&theme)
                .with_prompt(label)
                .interact_text_on(&term)
                .unwrap_or_default();
            return Ok(Choice::Free(typed.trim().to_string()));
        }
        Ok(Choice::Index(picked))
    }

    fn confirm(&mut self, question: &str) -> Result<bool> {
        let term = dialoguer::console::Term::stderr();
        let theme = dialoguer::theme::ColorfulTheme::default();
        Ok(cancelled(
            dialoguer::Confirm::with_theme(&theme)
                .with_prompt(question)
                .default(false)
                .interact_on_opt(&term),
        )?
        .unwrap_or(false))
    }
}

/// The trailing item on a menu that admits free text.
const FREE_TEXT: &str = "Type a value instead…";

/// A scripted operator, for tests.
///
/// This is why #374's interactive path has tests at all. An exhausted script is
/// an **error**, never a default: a test whose script ran out has asked a
/// question it did not expect, and answering it silently would hide exactly
/// that.
pub struct ScriptedPrompt {
    answers: std::collections::VecDeque<String>,
    /// Every question asked, in order — so a test can assert on what the
    /// operator was shown, not only on what came out the far end.
    pub asked: Vec<String>,
}

impl ScriptedPrompt {
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

    fn next(&mut self, question: &str) -> Result<String> {
        match self.answers.pop_front() {
            Some(a) => Ok(a),
            None => Err(std::io::Error::other(format!(
                "the scripted operator ran out of answers at: {question}"
            ))
            .into()),
        }
    }
}

impl Prompt for ScriptedPrompt {
    fn select(
        &mut self,
        question: &str,
        options: &[String],
        free: Option<&str>,
    ) -> Result<Choice> {
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
        ))
        .into())
    }

    fn confirm(&mut self, question: &str) -> Result<bool> {
        self.asked.push(question.to_string());
        let answer = self.next(question)?;
        match answer.to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            other => Err(std::io::Error::other(format!(
                "scripted answer {other:?} is not y/n for: {question}"
            ))
            .into()),
        }
    }
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    /// The menu is 1-based to the operator and 0-based to the caller.
    #[test]
    fn a_scripted_index_selects_that_option() {
        let mut s = ScriptedPrompt::new(["2"]);
        let opts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(s.select("pick", &opts, None).unwrap(), Choice::Index(1));
        assert_eq!(s.asked, vec!["pick".to_string()]);
        assert!(s.is_exhausted());
    }

    #[test]
    fn free_text_is_only_accepted_where_the_menu_offers_it() {
        let opts = vec!["a".to_string()];
        assert_eq!(
            ScriptedPrompt::new(["hello"])
                .select("pick", &opts, Some("a value"))
                .unwrap(),
            Choice::Free("hello".to_string())
        );
        assert!(
            ScriptedPrompt::new(["hello"])
                .select("pick", &opts, None)
                .is_err(),
            "a menu with no free-text escape must reject free text rather than \
             quietly taking it"
        );
    }

    /// An exhausted script errors. A test whose script ran out asked a question
    /// it did not expect, and a default would hide exactly that.
    #[test]
    fn an_exhausted_script_is_an_error() {
        let err = ScriptedPrompt::new(Vec::<String>::new())
            .confirm("really?")
            .unwrap_err();
        assert!(err.to_string().contains("really?"), "{err}");
    }

    #[test]
    fn confirm_accepts_only_yes_or_no() {
        assert!(ScriptedPrompt::new(["y"]).confirm("q").unwrap());
        assert!(ScriptedPrompt::new(["yes"]).confirm("q").unwrap());
        assert!(!ScriptedPrompt::new(["n"]).confirm("q").unwrap());
        assert!(!ScriptedPrompt::new(["NO"]).confirm("q").unwrap());
        assert!(
            ScriptedPrompt::new([""]).confirm("q").is_err(),
            "Enter must not mean yes — that is how an operator agrees to \
             something they did not read"
        );
    }
}
