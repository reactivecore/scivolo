// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaScanners.scala
//! Java's text blocks (`"""…"""`, dotty's `getTextBlock`).

use super::literals::escape;
use super::scanner::Token;

/// Scans the text block whose opening `"""` ends at `chars[*at]`: the common indentation of its
/// lines that are not blank and of the closing line dropped, trailing spaces dropped, line breaks
/// as `\n`, escapes translated, an escaped line break joining two lines.
pub fn text_block(chars: &[char], at: &mut usize) -> Result<Token, String> {
    while matches!(chars.get(*at), Some(' ' | '\t' | '\u{c}')) {
        *at += 1;
    }
    if !chars.get(*at).copied().is_some_and(is_break) {
        return Err("a text block without a line break after `\"\"\"`".to_owned());
    }
    skip_break(chars, at);
    let (end, common) = extent(chars, *at)?;
    let mut value = String::new();
    let mut i = *at;
    while i < end {
        let mut indent = common;
        while indent > 0 && i < end && !is_break(chars[i]) {
            i += 1;
            indent -= 1;
        }
        let (mut trailing, mut joined) = (0, false);
        while i < end && !is_break(chars[i]) && !joined {
            let c = chars[i];
            trailing = if c.is_whitespace() { trailing + 1 } else { 0 };
            i += 1;
            if c != '\\' {
                value.push(c);
            } else if chars.get(i).copied().is_some_and(is_break) {
                joined = true;
                skip_break(chars, &mut i);
            } else {
                value.push(escape(chars, &mut i)?);
            }
        }
        for _ in 0..trailing {
            value.pop();
        }
        if i < end && is_break(chars[i]) && !joined {
            skip_break(chars, &mut i);
            value.push('\n');
        }
    }
    *at = end + 3;
    Ok(Token::Str(value))
}

/// Finds the end of the text block whose content starts at `start` and the common indentation
/// of its lines; the refusal of an unclosed text block.
fn extent(chars: &[char], start: usize) -> Result<(usize, usize), String> {
    let (mut common, mut indent, mut blank) = (usize::MAX, 0, true);
    let mut i = start;
    loop {
        let c = *chars.get(i).ok_or("an unclosed text block")?;
        if c == '"' && chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') {
            return Ok((i, common.min(indent)));
        }
        if is_break(c) {
            skip_break(chars, &mut i);
            if !blank {
                common = common.min(indent);
            }
            (indent, blank) = (0, true);
        } else if blank && c.is_whitespace() {
            i += 1;
            indent += 1;
        } else {
            blank = false;
            let escaped_break = c == '\\' && chars.get(i + 1).copied().is_some_and(is_break);
            i += if c == '\\' && !escaped_break { 2 } else { 1 };
        }
    }
}

fn is_break(c: char) -> bool {
    c == '\n' || c == '\r'
}

/// Moves `at` past the line break at it, `\r\n` as one.
fn skip_break(chars: &[char], at: &mut usize) {
    *at += if chars[*at] == '\r' && chars.get(*at + 1) == Some(&'\n') { 2 } else { 1 };
}
