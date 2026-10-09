//! What JLine lists for the REPL's labels and the word under the cursor (`CompletionMatcherImpl`
//! with the REPL's options, `LineReaderImpl.computePost` with groups).

/// The group JLine's typo matcher puts the word itself in.
const ORIGINAL: &str = "original";
/// The group of ungrouped candidates when there are several groups.
const OTHERS: &str = "others";
/// The distance below which the typo matcher keeps a candidate (`DEFAULT_ERRORS`).
const ERRORS: usize = 2;

/// Candidates listed under one header, in JLine's order.
#[derive(Debug, PartialEq, Eq)]
pub struct Group {
    pub header: Option<&'static str>,
    pub labels: Vec<String>,
}

/// The groups JLine lists for `labels` and `word`: the labels starting with the word, else, if
/// `labels` are all dotty's, those containing it, else those within a typo of it and then the
/// word itself under `original`.
pub fn listed(word: &str, mut labels: Vec<String>, complete: bool) -> Vec<Group> {
    labels.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    labels.dedup();
    let word16: Vec<u16> = word.encode_utf16().collect();
    let matchers: [&dyn Fn(&str) -> bool; 3] = [&|l| l.starts_with(word), &|l| l.contains(word), &|l| {
        distance(&word16, &l.encode_utf16().collect::<Vec<_>>()) < ERRORS
    }];
    let tried = if complete { &matchers[..] } else { &matchers[..1] };
    let Some((i, matched)) = tried.iter().enumerate().find_map(|(i, m)| {
        let matched: Vec<String> = labels.iter().filter(|l| m(l)).cloned().collect();
        (!matched.is_empty()).then_some((i, matched))
    }) else {
        return Vec::new();
    };
    if i == 2 && matched.len() > 1 {
        let original = Group { header: Some(ORIGINAL), labels: vec![word.to_owned()] };
        return vec![original, Group { header: Some(OTHERS), labels: matched }];
    }
    vec![Group { header: None, labels: matched }]
}

/// The distance of `word` to `cand` or to its prefix as long as `word` (`ReaderUtils.distance`).
fn distance(word: &[u16], cand: &[u16]) -> usize {
    if word.len() < cand.len() {
        damerau(word, &cand[..word.len()]).min(damerau(word, cand))
    } else {
        damerau(word, cand)
    }
}

/// The Damerau-Levenshtein distance with unit costs, by JLine's `Levenshtein.distance`.
fn damerau(source: &[u16], target: &[u16]) -> usize {
    let (n, m) = (source.len(), target.len());
    if n == 0 || m == 0 {
        return n.max(m);
    }
    let mut table = vec![vec![0usize; m]; n];
    let mut index_by_char = std::collections::HashMap::new();
    table[0][0] = usize::from(source[0] != target[0]);
    index_by_char.insert(source[0], 0);
    for i in 1..n {
        let matched = i + usize::from(source[i] != target[0]);
        table[i][0] = (table[i - 1][0] + 1).min(i + 2).min(matched);
    }
    for j in 1..m {
        let matched = j + usize::from(source[0] != target[j]);
        table[0][j] = (j + 2).min(table[0][j - 1] + 1).min(matched);
    }
    for i in 1..n {
        let mut max_match = if source[i] == target[0] { Some(0) } else { None };
        for j in 1..m {
            let swap_index = index_by_char.get(&target[j]).copied();
            let j_swap = max_match;
            let mut matched = table[i - 1][j - 1];
            if source[i] != target[j] {
                matched += 1;
            } else {
                max_match = Some(j);
            }
            let swapped = match (swap_index, j_swap) {
                (Some(i_swap), Some(j_swap)) => {
                    let pre = if i_swap == 0 && j_swap == 0 {
                        0
                    } else {
                        table[i_swap.max(1) - 1][j_swap.max(1) - 1]
                    };
                    pre + (i - i_swap - 1) + (j - j_swap - 1) + 1
                }
                _ => usize::MAX,
            };
            table[i][j] = (table[i - 1][j] + 1).min(table[i][j - 1] + 1).min(matched).min(swapped);
        }
        index_by_char.insert(source[i], i);
    }
    table[n - 1][m - 1]
}

#[cfg(test)]
mod tests {
    use super::{Group, damerau, listed};

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn labels(ls: &[&str]) -> Vec<String> {
        ls.iter().map(|&l| l.to_owned()).collect()
    }

    #[test]
    fn measures_like_jline() {
        assert_eq!(damerau(&units("kitten"), &units("sitting")), 3);
        assert_eq!(damerau(&units("ab"), &units("ba")), 1);
        assert_eq!(damerau(&units("maq"), &units("map")), 1);
        assert_eq!(damerau(&units(""), &units("abc")), 3);
    }

    #[test]
    fn filters_by_prefix_then_substring_then_typo() {
        let all = labels(&["map", "max", "head", "headOption"]);
        let head = [Group { header: None, labels: labels(&["head", "headOption"]) }];
        assert_eq!(listed("hea", all.clone(), true), head);
        assert_eq!(listed("hea", all.clone(), false), head);
        let option = [Group { header: None, labels: labels(&["headOption"]) }];
        assert_eq!(listed("Option", all.clone(), true), option);
        let typo = listed("maq", all.clone(), true);
        assert_eq!(typo[0], Group { header: Some("original"), labels: labels(&["maq"]) });
        assert_eq!(typo[1], Group { header: Some("others"), labels: labels(&["map", "max"]) });
        assert_eq!(listed("zzz", all.clone(), true), []);
    }

    #[test]
    fn only_by_prefix_for_some_of_the_labels() {
        let all = labels(&["map", "max", "head", "headOption"]);
        assert_eq!(listed("Option", all.clone(), false), []);
        assert_eq!(listed("maq", all, false), []);
    }

    #[test]
    fn sorts_by_utf16_units() {
        let sorted = listed("", labels(&["`head`", "`headOption`", "b", "B"]), false);
        assert_eq!(sorted[0].labels, labels(&["B", "`headOption`", "`head`", "b"]));
    }
}
