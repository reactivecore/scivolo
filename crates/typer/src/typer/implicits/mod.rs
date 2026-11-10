//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod as_good;
mod attempts;
mod candidates;
mod capture;
mod clauses;
mod contextual;
mod dictionary;
mod divergence;
mod false_deps;
mod function_goals;
mod history;
mod measures;
mod not_given;
mod parts;
mod prefilter;
mod ranking;
mod scope;
mod scope_paths;
mod search;
mod synthesis;
mod too_late;
mod unsettled;
mod using_args;
mod views;

pub use history::History;
pub use search::is_misfit;
pub use views::{Goal, QualifierView, Viewed};
