//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod as_good;
mod attempts;
mod call_views;
mod candidates;
mod capture;
mod clauses;
mod contextual;
pub(super) mod conversions;
mod defaults;
mod dictionary;
mod divergence;
mod extension_containers;
mod extension_views;
mod false_deps;
mod function_goals;
mod history;
mod measures;
mod missing_given;
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
mod view_kinds;
mod view_scope;

pub use history::History;
pub use search::is_misfit;
