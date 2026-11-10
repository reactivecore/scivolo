//! Implicit search (dotty's `Implicits`, `Synthesizer`): implicit arguments, class tags, and
//! implicit conversions.

mod alt_types;
mod anon_mirrors;
mod as_good;
mod attempts;
mod can_equal;
mod candidates;
mod capture;
mod child_prefixes;
mod class_tag_erasure;
mod class_tags;
mod clauses;
mod contextual;
mod dictionary;
mod divergence;
mod false_deps;
mod function_goals;
mod history;
mod manifests;
mod measures;
mod mirror_sources;
mod mirror_types;
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
mod tupled_functions;
mod type_test_synthesis;
mod unsettled;
mod using_args;
mod views;

pub use history::History;
pub use search::is_misfit;
pub use views::{Goal, QualifierView, Viewed};
