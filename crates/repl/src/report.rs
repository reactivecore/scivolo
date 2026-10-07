//! Errors as output lines: `<line>:<col>: error: <message>`, `-- unsupported: <what>`.

use syntax::diag::{Diagnostics, Level};
use syntax::source::{Offset, SourceFile};

/// The errors of `diags`, positioned in `source`.
pub fn diagnostics(source: &SourceFile, diags: &Diagnostics) -> Vec<String> {
    let errors = diags.unique().into_iter().filter(|d| d.level == Level::Error);
    errors.map(|d| error(source, d.offset, &d.message)).collect()
}

fn error(source: &SourceFile, at: Option<Offset>, message: &str) -> String {
    match at.filter(|&o| o <= source.len()) {
        Some(o) => format!("{}:{}: error: {message}", source.offset_to_line(o) + 1, source.column(o) + 1),
        None => format!("error: {message}"),
    }
}

pub fn unsupported(what: &str) -> String {
    format!("-- unsupported: {what}")
}
