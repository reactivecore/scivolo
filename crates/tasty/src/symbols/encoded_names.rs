//! Names as the compiler spells them in file names (dotty's `NameTransformer`).

/// The operator characters of `NameTransformer` and their encodings in file names.
const OPERATORS: [(char, &str); 18] = [
    ('~', "$tilde"),
    ('=', "$eq"),
    ('<', "$less"),
    ('>', "$greater"),
    ('!', "$bang"),
    ('#', "$hash"),
    ('%', "$percent"),
    ('^', "$up"),
    ('&', "$amp"),
    ('|', "$bar"),
    ('*', "$times"),
    ('/', "$div"),
    ('+', "$plus"),
    ('-', "$minus"),
    (':', "$colon"),
    ('\\', "$bslash"),
    ('?', "$qmark"),
    ('@', "$at"),
];

/// `name` as the compiler spells it in file names: operators by their names, other characters
/// that are not Java identifier parts as `$uXXXX`.
pub fn encode(name: &str) -> String {
    if name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$') {
        return name.to_owned();
    }
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match OPERATORS.iter().find(|(op, _)| *op == c) {
            Some((_, code)) => out.push_str(code),
            None if c.is_alphanumeric() || c == '_' || c == '$' => out.push(c),
            None => c.encode_utf16(&mut [0; 2]).iter().for_each(|u| out.push_str(&format!("$u{u:04X}"))),
        }
    }
    out
}

/// The name `encode` spells as `encoded`.
pub fn decode(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len());
    let mut rest = encoded;
    while let Some(c) = rest.chars().next() {
        if let Some((op, code)) = OPERATORS.iter().find(|(_, code)| rest.starts_with(code)) {
            out.push(*op);
            rest = &rest[code.len()..];
            continue;
        }
        let unicode =
            rest.strip_prefix("$u").and_then(|r| r.get(..4)).and_then(|h| u32::from_str_radix(h, 16).ok());
        if let Some(c) = unicode.and_then(char::from_u32) {
            out.push(c);
            rest = &rest[6..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encodes_like_name_transformer() {
        assert_eq!(encode("::"), "$colon$colon");
        assert_eq!(encode("=:="), "$eq$colon$eq");
        assert_eq!(encode("List"), "List");
        assert_eq!(encode("Nothing$"), "Nothing$");
        assert_eq!(encode("a\u{2192}b"), "a$u2192b");
    }

    #[test]
    fn decodes_what_encode_spells() {
        for name in ["::", "=:=", "<:<", "List", "Nothing$", "a\u{2192}b", "^up"] {
            assert_eq!(decode(&encode(name)), name);
        }
    }
}
