// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/xml/Utility.scala
//   compiler/src/dotty/tools/dotc/util/Chars.scala
//! Character classes as dotty's `Chars`, which uses Java's `Character` predicates.

use unicode_general_category::{GeneralCategory as G, get_general_category};

pub fn is_identifier_start(c: char) -> bool {
    c == '_' || c == '$' || is_unicode_identifier_start(c)
}

pub fn is_identifier_part(c: char) -> bool {
    c == '$' || is_unicode_identifier_part(c)
}

/// Java's `Character.isUnicodeIdentifierStart`.
pub fn is_unicode_identifier_start(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::LetterNumber
    ) || is_other_id_start(c)
}

/// Java's `Character.isUnicodeIdentifierPart`.
pub fn is_unicode_identifier_part(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_' || is_identifier_ignorable(c);
    }
    is_unicode_identifier_start(c)
        || matches!(
            get_general_category(c),
            G::ConnectorPunctuation | G::DecimalNumber | G::SpacingMark | G::NonspacingMark | G::Format
        )
        || is_other_id_continue(c)
        || is_identifier_ignorable(c)
}

/// Java's `Character.isJavaIdentifierPart`.
pub fn is_java_identifier_part(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::CurrencySymbol
            | G::ConnectorPunctuation
            | G::DecimalNumber
            | G::LetterNumber
            | G::SpacingMark
            | G::NonspacingMark
    ) || is_identifier_ignorable(c)
}

/// Java's `Character.isIdentifierIgnorable`.
fn is_identifier_ignorable(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{E}'..='\u{1B}' | '\u{7F}'..='\u{9F}')
        || !c.is_ascii() && get_general_category(c) == G::Format
}

fn is_other_id_start(c: char) -> bool {
    matches!(c, '\u{1885}' | '\u{1886}' | '\u{2118}' | '\u{212E}' | '\u{309B}' | '\u{309C}')
}

fn is_other_id_continue(c: char) -> bool {
    matches!(c, '\u{B7}' | '\u{387}' | '\u{1369}'..='\u{1371}' | '\u{19DA}')
}

/// A Unicode math or other symbol.
pub fn is_special(c: char) -> bool {
    !c.is_ascii() && matches!(get_general_category(c), G::MathSymbol | G::OtherSymbol)
}

pub fn is_operator_part(c: char) -> bool {
    matches!(
        c,
        '~' | '!'
            | '@'
            | '#'
            | '%'
            | '^'
            | '*'
            | '+'
            | '-'
            | '<'
            | '>'
            | '?'
            | ':'
            | '='
            | '&'
            | '|'
            | '/'
            | '\\'
    ) || is_special(c)
}

/// The value of digit `c` in `base`, if it is one.
pub fn digit(c: char, base: u32) -> Option<u32> {
    let d = match c {
        '0'..='9' => c as u32 - '0' as u32,
        'a'..='z' => c as u32 - 'a' as u32 + 10,
        'A'..='Z' => c as u32 - 'A' as u32 + 10,
        _ => return None,
    };
    (d < base).then_some(d)
}

/// Whether `c` can start an XML name (`scala.xml.Utility.isNameStart`), for a UTF-16 unit:
/// false for supplementary characters.
pub fn is_xml_name_start(c: char) -> bool {
    c == '_'
        || (c as u32) <= 0xFFFF
            && matches!(
                get_general_category(c),
                G::UppercaseLetter
                    | G::LowercaseLetter
                    | G::TitlecaseLetter
                    | G::OtherLetter
                    | G::LetterNumber
            )
}
