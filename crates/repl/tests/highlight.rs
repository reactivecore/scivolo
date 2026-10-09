//! `repl::highlight` against dotty's highlighting of tests/highlight/cases.txt, recorded in
//! cases.check by `just highlight-oracle`.

use std::path::Path;

#[test]
fn highlighting_matches_dotty() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/highlight");
    let cases = std::fs::read_to_string(dir.join("cases.txt")).unwrap();
    let check = std::fs::read_to_string(dir.join("cases.check")).unwrap();
    assert_eq!(cases.lines().count(), check.lines().count(), "cases.check is stale");
    let failed: Vec<String> = cases
        .lines()
        .zip(check.lines())
        .filter_map(|(case, expected)| {
            let got = repl::highlight(&case.replace("\\n", "\n")).replace('\x1b', "\\e").replace('\n', "\\n");
            (got != expected).then(|| format!("{case}\n  dotty: {expected}\n  ours:  {got}"))
        })
        .collect();
    assert!(
        failed.is_empty(),
        "{} of {} differ:\n{}",
        failed.len(),
        cases.lines().count(),
        failed.join("\n")
    );
}
