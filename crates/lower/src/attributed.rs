//! The refusals of a unit of several source files, each with the files it lies in.

use crate::Refusal;
use syntax::ast::Span;
use syntax::diag::{Diagnostics, Level};
use syntax::source::{Offset, Sources};

/// A refusal of a unit and the indices of the files it lies in, none if it lies in no file.
pub struct Attributed {
    pub refusal: Refusal,
    pub files: Vec<usize>,
}

/// The output of a unit, its warnings, and the refusals after typing of the files left out of it.
pub struct Accepted<R> {
    pub out: R,
    pub diags: Diagnostics,
    pub left_out: Vec<Attributed>,
}

/// The first refusal of `refusals`.
pub fn first(mut refusals: Vec<Attributed>) -> Refusal {
    refusals.swap_remove(0).refusal
}

/// The indices of the files the errors among `diags` are in, none if one of them has no position.
pub fn error_files(diags: &Diagnostics, sources: &Sources) -> Vec<usize> {
    let errors = diags.items.iter().filter(|d| d.level == Level::Error);
    let Some(mut files) = errors.map(|d| d.offset.map(|o| sources.index(o))).collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    files.sort_unstable();
    files.dedup();
    files
}

/// The index of the file the point of `at` is in, if it has one in `sources`.
pub fn file_at(sources: &Sources, at: Span) -> Vec<usize> {
    match at.exists() && !at.is_in_other_source() {
        true => vec![sources.index(at.without_inlined_call().point())],
        false => Vec::new(),
    }
}

/// A refusal `what` without diagnostics.
pub fn refused(what: String) -> Refusal {
    Refusal { what, diags: Diagnostics::default() }
}

/// ` at FILE:LINE:COLUMN` of the point of `at`, if it has one in `sources`.
pub fn at_point(sources: &Sources, at: Span) -> String {
    match at.exists() && !at.is_in_other_source() {
        true => format!(" at {}", position(sources, at.without_inlined_call().point())),
        false => String::new(),
    }
}

/// `FILE:LINE:COLUMN` of `offset`.
pub fn position(sources: &Sources, offset: Offset) -> String {
    let (file, at) = sources.locate(offset);
    format!("{}:{}:{}", file.name, file.offset_to_line(at) + 1, file.column(at) + 1)
}

/// A refusal `what` listing the errors among `diags`, each with its position in `sources`.
pub fn refusal(what: &str, diags: &Diagnostics, sources: &Sources) -> Refusal {
    let errors = diags.items.iter().filter(|d| d.level == Level::Error).map(|d| {
        let at = d.offset.map_or_else(String::new, |o| format!("{}: ", position(sources, o)));
        format!("\n  {at}{}", d.message)
    });
    refused(format!("{what}:{}", errors.collect::<String>()))
}
