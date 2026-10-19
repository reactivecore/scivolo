// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/MessageRendering.scala
//   compiler/src/dotty/tools/dotc/util/SourcePosition.scala
//! A diagnostic drawn as dotty's `MessageRendering.messageAndPos` draws it, without colors:
//! a title box, the source lines with a `N |` gutter, a `^` marker and the message under it.

use syntax::diag::{Diagnostic, Level};
use syntax::source::{Offset, SourceFile};

/// The lines of `d` in `source`, boxes `width` columns wide.
pub fn lines(source: &SourceFile, d: &Diagnostic, width: usize) -> Vec<String> {
    let mut out = vec![title(d, width)];
    let at = d.offset.filter(|&o| o <= source.len() && d.start <= d.end && d.end <= source.len());
    let gutter = match at {
        Some(point) => {
            let (start, end, point) = adjust(source, d.start, d.end, point);
            let gutter = (source.offset_to_line(end) + 1).to_string().len() + 2;
            out.extend(positioned(source, (start, end, point), &d.message, gutter, width));
            gutter
        }
        None => {
            out.extend(d.message.lines().map(str::to_owned));
            3
        }
    };
    if d.id.is_some_and(|id| id.explained) {
        let bar = bar(gutter);
        out.push(bar.clone());
        out.push(format!("{bar} longer explanation available when compiling with `-explain`"));
    }
    out
}

/// `-- [E018] Syntax Error: ----`, the title padded with dashes to `width`.
fn title(d: &Diagnostic, width: usize) -> String {
    let level = if d.level == Level::Error { "Error" } else { "Warning" };
    let title = match d.id {
        Some(id) => format!("[E{:03}] {} {level}:", id.code, id.kind),
        None => format!("{level}:"),
    };
    format!("-- {title} {}", "-".repeat(width.saturating_sub(title.chars().count() + 4)))
}

/// A zero-extent position at the end of a source ending in a line feed, moved before it.
fn adjust(source: &SourceFile, start: Offset, end: Offset, point: Offset) -> (Offset, Offset, Offset) {
    let at_eof = start == end && start == source.len() && source.text.ends_with('\n');
    if at_eof { (start - 1, end - 1, point.saturating_sub(1)) } else { (start, end, point) }
}

/// The source lines of the position, the marker, the message and the lines after the point.
fn positioned(
    source: &SourceFile,
    (start, end, point): (Offset, Offset, Offset),
    message: &str,
    gutter: usize,
    width: usize,
) -> Vec<String> {
    let (start_line, end_line) = (source.offset_to_line(start), source.offset_to_line(end));
    let last = source.offset_to_line(end.saturating_sub(1));
    let lines: Vec<usize> =
        if start_line >= last { vec![source.offset_to_line(point)] } else { (start_line..=last).collect() };
    let (before, after): (Vec<usize>, Vec<usize>) =
        lines.into_iter().partition(|&l| source.line_to_offset(l) <= point);
    let mut texts = slice_lines(source, start, end).into_iter();
    let shown = |l: usize, text: &str| format!("{:>w$} |{text}", l + 1, w = gutter - 2);
    let mut out: Vec<String> = before.into_iter().zip(&mut texts).map(|(l, text)| shown(l, text)).collect();
    let after: Vec<(usize, &str)> = after.into_iter().zip(texts).collect();
    let bar = bar(gutter);
    let carets =
        if start_line == end_line { column(source, end).saturating_sub(column(source, start)) } else { 1 };
    let pad = padding(source, start);
    out.push(format!("{bar}{pad}{}", "^".repeat(carets.max(1))));
    let mut pad_len = pad.chars().count();
    for line in message.lines() {
        let max_pad = (width as isize - gutter as isize - utf16(line) as isize).max(0) - gutter as isize;
        if max_pad < pad_len as isize {
            pad_len = max_pad.max(0) as usize;
        }
    }
    let pad = if pad_len < pad.chars().count() { " ".repeat(pad_len) } else { pad };
    out.extend(message.lines().map(|l| if l.is_empty() { bar.clone() } else { format!("{bar}{pad}{l}") }));
    if !after.is_empty() {
        out.push(bar);
    }
    let after =
        if after.len() > 3 { vec![after[0], (after[1].0, "..."), after[after.len() - 1]] } else { after };
    out.extend(after.into_iter().map(|(l, text)| shown(l, text)));
    out
}

/// The texts of the lines from the one of `start` to the one of `end`, runs of line breaks
/// separating them (dotty's `linesSlice` and `linesFrom`).
fn slice_lines(source: &SourceFile, start: Offset, end: Offset) -> Vec<&str> {
    let sentinel = source.offset_to_line(source.len()) + 1;
    let next = source.line_to_offset((source.offset_to_line(end) + 1).min(sentinel));
    let from = source.start_of_line(start) as usize;
    let slice = &source.text[from..(next as usize).max(from)];
    let breaks = |c: char| matches!(c, '\n' | '\r' | '\x0C' | '\x1A');
    let mut lines: Vec<&str> = slice.split(breaks).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() || slice.starts_with(breaks) {
        lines.insert(0, "");
    }
    lines
}

/// `|` after the gutter's spaces.
fn bar(gutter: usize) -> String {
    format!("{}|", " ".repeat(gutter - 1))
}

/// The column of `offset` in UTF-16 units, as dotty counts.
fn column(source: &SourceFile, offset: Offset) -> usize {
    utf16(&source.text[source.start_of_line(offset) as usize..offset as usize])
}

/// A space per UTF-16 unit before `offset` in its line, a tab for a tab.
fn padding(source: &SourceFile, offset: Offset) -> String {
    let before = &source.text[source.start_of_line(offset) as usize..offset as usize];
    before
        .chars()
        .flat_map(|c| std::iter::repeat_n(if c == '\t' { '\t' } else { ' ' }, c.len_utf16()))
        .collect()
}

fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}
