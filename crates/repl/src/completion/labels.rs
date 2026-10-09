//! The labels of completed names (dotty's `backtickCompletions`).

use syntax::scan::{is_identifier_part, is_identifier_start, is_operator_part};

/// `name` as a label: backticked if `backticked` or if it is no plain identifier or operator.
pub fn label(name: &str, backticked: bool) -> String {
    if backticked || needs_backtick(name) { format!("`{name}`") } else { name.to_owned() }
}

/// Whether `name` must be backticked: its chunks between underscores are not all identifier
/// characters but for a last chunk of operator characters, it starts neither as an identifier
/// nor with operator characters, or it is a keyword (dotty's `needsBacktick`).
fn needs_backtick(name: &str) -> bool {
    let chunks: Vec<&str> = name.split('_').collect();
    let last = chunks.len() - 1;
    let valid_chunks = chunks.iter().enumerate().all(|(i, chunk)| {
        chunk.chars().all(is_identifier_part)
            || chunk.chars().all(is_operator_part) && i == last && !(i == 1 && chunks[0].is_empty())
    });
    let valid_start =
        name.chars().next().is_some_and(is_identifier_start) || chunks[0].chars().all(is_operator_part);
    !(valid_chunks && valid_start && syntax::token::keyword(name).is_none())
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn backticks_what_is_no_identifier() {
        assert_eq!(label("head", false), "head");
        assert_eq!(label("head", true), "`head`");
        assert_eq!(label("odd name", false), "`odd name`");
        assert_eq!(label("unary_-", false), "unary_-");
        assert_eq!(label("++:", false), "++:");
        assert_eq!(label("_1$mcI$sp", false), "_1$mcI$sp");
        assert_eq!(label("type", false), "`type`");
        assert_eq!(label("→", false), "→");
        assert_eq!(label("reverse_:::", false), "reverse_:::");
    }
}
