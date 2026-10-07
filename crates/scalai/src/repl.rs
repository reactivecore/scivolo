//! The REPL: line editing on a terminal, else the lines of stdin.

use ::repl::{Session, is_incomplete};
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Editor, Helper};
use std::io::{BufRead, IsTerminal};
use std::panic::catch_unwind;
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
    let mut session = Session::new(vm, image, width).map_err(|e| e.to_string())?;
    session.watch_interrupts().map_err(|e| e.to_string())?;
    let read = match std::io::stdin().is_terminal() {
        true => interactive(&mut session),
        false => piped(&mut session),
    };
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

/// Reads inputs with line editing and history: Ctrl-C clears the input, Ctrl-D ends.
fn interactive(session: &mut Session) -> Result<(), String> {
    let mut editor: Editor<Input, DefaultHistory> = Editor::new().map_err(|e| e.to_string())?;
    editor.set_helper(Some(Input));
    let history = crate::libs::cache_dir().join("repl-history");
    let _ = editor.load_history(&history);
    let _ = std::fs::create_dir_all(crate::libs::cache_dir());
    println!("{WELCOME}");
    println!();
    loop {
        match editor.readline(PROMPT) {
            Ok(text) if text.trim().is_empty() => {}
            Ok(text) => {
                let _ = editor.add_history_entry(text.as_str());
                let _ = editor.append_history(&history);
                if !eval(session, &text) {
                    return Ok(());
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Signal(_)) => {}
            Err(ReadlineError::Eof) => return Ok(()),
            Err(e) => return Err(format!("terminal: {e}")),
        }
    }
}

/// The editor's helper: an input is complete when it parses as one.
struct Input;

impl Validator for Input {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(match incomplete(ctx.input()) {
            true => ValidationResult::Incomplete,
            false => ValidationResult::Valid(None),
        })
    }
}

impl Completer for Input {
    type Candidate = String;
}

impl Hinter for Input {
    type Hint = String;
}

impl Highlighter for Input {}

impl Helper for Input {}
