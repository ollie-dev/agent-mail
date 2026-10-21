//! The Terminal port: how `mandate create` talks to the human.
//!
//! The real adapter draws `inquire` prompts on the controlling terminal. A
//! scripted terminal for tests replays queued answers and records what was
//! shown. Only `mandate create` uses a Terminal.

use crate::mandate::Credential;

/// The human cancelled (Esc or Ctrl-C) or the terminal failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancelled;

/// Prompts and messages for the human at a terminal.
pub trait Terminal {
    /// Whether a human can be talking to this terminal.
    fn is_interactive(&self) -> bool;
    /// Asks for one line of text, offering `default` if given.
    fn text(&self, prompt: &str, default: Option<&str>) -> Result<String, Cancelled>;
    /// Asks for a Credential with hidden input: nothing is echoed or masked.
    fn hidden(&self, prompt: &str) -> Result<Credential, Cancelled>;
    /// Asks the human to pick one of `options`, with the cursor starting on
    /// `default`; returns the picked index.
    fn choose(&self, prompt: &str, options: &[String], default: usize) -> Result<usize, Cancelled>;
    /// Asks the human to tick any of `options`, with the indexes in `ticked`
    /// ticked to begin with; returns the ticked indexes in option order.
    fn checklist(
        &self,
        prompt: &str,
        options: &[String],
        ticked: &[usize],
    ) -> Result<Vec<usize>, Cancelled>;
    /// Asks a yes/no question.
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, Cancelled>;
    /// Shows a message.
    fn message(&self, text: &str);
    /// Shows a secret (the one-time Agent Token) on the controlling terminal
    /// only, never on stdout or stderr. Fails if it could not be shown.
    fn reveal(&self, text: &str) -> Result<(), Cancelled>;
}

/// Escapes control characters (newlines, ANSI escapes) and invisible format
/// characters (bidi overrides, zero-width characters) in untrusted text, such
/// as server-supplied names or store metadata, so it cannot forge lines,
/// styling or visual order.
pub fn escape_controls(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || is_format(c) {
                c.escape_unicode().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Unicode format characters (general category Cf) that alter how text
/// displays without showing themselves.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

/// A terminal no human sits at. The default for contexts that never prompt.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoTerminal;

impl Terminal for NoTerminal {
    fn is_interactive(&self) -> bool {
        false
    }

    fn text(&self, _prompt: &str, _default: Option<&str>) -> Result<String, Cancelled> {
        Err(Cancelled)
    }

    fn hidden(&self, _prompt: &str) -> Result<Credential, Cancelled> {
        Err(Cancelled)
    }

    fn choose(
        &self,
        _prompt: &str,
        _options: &[String],
        _default: usize,
    ) -> Result<usize, Cancelled> {
        Err(Cancelled)
    }

    fn checklist(
        &self,
        _prompt: &str,
        _options: &[String],
        _ticked: &[usize],
    ) -> Result<Vec<usize>, Cancelled> {
        Err(Cancelled)
    }

    fn confirm(&self, _prompt: &str, _default: bool) -> Result<bool, Cancelled> {
        Err(Cancelled)
    }

    fn message(&self, _text: &str) {}

    fn reveal(&self, _text: &str) -> Result<(), Cancelled> {
        Err(Cancelled)
    }
}

/// The controlling terminal, opened directly so nothing goes through the
/// process's stdout or stderr.
const TTY_PATH: &str = "/dev/tty";

fn write_tty(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut tty = std::fs::OpenOptions::new().write(true).open(TTY_PATH)?;
    tty.write_all(text.as_bytes())?;
    tty.write_all(b"\n")?;
    tty.flush()
}

/// The real terminal, drawn with `inquire`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InquireTerminal;

impl Terminal for InquireTerminal {
    /// Stdin and stdout must both be TTYs. Stderr must be one too, because
    /// `inquire` draws its prompts there. The controlling terminal must open
    /// for writing, because messages and the Agent Token go there.
    fn is_interactive(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
            && std::io::stdout().is_terminal()
            && std::io::stderr().is_terminal()
            && std::fs::OpenOptions::new()
                .write(true)
                .open(TTY_PATH)
                .is_ok()
    }

    fn text(&self, prompt: &str, default: Option<&str>) -> Result<String, Cancelled> {
        let mut text = inquire::Text::new(prompt);
        if let Some(default) = default {
            text = text.with_default(default);
        }
        text.prompt().map_err(|_| Cancelled)
    }

    fn hidden(&self, prompt: &str) -> Result<Credential, Cancelled> {
        inquire::Password::new(prompt)
            .without_confirmation()
            .with_display_mode(inquire::PasswordDisplayMode::Hidden)
            .prompt()
            .map(Credential::new)
            .map_err(|_| Cancelled)
    }

    fn choose(&self, prompt: &str, options: &[String], default: usize) -> Result<usize, Cancelled> {
        inquire::Select::new(prompt, options.to_vec())
            .with_starting_cursor(default.min(options.len().saturating_sub(1)))
            .without_filtering()
            .raw_prompt()
            .map(|choice| choice.index)
            .map_err(|_| Cancelled)
    }

    fn checklist(
        &self,
        prompt: &str,
        options: &[String],
        ticked: &[usize],
    ) -> Result<Vec<usize>, Cancelled> {
        let initial: Vec<usize> = ticked
            .iter()
            .copied()
            .filter(|i| *i < options.len())
            .collect();
        let mut ticked: Vec<usize> = inquire::MultiSelect::new(prompt, options.to_vec())
            .with_default(&initial)
            .with_help_message("space to tick, enter to accept")
            .raw_prompt()
            .map_err(|_| Cancelled)?
            .into_iter()
            .map(|choice| choice.index)
            .collect();
        ticked.sort_unstable();
        Ok(ticked)
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, Cancelled> {
        inquire::Confirm::new(prompt)
            .with_default(default)
            .prompt()
            .map_err(|_| Cancelled)
    }

    /// Messages go to `/dev/tty`, never to stdout or stderr.
    fn message(&self, text: &str) {
        let _ = write_tty(text);
    }

    /// Writes to `/dev/tty` only, so a wrapper that captures fd 1 or fd 2
    /// (even through a pty) never sees the secret.
    fn reveal(&self, text: &str) -> Result<(), Cancelled> {
        write_tty(text).map_err(|_| Cancelled)
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use scripted::{Answer, Entry, Kind, ScriptedTerminal};

#[cfg(any(test, feature = "test-support"))]
mod scripted {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::{Cancelled, Terminal};
    use crate::mandate::Credential;

    /// Marker that a release build must not contain (checked by a test).
    pub const MARKER: &str = "agent-mail-scripted-terminal";

    /// Keeps the marker bytes in every binary this module is compiled into.
    #[used]
    static MARKER_BYTES: [u8; MARKER.len()] = *b"agent-mail-scripted-terminal";

    /// A queued answer for the next prompt. The kind must match the prompt.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Answer {
        /// A text answer. An empty string takes the prompt's default.
        Text(String),
        /// A hidden answer; never recorded.
        Hidden(String),
        /// Picks the option with exactly this label.
        Choose(String),
        /// Ticks the options with exactly these labels.
        Tick(Vec<String>),
        /// Answers a yes/no question.
        Confirm(bool),
        /// Accepts what the prompt offers unchanged: the text default (or
        /// an empty string without one), the default choice, the pre-ticked
        /// options, or the default yes/no.
        Accept,
        /// Cancels the prompt, as Esc or Ctrl-C would.
        Cancel,
    }

    impl Answer {
        /// A text answer.
        pub fn text(value: &str) -> Self {
            Self::Text(value.to_string())
        }

        /// A hidden answer.
        pub fn hidden(value: &str) -> Self {
            Self::Hidden(value.to_string())
        }

        /// Picks the option labelled `label`.
        pub fn choose(label: &str) -> Self {
            Self::Choose(label.to_string())
        }

        /// Ticks the options labelled `labels`.
        pub fn tick(labels: &[&str]) -> Self {
            Self::Tick(labels.iter().map(|l| (*l).to_string()).collect())
        }
    }

    /// What kind of thing the terminal showed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Kind {
        /// A text prompt.
        Text,
        /// A hidden (Credential) prompt.
        Hidden,
        /// A single choice.
        Choice,
        /// A multi-select checklist.
        Checklist,
        /// A yes/no question.
        Confirm,
        /// A message.
        Message,
        /// A secret shown on the controlling terminal only.
        Reveal,
    }

    /// One thing the terminal showed, and the answer if it was a prompt.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Entry {
        /// What kind of thing it was.
        pub kind: Kind,
        /// The prompt or message text.
        pub text: String,
        /// The options offered by a choice or checklist.
        pub options: Vec<String>,
        /// The answer given. Always `None` for hidden prompts and messages.
        pub answer: Option<String>,
        /// What the prompt offered before any answer: the text default, the
        /// label the choice cursor started on, or `yes`/`no` for a yes/no
        /// question. `None` for other entries and text prompts without one.
        pub default: Option<String>,
        /// The labels a checklist showed ticked before any answer.
        pub pre_ticked: Vec<String>,
    }

    /// A test terminal that replays queued answers and records a transcript.
    #[derive(Debug)]
    pub struct ScriptedTerminal {
        interactive: bool,
        reveal_fails: bool,
        answers: RefCell<VecDeque<Answer>>,
        transcript: RefCell<Vec<Entry>>,
    }

    impl ScriptedTerminal {
        /// An interactive terminal that will give `answers` in order.
        pub fn new(answers: Vec<Answer>) -> Self {
            Self {
                interactive: true,
                reveal_fails: false,
                answers: RefCell::new(answers.into()),
                transcript: RefCell::new(Vec::new()),
            }
        }

        /// A terminal that reports it is not interactive.
        pub fn non_interactive() -> Self {
            Self {
                interactive: false,
                ..Self::new(Vec::new())
            }
        }

        /// An interactive terminal giving `answers` whose controlling
        /// terminal cannot be written when a secret is revealed.
        pub fn failing_reveal(answers: Vec<Answer>) -> Self {
            Self {
                reveal_fails: true,
                ..Self::new(answers)
            }
        }

        /// Everything shown so far.
        pub fn transcript(&self) -> Vec<Entry> {
            self.transcript.borrow().clone()
        }

        /// The transcript as one string: every prompt, option, recorded
        /// answer and message.
        pub fn transcript_text(&self) -> String {
            let mut out = String::new();
            for entry in self.transcript.borrow().iter() {
                out.push_str(&entry.text);
                out.push('\n');
                for option in &entry.options {
                    out.push_str(option);
                    out.push('\n');
                }
                if let Some(answer) = &entry.answer {
                    out.push_str(answer);
                    out.push('\n');
                }
            }
            out
        }

        /// Answers not yet used.
        pub fn remaining(&self) -> usize {
            self.answers.borrow().len()
        }

        fn next(&self, kind: Kind, prompt: &str) -> Answer {
            self.answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| panic!("script ran out at {kind:?} prompt {prompt:?}"))
        }

        fn record(&self, kind: Kind, text: &str, options: &[String], answer: Option<String>) {
            self.record_offer(kind, text, options, answer, None, Vec::new());
        }

        fn record_offer(
            &self,
            kind: Kind,
            text: &str,
            options: &[String],
            answer: Option<String>,
            default: Option<String>,
            pre_ticked: Vec<String>,
        ) {
            self.transcript.borrow_mut().push(Entry {
                kind,
                text: text.to_string(),
                options: options.to_vec(),
                answer,
                default,
                pre_ticked,
            });
        }

        fn index_of(options: &[String], label: &str, prompt: &str) -> usize {
            options
                .iter()
                .position(|o| o == label)
                .unwrap_or_else(|| panic!("no option {label:?} at {prompt:?}: {options:?}"))
        }
    }

    impl Terminal for ScriptedTerminal {
        fn is_interactive(&self) -> bool {
            self.interactive
        }

        fn text(&self, prompt: &str, default: Option<&str>) -> Result<String, Cancelled> {
            let offered = default.map(str::to_string);
            let record = |answer| {
                self.record_offer(Kind::Text, prompt, &[], answer, offered.clone(), Vec::new());
            };
            match self.next(Kind::Text, prompt) {
                Answer::Text(value) => {
                    let value = match default {
                        Some(default) if value.is_empty() => default.to_string(),
                        _ => value,
                    };
                    record(Some(value.clone()));
                    Ok(value)
                }
                Answer::Accept => {
                    let value = default.unwrap_or_default().to_string();
                    record(Some(value.clone()));
                    Ok(value)
                }
                Answer::Cancel => {
                    record(None);
                    Err(Cancelled)
                }
                other => panic!("text prompt {prompt:?} got {other:?}"),
            }
        }

        fn hidden(&self, prompt: &str) -> Result<Credential, Cancelled> {
            self.record(Kind::Hidden, prompt, &[], None);
            match self.next(Kind::Hidden, prompt) {
                Answer::Hidden(value) => Ok(Credential::new(value)),
                Answer::Cancel => Err(Cancelled),
                _ => panic!("hidden prompt {prompt:?} got a non-hidden answer"),
            }
        }

        fn choose(
            &self,
            prompt: &str,
            options: &[String],
            default: usize,
        ) -> Result<usize, Cancelled> {
            let offered = options.get(default).cloned();
            let record = |answer| {
                self.record_offer(
                    Kind::Choice,
                    prompt,
                    options,
                    answer,
                    offered.clone(),
                    Vec::new(),
                );
            };
            match self.next(Kind::Choice, prompt) {
                Answer::Choose(label) => {
                    let index = Self::index_of(options, &label, prompt);
                    record(Some(label));
                    Ok(index)
                }
                Answer::Accept => {
                    let label = offered
                        .clone()
                        .unwrap_or_else(|| panic!("choice {prompt:?} has no default"));
                    record(Some(label));
                    Ok(default)
                }
                Answer::Cancel => {
                    record(None);
                    Err(Cancelled)
                }
                other => panic!("choice {prompt:?} got {other:?}"),
            }
        }

        fn checklist(
            &self,
            prompt: &str,
            options: &[String],
            ticked: &[usize],
        ) -> Result<Vec<usize>, Cancelled> {
            let mut initial: Vec<usize> = ticked.to_vec();
            initial.sort_unstable();
            initial.dedup();
            let pre_ticked: Vec<String> = initial
                .iter()
                .map(|i| {
                    options
                        .get(*i)
                        .unwrap_or_else(|| panic!("pre-tick {i} out of range at {prompt:?}"))
                        .clone()
                })
                .collect();
            let record = |answer| {
                self.record_offer(
                    Kind::Checklist,
                    prompt,
                    options,
                    answer,
                    None,
                    pre_ticked.clone(),
                );
            };
            match self.next(Kind::Checklist, prompt) {
                Answer::Tick(labels) => {
                    let mut ticked: Vec<usize> = labels
                        .iter()
                        .map(|l| Self::index_of(options, l, prompt))
                        .collect();
                    ticked.sort_unstable();
                    ticked.dedup();
                    record(Some(labels.join(", ")));
                    Ok(ticked)
                }
                Answer::Accept => {
                    record(Some(pre_ticked.join(", ")));
                    Ok(initial)
                }
                Answer::Cancel => {
                    record(None);
                    Err(Cancelled)
                }
                other => panic!("checklist {prompt:?} got {other:?}"),
            }
        }

        fn confirm(&self, prompt: &str, default: bool) -> Result<bool, Cancelled> {
            let word = |yes: bool| if yes { "yes" } else { "no" }.to_string();
            let record = |answer| {
                self.record_offer(
                    Kind::Confirm,
                    prompt,
                    &[],
                    answer,
                    Some(word(default)),
                    Vec::new(),
                );
            };
            let yes = match self.next(Kind::Confirm, prompt) {
                Answer::Confirm(yes) => yes,
                Answer::Accept => default,
                Answer::Cancel => {
                    record(None);
                    return Err(Cancelled);
                }
                other => panic!("confirm {prompt:?} got {other:?}"),
            };
            record(Some(word(yes)));
            Ok(yes)
        }

        fn message(&self, text: &str) {
            self.record(Kind::Message, text, &[], None);
        }

        fn reveal(&self, text: &str) -> Result<(), Cancelled> {
            if self.reveal_fails {
                return Err(Cancelled);
            }
            self.record(Kind::Reveal, text, &[], None);
            Ok(())
        }
    }
}
