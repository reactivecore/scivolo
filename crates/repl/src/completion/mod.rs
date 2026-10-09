//! Tab completion as dotty's REPL does it (docs/research/11-completion.md): the labels dotty
//! computes, filtered and ordered by JLine.

mod labels;
mod listed;
mod selection;
mod word;

pub use labels::label;
pub use listed::{Group, listed};
pub use selection::{desugared, selection};
pub use word::{inserts_tab, word};
