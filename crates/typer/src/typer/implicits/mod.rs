//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

pub(super) mod alt_types;
mod as_good;
mod attempts;
pub(super) mod candidates;
mod capture;
mod clauses;
mod contextual;
mod covering;
mod dictionary;
mod divergence;
mod false_deps;
mod function_goals;
mod history;
mod measures;
mod not_given;
mod parts;
mod prefilter;
mod rank;
pub(super) mod ranking;
mod scope;
mod scope_paths;
mod search;
mod synthesis;
mod too_late;
mod unsettled;
mod using_args;
mod views;

pub use attempts::{CANDIDATE_OUTSIDE, NESTED_OUTSIDE};
pub use false_deps::method_without_false_deps;
pub use history::History;
pub use search::is_misfit;
pub use views::{Goal, QualifierView, Viewed};
