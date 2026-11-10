//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod anon_mirrors;
mod as_good;
mod attempts;
mod call_views;
mod candidates;
mod capture;
mod child_prefixes;
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
mod mirror_sources;
mod mirror_types;
mod missing_given;
mod not_given;
mod parts;
mod prefilter;
mod product_mirrors;
mod ranking;
mod scope;
mod scope_paths;
mod search;
mod sum_children;
mod sum_mirrors;
mod synthesis;
mod too_late;
mod tuple_mirrors;
mod unsettled;
mod view_kinds;
mod view_scope;

pub use history::History;
pub use search::is_misfit;
