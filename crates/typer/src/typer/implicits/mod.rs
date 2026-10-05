//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod candidates;
pub(super) mod conversions;
mod ranking;
mod search;
mod synthesis;
