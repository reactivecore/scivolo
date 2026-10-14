//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod as_good;
mod can_equal;
mod candidates;
mod class_tags;
mod clauses;
mod contextual;
pub(super) mod conversions;
mod extension_views;
mod not_given;
mod parts;
mod ranking;
mod scope;
mod search;
mod synthesis;
mod too_late;
mod unsettled;
mod view_kinds;

pub use search::is_misfit;
