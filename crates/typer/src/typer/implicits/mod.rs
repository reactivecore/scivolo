//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod can_equal;
mod candidates;
mod class_tags;
mod clauses;
pub(super) mod conversions;
mod extension_views;
mod not_given;
mod ranking;
mod scope;
mod search;
mod synthesis;
mod too_late;
mod unsettled;
mod view_kinds;

pub use search::is_misfit;
