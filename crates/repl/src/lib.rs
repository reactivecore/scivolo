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
mod render;
mod report;
mod session;
mod show_methods;
mod trace;
mod wrap;

pub use completion::{Group, inserts_tab, word};
pub use highlight::highlight;
pub use input::is_incomplete;
pub use report::compile_errors;
pub use session::Session;
