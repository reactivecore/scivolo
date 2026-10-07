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
use std::path::PathBuf;

const PROMPT: &str = "scala> ";
const WELCOME: &str = "Welcome to Scala 3.9.0 (scalai). Type in expressions for evaluation. Or try :help.";

/// Runs the REPL over classpath `cp`; the exit code.
pub fn run(cp: &[PathBuf]) -> Result<i32, String> {
    let (vm, image) = crate::vm(cp)?;
    let image = image.ok_or("the REPL needs the library image")?;
    let mut session = Session::new(vm, image).map_err(|e| e.to_string())?;
    match std::io::stdin().is_terminal() {
        true => interactive(&mut session)?,
        false => piped(&mut session),
    }
    let code = session.exit().unwrap_or(0);
    session.shutdown().map_err(|e| e.to_string())?;
    Ok(code)
}

/// Evaluates `input` and prints its output; whether the session goes on.
fn eval(session: &mut Session, input: &str) -> bool {
    for line in session.eval(input) {
        println!("{line}");
    }
    session.exit().is_none()
}

/// Reads inputs from stdin, each the lines up to the first that completes it.
fn piped(session: &mut Session) {
    let mut input = String::new();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        input.push_str(&line);
        if is_incomplete(&input) {
            input.push('\n');
            continue;
        }
        let text = std::mem::take(&mut input);
        if !text.trim().is_empty() && !eval(session, &text) {
            return;
        }
    }
    if !input.trim().is_empty() {
        eval(session, &input);
    }
}

/// Reads inputs with line editing and history: Ctrl-C clears the input, Ctrl-D ends.
fn interactive(session: &mut Session) -> Result<(), String> {
    let mut editor: Editor<Input, DefaultHistory> = Editor::new().map_err(|e| e.to_string())?;
    editor.set_helper(Some(Input));
    let history = crate::libs::cache_dir().join("repl-history");
    let _ = editor.load_history(&history);
    println!("{WELCOME}");
    println!();
    loop {
        match editor.readline(PROMPT) {
            Ok(text) if text.trim().is_empty() => {}
            Ok(text) => {
                let _ = editor.add_history_entry(text.as_str());
                if !eval(session, &text) {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => {}
            Err(ReadlineError::Eof) => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = std::fs::create_dir_all(crate::libs::cache_dir());
    let _ = editor.save_history(&history);
    Ok(())
}

/// The editor's helper: an input is complete when it parses as one.
struct Input;

impl Validator for Input {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(match is_incomplete(ctx.input()) {
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
