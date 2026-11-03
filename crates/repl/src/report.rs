//! What an input prints instead of or before its values: diagnostics in dotty's boxes, with the
//! count of warnings and errors as dotty's REPL prints it; `-- unsupported: <what>`,
//! `-- internal error: <panic>`.

use crate::diagnostic;
use std::any::Any;
use syntax::diag::{Diagnostic, Diagnostics, Level};
use syntax::source::{Offset, SourceFile};

/// The diagnostics of an input that does not parse: no count.
pub fn syntax_errors(source: &SourceFile, diags: &Diagnostics, width: usize) -> Vec<String> {
    boxes(source, &diags.unique(), width)
}

/// The diagnostics of an input that does not compile, errors before warnings, then their count.
pub fn compile_errors(source: &SourceFile, diags: &Diagnostics, width: usize) -> Vec<String> {
    let mut shown = diags.unique();
    shown.sort_by_key(|d| d.level != Level::Error);
    let mut out = boxes(source, &shown, width);
    out.extend(count(&shown, Level::Warning, "warning"));
    out.extend(count(&shown, Level::Error, "error"));
    out
}

/// Where an entry of an input's output sorts, as dotty's `ReplDriver` orders definitions and
/// warnings: by line (-1 without a position), warnings first, then by column.
pub type Place = (i64, bool, usize);

/// The place of an entry at `offset` of `source`.
pub fn place(source: &SourceFile, offset: Option<Offset>, warning: bool) -> Place {
    offset.map_or((-1, !warning, 0), |o| (source.offset_to_line(o) as i64, !warning, source.column(o)))
}

/// The warnings of an input that compiles: their count, and each warning with its place.
pub fn warnings(
    source: &SourceFile,
    diags: &Diagnostics,
    width: usize,
) -> (Vec<String>, Vec<(Place, Vec<String>)>) {
    let shown = diags.unique();
    let count = count(&shown, Level::Warning, "warning").into_iter().collect();
    let boxes = shown.iter().map(|d| (place(source, d.offset, true), diagnostic::lines(source, d, width)));
    (count, boxes.collect())
}

fn boxes(source: &SourceFile, shown: &[&Diagnostic], width: usize) -> Vec<String> {
    shown.iter().flat_map(|d| diagnostic::lines(source, d, width)).collect()
}

/// `1 error found`, `2 warnings found`; `None` for none.
fn count(shown: &[&Diagnostic], level: Level, what: &str) -> Option<String> {
    let n = shown.iter().filter(|d| d.level == level).count();
    let plural = if n == 1 { "" } else { "s" };
    (n > 0).then(|| format!("{n} {what}{plural} found"))
}

/// What an input that starts with `//>` prints instead of running.
pub const DIRECTIVES: &str =
    "Please use `:dep com.example::artifact:version` to add dependencies in the REPL";

pub fn unsupported(what: &str) -> String {
    format!("-- unsupported: {what}")
}

/// The line for a panic with payload `panic`.
pub fn internal(panic: &(dyn Any + Send)) -> String {
    let what = (panic.downcast_ref::<String>().map(String::as_str)).or(panic.downcast_ref::<&str>().copied());
    format!("-- internal error: {}", what.unwrap_or("panic"))
}
