//! A Scala REPL after dotty's: each input compiled through the own frontend into a wrapper
//! object and run in one VM (ADR 0012).

mod commands;
mod earlier;
mod imports;
mod input;
mod render;
mod report;
mod session;
mod show_methods;
mod unit_results;
mod value;
mod wrap;

pub use input::is_incomplete;
pub use session::Session;
