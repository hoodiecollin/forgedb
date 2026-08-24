//! May ForgeDB ask a question, and who answers it? (#367, epic #332)
//!
//! Two project decisions cannot be settled by a flag *and made to stick*: which
//! ecosystem manifest names an ambiguous project root, and what to do when the
//! resolved id is already claimed.  A flag could express either answer — the
//! problem is that the id keys `~/.forgedb/projects/<id>/`, so an answer living
//! in one `argv` is a **different project** on the next invocation that omits
//! it, and the invocations that would omit it are the ones ForgeDB itself
//! scaffolds (the `Dockerfile`, `docker-compose.yml`, CI).  So the deliverable
//! is a *persisting act* — [`crate::commands::project`] — and a prompt is one
//! front end to it.
//!
//! This module owns the front end, in two strictly separated halves:
//!
//! * **The decision** — [`Askability`], a pure predicate over four booleans.
//!   Its truth table is a unit test, not something only a terminal can run.
//! * **The widget** — [`TerminalAsk`], reached *only* through [`asker`], and
//!   only past [`Askability::may_ask`].
//!
//! [`Asker`] sits between them.  That seam is why the interactive path has a
//! test at all: the existing harness drives `forgedb` as a subprocess with
//! piped stdio, so by construction it can never execute a branch that needs a
//! terminal.  A test-local `Asker` executes the *decision* and the *act* with
//! no pty anywhere.
//!
//! **Questions render to stderr**, deliberately unlike [`crate::ui`], which
//! writes to stdout by design (its warnings are part of `validate`'s report).
//! A question is the same class of thing as an error: it must not enter a
//! captured stdout, and `forgedb generate > build.log` must still show it.

use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Result;
use crate::project::{Answer, Asker, Question};

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

/// The asker for this invocation: a real prompt at a terminal, [`NeverAsk`]
/// anywhere else.
///
/// **The only constructor of [`TerminalAsk`]** — a structural guard asserts
/// that, so a future call site cannot reach the widget past the boundary.
pub fn asker() -> Box<dyn Asker> {
    let askability = Askability::detect();
    trace(askability.reason());
    if askability.may_ask() {
        Box::new(TerminalAsk)
    } else {
        Box::new(NeverAsk)
    }
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

/// The non-interactive asker: answers nothing, consents to nothing.
///
/// `ask` returning `Ok(None)` is what makes "cannot ask" and "declined" the
/// **same** path — today's diagnostic, today's exit status. Declining must not
/// be a third behaviour.
pub struct NeverAsk;

impl Asker for NeverAsk {
    fn ask(&self, _q: &Question) -> Result<Option<Answer>> {
        Ok(None)
    }
    fn confirm_edit(&self, _path: &Path) -> Result<bool> {
        Ok(false)
    }
}

/// The asker for `forgedb project …`: still asks nothing, but **consents** to
/// editing a config ForgeDB did not author.
///
/// Typing the command *is* the in-session confirmation the accepted design
/// requires, which is why consent is a property of the asker rather than a flag:
/// the same [`crate::project::record_name`] call is a refusal when a `generate`
/// reaches it and an authorised edit when the user asked for it by name.
pub struct CommandConsent;

impl Asker for CommandConsent {
    fn ask(&self, _q: &Question) -> Result<Option<Answer>> {
        Ok(None)
    }
    fn confirm_edit(&self, _path: &Path) -> Result<bool> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// The widget
// ---------------------------------------------------------------------------

/// The terminal prompt. **Everything renders on stderr.**
///
/// Deliberately unlike [`crate::ui`], which writes to stdout by design (its
/// warnings are part of `validate`'s report). A question is the same class of
/// thing as an error: it must not enter a captured stdout, and
/// `forgedb generate > build.log` must still put it in front of the person
/// running it.
///
/// This type is reached only through [`asker`], and only past
/// [`Askability::may_ask`] — so nothing here has to re-check whether asking is
/// allowed, and nothing here may be constructed by a caller that skipped the
/// check.
pub struct TerminalAsk;

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

/// The trailing item on every select: type something that is not on the list.
const OTHER: &str = "Enter a different name…";
const CANCEL: &str = "Cancel (leave it unresolved)";

impl Asker for TerminalAsk {
    fn ask(&self, q: &Question) -> Result<Option<Answer>> {
        let term = dialoguer::console::Term::stderr();
        let theme = dialoguer::theme::ColorfulTheme::default();

        match q {
            Question::WhichName {
                root, candidates, ..
            } => {
                // The computed facts go in FRONT of the question. Which manifest
                // a name came from is the whole basis for choosing between them,
                // and a bare list of names does not carry it.
                let mut items: Vec<String> = candidates
                    .iter()
                    .map(|(manifest, name)| format!("{name}   (from {manifest})"))
                    .collect();
                items.push(OTHER.to_string());

                let _ = term.write_line(&format!(
                    "\n{} names this directory in {} ecosystem manifests, and they \
                     disagree.\nPicking one silently would key a build cache on a \
                     guess you never saw.",
                    root.display(),
                    candidates.len()
                ));

                let Some(picked) = cancelled(
                    dialoguer::Select::with_theme(&theme)
                        .with_prompt("Which name is this project's?")
                        .items(&items)
                        .default(0)
                        .interact_on_opt(&term),
                )?
                else {
                    return Ok(None);
                };

                let name = match candidates.get(picked) {
                    Some((_, name)) => name.clone(),
                    None => match free_text(&term, &theme)? {
                        Some(n) => n,
                        None => return Ok(None),
                    },
                };
                Ok(Some(Answer::Name(name)))
            }

            Question::Collision {
                id,
                held_by,
                holder_exists,
                ..
            } => {
                let _ = term.write_line(&format!(
                    "\nThe project id {id:?} is already claimed by {}.",
                    held_by.display()
                ));

                // The offered ANSWERS differ by liveness, which is exactly why
                // this decision cannot be a flag: whether the holding root still
                // exists is a fact about the filesystem the user cannot know
                // when they type the command.
                let mut items = Vec::new();
                if !*holder_exists {
                    let _ = term.write_line(
                        "That path no longer exists. Nothing removes a claim, so this \
                         is very likely this project colliding with its own record — \
                         but a missing path can also mean an unmounted volume, which \
                         is exactly when taking the id over would be wrong.",
                    );
                    items.push("Take over the claim (keep this project's name)".to_string());
                } else {
                    let _ = term.write_line(
                        "That path still exists, so this is a real collision: two \
                         projects sharing an id would share one build cache, one \
                         lockfile and one target directory.",
                    );
                }
                items.push(OTHER.to_string());
                items.push(CANCEL.to_string());

                let Some(picked) = cancelled(
                    dialoguer::Select::with_theme(&theme)
                        .with_prompt("How should this be resolved?")
                        .items(&items)
                        .default(0)
                        .interact_on_opt(&term),
                )?
                else {
                    return Ok(None);
                };

                match items[picked].as_str() {
                    CANCEL => Ok(None),
                    OTHER => Ok(free_text(&term, &theme)?.map(Answer::Name)),
                    _ => Ok(Some(Answer::TakeOverClaim)),
                }
            }
        }
    }

    fn confirm_edit(&self, path: &Path) -> Result<bool> {
        let term = dialoguer::console::Term::stderr();
        let theme = dialoguer::theme::ColorfulTheme::default();
        let _ = term.write_line(&format!(
            "\n{} already exists, and ForgeDB did not write it.",
            path.display()
        ));
        // Defaulting to NO. A config in someone else's repository is not a file
        // to edit on a keystroke that was aimed at the previous question.
        Ok(cancelled(
            dialoguer::Confirm::with_theme(&theme)
                .with_prompt("Record `[project].name` there, preserving its formatting?")
                .default(false)
                .interact_on_opt(&term),
        )?
        .unwrap_or(false))
    }
}

fn free_text(
    term: &dialoguer::console::Term,
    theme: &dialoguer::theme::ColorfulTheme,
) -> Result<Option<String>> {
    let typed: String = dialoguer::Input::with_theme(theme)
        .with_prompt("Project name")
        .interact_text_on(term)
        .unwrap_or_default();
    let typed = typed.trim().to_string();
    Ok((!typed.is_empty()).then_some(typed))
}
