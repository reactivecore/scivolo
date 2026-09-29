//! Character references `&#digits;` and `&#xhexdigits;`.

use super::lexical::SU;
use super::{Abort, XmlResult};
use crate::parse::Parser;

/// The UTF-16 units of code point `i`, as `new String(Array(i), 0, 1)`; `None` if invalid.
pub fn code_point_units(i: i32) -> Option<Vec<u16>> {
    match u32::try_from(i).ok()? {
        u @ 0xD800..=0xDFFF => Some(vec![u as u16]),
        u => char::from_u32(u).map(|c| c.encode_utf16(&mut [0; 2]).to_vec()),
    }
}

/// A character reference in an attribute value, from unit `u` up to `;`; `None` for a
/// malformed one.
pub fn char_ref_units(mut u: u16, it: &mut impl Iterator<Item = u16>) -> Option<Vec<u16>> {
    let hex = u == b'x' as u16;
    if hex {
        u = it.next()?;
    }
    let base = if hex { 16 } else { 10 };
    let mut i: i32 = 0;
    while u != b';' as u16 {
        let d = char::from_u32(u as u32)?.to_digit(base as u32)?;
        i = i.wrapping_mul(base).wrapping_add(d as i32);
        u = it.next()?;
    }
    code_point_units(i)
}

impl Parser<'_> {
    /// `[x] digits` up to `;` of a character reference after `&#`.
    pub fn x_char_ref(&mut self) -> XmlResult<Vec<u16>> {
        let hex = self.ch() == 'x';
        if hex {
            self.xml_next();
        }
        let base = if hex { 16 } else { 10 };
        let mut i: i32 = 0;
        while self.ch() != ';' {
            match self.ch() {
                c @ '0'..='9' => i = i.wrapping_mul(base).wrapping_add(c as i32 - '0' as i32),
                c @ ('a'..='f' | 'A'..='F') => {
                    if hex {
                        i = i.wrapping_mul(base).wrapping_add(c.to_digit(16).unwrap_or(0) as i32);
                    } else {
                        self.xml_error(
                            "hex char not allowed in decimal char ref\nDid you mean to write &#x ?",
                        );
                    }
                }
                SU => return Err(Abort::Truncated),
                c => self.xml_error(&format!("character '{c}' not allowed in char ref\n")),
            }
            self.xml_next();
        }
        Ok(code_point_units(i).unwrap_or_else(|| {
            let o = self.xml_offset();
            self.syntax_error("in XML literal: character reference out of range", o);
            vec![0xFFFD]
        }))
    }
}
