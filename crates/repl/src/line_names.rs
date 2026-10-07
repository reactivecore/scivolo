//! Wrapper object names `rs$line$N`: the VM's `N` grows across resets, the user's restarts at 1.

pub const PREFIX: &str = "rs$line$";

/// `text` with every wrapper name `rs$line$K` past `base` as `rs$line$(K - base)`.
pub fn shown(text: &str, base: u32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let (before, after) = rest.split_at(at + PREFIX.len());
        let digits = &after[..after.bytes().take_while(u8::is_ascii_digit).count()];
        out.push_str(before);
        match digits.parse::<u32>() {
            Ok(k) if k > base => out.push_str(&(k - base).to_string()),
            _ => out.push_str(digits),
        }
        rest = &after[digits.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::shown;

    #[test]
    fn numbers_from_the_base() {
        assert_eq!(shown("  at rs$line$32$.f(rs$line$32:1)", 30), "  at rs$line$2$.f(rs$line$2:1)");
        assert_eq!(shown("List(rs$line$7$C@1f)", 0), "List(rs$line$7$C@1f)");
        assert_eq!(shown("rs$line$3 rs$line$ rs$line$x", 5), "rs$line$3 rs$line$ rs$line$x");
    }
}
