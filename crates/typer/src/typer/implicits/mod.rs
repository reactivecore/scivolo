//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod as_good;
mod attempts;
mod can_equal;
mod candidates;
mod capture;
mod class_tags;
mod clauses;
mod contextual;
pub(super) mod conversions;
mod dictionary;
mod divergence;
mod extension_views;
mod history;
mod measures;
mod not_given;
mod parts;
mod prefilter;
mod ranking;
mod scope;
mod search;
mod synthesis;
mod too_late;
mod tuple_mirrors;
mod unsettled;
mod view_kinds;

pub use history::History;
pub use search::is_misfit;
