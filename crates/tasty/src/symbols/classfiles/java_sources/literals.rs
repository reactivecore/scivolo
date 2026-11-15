// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaScanners.scala
//! Java's number, character and string literals.

use super::scanner::Token;

/// Scans the number literal at `chars[*at]`: decimal, hexadecimal, octal or binary integers,
/// `L` for a long, and decimal floating-point numbers, `F` for a float; the refusal of a
/// hexadecimal floating-point number or of an integer out of range.
pub fn number(chars: &[char], at: &mut usize) -> Result<Token, String> {
    let start = *at;
    let radix = match (chars[start], chars.get(start + 1).map(char::to_ascii_lowercase)) {
        ('0', Some('x')) => 16,
        ('0', Some('b')) => 2,
        _ => 10,
    };
    if radix != 10 {
        *at += 2;
    }
    let digit = |c: char| c.is_digit(radix) || c == '_';
    while chars.get(*at).is_some_and(|&c| digit(c)) {
        *at += 1;
    }
    let mut float = false;
    if radix == 10 && chars.get(*at) == Some(&'.') && !chars.get(*at + 1).is_some_and(|&c| c == '.') {
        float = true;
        *at += 1;
        while chars.get(*at).is_some_and(|&c| digit(c)) {
            *at += 1;
        }
    }
    if radix == 10 && chars.get(*at).is_some_and(|c| matches!(c, 'e' | 'E')) {
        float = true;
        *at += 1;
        if chars.get(*at).is_some_and(|c| matches!(c, '+' | '-')) {
            *at += 1;
        }
        while chars.get(*at).is_some_and(|&c| digit(c)) {
            *at += 1;
        }
    }
    if radix == 16 && chars.get(*at).is_some_and(|c| matches!(c, '.' | 'p' | 'P')) {
        return Err("a hexadecimal floating-point literal".to_owned());
    }
    let text: String = chars[start..*at].iter().filter(|&&c| c != '_').collect();
    let suffix = chars.get(*at).map(char::to_ascii_lowercase);
    if matches!(suffix, Some('l' | 'f' | 'd')) {
        *at += 1;
    }
    let bad = || format!("number literal `{text}`");
    if float || matches!(suffix, Some('f' | 'd')) && radix == 10 {
        if suffix == Some('f') {
            let value: f32 = text.parse().map_err(|_| bad())?;
            return Ok(Token::Float(f64::from(value)));
        }
        return Ok(Token::Double(text.parse().map_err(|_| bad())?));
    }
    let (digits, radix) = match radix {
        10 if text.len() > 1 && text.starts_with('0') => (&text[1..], 8),
        10 => (&text[..], 10),
        _ => (&text[2..], radix),
    };
    let value = u64::from_str_radix(digits, radix).map_err(|_| bad())?;
    let long = suffix == Some('l');
    let limit = if long { u64::MAX } else { u64::from(u32::MAX) };
    if value > limit || radix == 10 && value > if long { 1 << 63 } else { 1 << 31 } {
        return Err(bad());
    }
    Ok(match long {
        true => Token::Long(value as i64),
        false if radix == 10 => Token::Int(value as i64),
        false => Token::Int(i64::from(value as u32 as i32)),
    })
}

/// Scans the character or string literal at `chars[*at]` with its escapes.
pub fn quoted(chars: &[char], at: &mut usize) -> Result<Token, String> {
    let quote = chars[*at];
    *at += 1;
    let mut value = String::new();
    loop {
        let c = *chars.get(*at).ok_or("an unclosed literal")?;
        *at += 1;
        match c {
            '\n' => return Err("a line break in a literal".to_owned()),
            '\\' => value.push(escape(chars, at)?),
            c if c == quote => break,
            c => value.push(c),
        }
    }
    if quote == '"' {
        return Ok(Token::Str(value));
    }
    let mut it = value.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(Token::Char(c)),
        _ => Err(format!("character literal `'{value}'`")),
    }
}

/// The character of the escape after a backslash at `chars[*at]`.
fn escape(chars: &[char], at: &mut usize) -> Result<char, String> {
    let c = *chars.get(*at).ok_or("an unclosed literal")?;
    *at += 1;
    Ok(match c {
        'b' => '\u{8}',
        't' => '\t',
        'n' => '\n',
        'f' => '\u{c}',
        'r' => '\r',
        's' => ' ',
        '"' | '\'' | '\\' => c,
        '0'..='7' => {
            let max = if c <= '3' { 2 } else { 1 };
            let mut code = c.to_digit(8).unwrap_or(0);
            for _ in 0..max {
                let Some(d) = chars.get(*at).and_then(|d| d.to_digit(8)) else { break };
                code = code * 8 + d;
                *at += 1;
            }
            char::from_u32(code).ok_or("an octal escape")?
        }
        _ => return Err(format!("escape `\\{c}`")),
    })
}
