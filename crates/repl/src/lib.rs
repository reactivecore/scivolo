//! A Scala REPL after dotty's: each input compiled through the own frontend into a wrapper
//! object and run in one VM (ADR 0012).

mod commands;
mod completion;
mod diagnostic;
mod earlier;
mod highlight;
mod import_text;
mod imports;
mod input;
mod line_names;
mod program;
mod render;
mod report;
mod session;
mod show_methods;
mod trace;
mod wrap;

pub use completion::{Group, inserts_tab, word};
pub use highlight::highlight;
pub use input::is_incomplete;
pub use program::{Program, Ran, compile_program, run_program};
pub use report::{compile_errors, fatal_warnings};
pub use session::Session;

/// The prompt before each input.
pub const PROMPT: &str = "> ";

/// Builds the line the REPL greets with.
pub fn welcome() -> String {
    format!(
        "Welcome to Scivolo {} (target Scala {}). Type in expressions for evaluation. Or try :help.",
        version::VERSION,
        libimage::SCALA
    )
}
