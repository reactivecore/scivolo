//! What an input prints instead of or before its values: diagnostics in dotty's boxes, with the
//! count of warnings and errors as dotty's REPL prints it; `-- unsupported: <what>`,
//! `-- internal error: <panic>`.

use crate::diagnostic;
use std::any::Any;
use syntax::diag::{Diagnostic, Diagnostics, Level};
use syntax::source::SourceFile;

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

/// The warnings of an input that compiles: their count, then the warnings in source order.
pub fn warnings(source: &SourceFile, diags: &Diagnostics, width: usize) -> Vec<String> {
    let mut shown = diags.unique();
    shown.sort_by_key(|d| d.offset);
    let mut out: Vec<String> = count(&shown, Level::Warning, "warning").into_iter().collect();
    out.extend(boxes(source, &shown, width));
    out
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

pub fn unsupported(what: &str) -> String {
    format!("-- unsupported: {what}")
}

/// The line for a panic with payload `panic`.
pub fn internal(panic: &(dyn Any + Send)) -> String {
    let what = (panic.downcast_ref::<String>().map(String::as_str)).or(panic.downcast_ref::<&str>().copied());
    format!("-- internal error: {}", what.unwrap_or("panic"))
}
