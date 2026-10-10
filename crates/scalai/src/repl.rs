//! The REPL: line editing on a terminal, else the lines of stdin.

use crate::{history, tab};
use ::repl::{Session, highlight, is_incomplete};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::MemHistory;
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Changeset, CompletionType, Config, Context, Editor, Helper};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::io::{BufRead, IsTerminal};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use vm::{Classpath, Vm};

const PROMPT: &str = "scala> ";
const WELCOME: &str = "Welcome to Scala 3.9.0 (scalai). Type in expressions for evaluation. Or try :help.";

/// Runs the REPL over classpath `cp`; the exit code.
pub fn run(cp: &[PathBuf]) -> Result<i32, String> {
    let jars = crate::libs::jars()?;
    let image = crate::libs::image(&jars).map_err(|e| format!("the REPL needs the library image: {e}"))?;
    let classpath = Classpath::with_image(cp, image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let width = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).filter(|&w| w > 0).unwrap_or(80);
    let mut session = RefCell::new(Session::new(vm, image, width).map_err(|e| e.to_string())?);
    session.get_mut().watch_interrupts().map_err(|e| e.to_string())?;
    let read = match std::io::stdin().is_terminal() {
        true => interactive(&session),
        false => piped(session.get_mut()),
    };
    let session = session.into_inner();
    let code = session.exit().unwrap_or(0);
    session.shutdown().map_err(|e| e.to_string())?;
    read.map(|()| code)
}

/// Evaluates `input` and prints its output; whether the session goes on.
fn eval(session: &mut Session, input: &str) -> bool {
    for line in session.eval(input) {
        println!("{line}");
    }
    session.exit().is_none()
}

/// Whether `text` needs more lines; false when the check panics, so that the input runs.
fn incomplete(text: &str) -> bool {
    catch_unwind(|| is_incomplete(text)).unwrap_or(false)
}

/// Reads inputs from stdin, each the lines up to the first that completes it.
fn piped(session: &mut Session) -> Result<(), String> {
    let (mut stdin, mut bytes, mut input) = (std::io::stdin().lock(), Vec::new(), String::new());
    loop {
        bytes.clear();
        if stdin.read_until(b'\n', &mut bytes).map_err(|e| format!("stdin: {e}"))? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&bytes);
        input.push_str(line.trim_end_matches(['\n', '\r']));
        if incomplete(&input) {
            input.push('\n');
            continue;
        }
        let text = std::mem::take(&mut input);
        if !text.trim().is_empty() && !eval(session, &text) {
            return Ok(());
        }
    }
    if !input.trim().is_empty() {
        eval(session, &input);
    }
    Ok(())
}

/// Reads inputs with line editing, history and completion: Ctrl-C clears the input, Ctrl-D ends.
fn interactive(session: &RefCell<Session>) -> Result<(), String> {
    let config = Config::builder()
        .max_history_size(history::MEMORY)
        .and_then(|c| c.history_ignore_dups(false))
        .map_err(|e| e.to_string())?
        .history_ignore_space(false)
        .completion_type(CompletionType::List)
        .completion_show_all_if_ambiguous(true)
        .completion_prompt_limit(tab::LIST_MAX - 1)
        .build();
    let memory = MemHistory::with_config(&config);
    let mut editor = Editor::with_history(config, memory).map_err(|e| e.to_string())?;
    editor.set_helper(Some(Input { session, end: Cell::new(0) }));
    let file = history::path();
    let mut last = None;
    for entry in file.as_deref().map(history::load).unwrap_or_default() {
        let _ = editor.add_history_entry(entry.as_str());
        last = Some(entry);
    }
    println!("{WELCOME}");
    loop {
        println!();
        match editor.readline(PROMPT) {
            Ok(text) => {
                if let Some(entry) = history::accept(last.as_deref(), &text) {
                    let _ = editor.add_history_entry(entry.as_str());
                    if let Some(file) = &file {
                        let _ = history::append(file, &entry);
                    }
                    last = Some(entry);
                }
                if !text.trim().is_empty() && !eval(&mut session.borrow_mut(), &text) {
                    return Ok(());
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Signal(_)) => {}
            Err(ReadlineError::Eof) => return Ok(()),
            Err(e) => return Err(format!("terminal: {e}")),
        }
    }
}

/// The editor's helper: an input is complete when it parses as one, coloured and completed as
/// in dotty; `end` is where the range of the last completion ends.
struct Input<'s> {
    session: &'s RefCell<Session>,
    end: Cell<usize>,
}

/// The result of `f`, with the panic hook silenced while it runs.
fn unreported<T>(f: impl FnOnce() -> T) -> T {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = f();
    std::panic::set_hook(hook);
    out
}

impl Validator for Input<'_> {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(match unreported(|| incomplete(ctx.input())) {
            true => ValidationResult::Incomplete,
            false => ValidationResult::Valid(None),
        })
    }
}

impl Completer for Input<'_> {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        let Ok(mut session) = self.session.try_borrow_mut() else {
            return Ok((pos, Vec::new()));
        };
        let found = unreported(|| catch_unwind(AssertUnwindSafe(|| tab::complete(&mut session, line, pos))));
        let (range, candidates) = found.unwrap_or((pos..pos, Vec::new()));
        self.end.set(range.end);
        Ok((range.start, candidates))
    }

    fn update(&self, line: &mut LineBuffer, start: usize, elected: &str, cl: &mut Changeset) {
        let end = self.end.get().clamp(line.pos(), line.len());
        line.replace(start..end, elected, cl);
    }
}

impl Hinter for Input<'_> {
    type Hint = String;
}

impl Highlighter for Input<'_> {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(unreported(|| highlight(line)))
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, default: bool) -> Cow<'b, str> {
        match default {
            true => Cow::Owned(format!("\x1b[35m{prompt}\x1b[0m")),
            false => Cow::Borrowed(prompt),
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, kind: CmdKind) -> bool {
        kind != CmdKind::MoveCursor
    }
}

impl Helper for Input<'_> {}
