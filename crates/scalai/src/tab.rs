//! Tab in the editor: what JLine's `complete-word` replaces with what, for dotty's REPL.

use ::repl::{Group, Session, inserts_tab, word};
use rustyline::completion::Pair;
use std::ops::Range;

/// The number of candidates from which the editor asks before listing them (dotty's `LIST_MAX`).
pub const LIST_MAX: usize = 400;

/// The byte range Tab replaces at byte offset `cursor` of `text`, and the candidates there.
pub fn complete(session: &mut Session, text: &str, cursor: usize) -> (Range<usize>, Vec<Pair>) {
    let range = replaced(text, cursor);
    if inserts_tab(text) {
        return (range, vec![Pair { display: "\t".into(), replacement: "\t".into() }]);
    }
    let groups = session.complete(text, cursor);
    let pairs = candidates(groups, &text[range.clone()], &text[range.start..cursor]);
    (range, pairs)
}

/// The byte range Tab replaces at `cursor` of `text`: the word around it, or the cursor alone
/// where Tab inserts a tab.
fn replaced(text: &str, cursor: usize) -> Range<usize> {
    match inserts_tab(text) {
        true => cursor..cursor,
        false => word(text, cursor),
    }
}

/// The candidates for `groups`, listed for `word` of which `typed` precedes the cursor, each
/// displayed without backticks. The only one is replaced by its label; several by what Tab
/// inserts for all: their common prefix if it starts with `word`, else `typed`.
fn candidates(groups: Vec<Group>, word: &str, typed: &str) -> Vec<Pair> {
    let labels: Vec<String> = groups.into_iter().flat_map(|g| g.labels).collect();
    let inserted = match labels.as_slice() {
        [] | [_] => None,
        all => Some(Some(common_prefix(all)).filter(|p| p.starts_with(word)).unwrap_or(typed).to_owned()),
    };
    let pair = |label: &String| Pair {
        display: displayed(label).to_owned(),
        replacement: inserted.clone().unwrap_or_else(|| label.clone()),
    };
    labels.iter().map(pair).collect()
}

/// The longest prefix all of `labels` start with.
fn common_prefix(labels: &[String]) -> &str {
    let Some((first, rest)) = labels.split_first() else {
        return "";
    };
    rest.iter().fold(first.as_str(), |prefix, label| {
        let differs = prefix.char_indices().zip(label.chars()).find(|((_, a), b)| a != b);
        &prefix[..differs.map_or(prefix.len().min(label.len()), |((i, _), _)| i)]
    })
}

/// `label` without enclosing backticks (dotty's `stripBackTicks`).
fn displayed(label: &str) -> &str {
    match label.starts_with('`') && label.ends_with('`') {
        true => label.get(1..label.len() - 1).unwrap_or_default(),
        false => label,
    }
}

#[cfg(test)]
mod tests {
    use super::{candidates, common_prefix, displayed, replaced};
    use ::repl::Group;

    fn group(header: Option<&'static str>, labels: &[&str]) -> Group {
        Group { header, labels: labels.iter().map(|&l| l.to_owned()).collect() }
    }

    fn pairs(groups: Vec<Group>, word: &str, typed: &str) -> Vec<(String, String)> {
        candidates(groups, word, typed).into_iter().map(|p| (p.display, p.replacement)).collect()
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|&(d, r)| (d.to_owned(), r.to_owned())).collect()
    }

    #[test]
    fn replaces_the_word_around_the_cursor() {
        assert_eq!(replaced("x.head", 4), 2..6);
        assert_eq!(replaced("x.", 2), 2..2);
        assert_eq!(replaced(":t", 2), 0..2);
        assert_eq!(replaced("x.he\n  ", 4), 4..4);
        assert_eq!(replaced("", 0), 0..0);
    }

    #[test]
    fn finds_the_common_prefix() {
        let labels = |ls: &[&str]| ls.iter().map(|&l| l.to_owned()).collect::<Vec<_>>();
        assert_eq!(common_prefix(&labels(&["head", "headOption"])), "head");
        assert_eq!(common_prefix(&labels(&["headOption", "head"])), "head");
        assert_eq!(common_prefix(&labels(&["map", "max", "maq"])), "ma");
        assert_eq!(common_prefix(&labels(&["fée", "fête"])), "f");
        assert_eq!(common_prefix(&labels(&["a", "b"])), "");
        assert_eq!(common_prefix(&labels(&[])), "");
    }

    #[test]
    fn inserts_the_only_candidate() {
        let only = pairs(vec![group(None, &["headOption"])], "Option", "Opt");
        assert_eq!(only, owned(&[("headOption", "headOption")]));
        let backticked = pairs(vec![group(None, &["`type`"])], "`ty", "`ty");
        assert_eq!(backticked, owned(&[("type", "`type`")]));
    }

    #[test]
    fn inserts_the_common_prefix_of_several() {
        let several = pairs(vec![group(None, &["head", "headOption"])], "he", "he");
        assert_eq!(several, owned(&[("head", "head"), ("headOption", "head")]));
        let ticks = pairs(vec![group(None, &["`headOption`", "`head`"])], "`he", "`he");
        assert_eq!(ticks, owned(&[("headOption", "`head"), ("head", "`head")]));
    }

    #[test]
    fn keeps_the_typed_text_unless_the_prefix_extends_the_word() {
        let groups = vec![group(Some("original"), &["maq"]), group(Some("others"), &["map", "max"])];
        let typo = pairs(groups, "maq", "ma");
        assert_eq!(typo, owned(&[("maq", "ma"), ("map", "ma"), ("max", "ma")]));
        let inside = pairs(vec![group(None, &["headOption", "lastOption"])], "Option", "Option");
        assert_eq!(inside, owned(&[("headOption", "Option"), ("lastOption", "Option")]));
    }

    #[test]
    fn lists_in_the_order_given() {
        let listed = pairs(vec![group(None, &["b", "a"]), group(None, &["c"])], "", "");
        let displays: Vec<_> = listed.into_iter().map(|(d, _)| d).collect();
        assert_eq!(displays, ["b", "a", "c"]);
        assert!(pairs(Vec::new(), "x", "x").is_empty());
    }

    #[test]
    fn displays_without_backticks() {
        assert_eq!(displayed("`head`"), "head");
        assert_eq!(displayed("`hea"), "`hea");
        assert_eq!(displayed("`"), "");
        assert_eq!(displayed("map"), "map");
    }
}
