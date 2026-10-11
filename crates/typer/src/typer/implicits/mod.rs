//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod can_equal;
mod candidates;
pub(super) mod conversions;
mod ranking;
mod scope;
mod search;
mod synthesis;

pub use search::is_misfit;
