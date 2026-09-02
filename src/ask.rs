use std::io::IsTerminal;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Result;

static FORBIDDEN: AtomicBool = AtomicBool::new(false);

const TRACE_ENV: &str = "FORGEDB_ASK_TRACE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Askability {
    pub stdin_tty: bool,
    pub stderr_tty: bool,
    pub quiet: bool,
    pub forbidden: bool,
}

impl Askability {
    pub fn may_ask(&self) -> bool {
        self.stdin_tty && self.stderr_tty && !self.quiet && !self.forbidden
    }

    pub fn detect() -> Askability {
        Askability {
            stdin_tty: std::io::stdin().is_terminal(),
            stderr_tty: std::io::stderr().is_terminal(),
            quiet: crate::ui::is_quiet(),
            forbidden: is_forbidden(),
        }
    }

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

pub fn forbid() {
    FORBIDDEN.store(true, Ordering::Relaxed);
    trace("forbid");
}

pub fn is_forbidden() -> bool {
    FORBIDDEN.load(Ordering::Relaxed)
}

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

fn cancelled<T>(r: std::result::Result<Option<T>, dialoguer::Error>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(v),
        Err(dialoguer::Error::IO(e)) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(dialoguer::Error::IO(e)) => Err(e.into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Index(usize),
    Free(String),
}

pub trait Prompt {
    fn select(&mut self, question: &str, options: &[String], free: Option<&str>)
    -> Result<Choice>;

    fn confirm(&mut self, question: &str) -> Result<bool>;
}

pub fn prompt() -> Option<Box<dyn Prompt>> {
    let askability = Askability::detect();
    trace(askability.reason());
    askability
        .may_ask()
        .then(|| Box::new(TerminalPrompt) as Box<dyn Prompt>)
}

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

const FREE_TEXT: &str = "Type a value instead…";

pub struct ScriptedPrompt {
    answers: std::collections::VecDeque<String>,
    pub asked: Vec<String>,
}

impl ScriptedPrompt {
    pub fn new<I: IntoIterator<Item = S>, S: Into<String>>(answers: I) -> Self {
        Self {
            answers: answers.into_iter().map(Into::into).collect(),
            asked: Vec::new(),
        }
    }

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
