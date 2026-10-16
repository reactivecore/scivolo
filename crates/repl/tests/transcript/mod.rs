//! Transcripts in the format of dotty's `test-resources/repl`: `scala> ` input lines, `     | `
//! continuation lines (`     |` an empty one), the expected output; blank lines do not count.

use repl::Session;

const PROMPT: &str = "scala> ";
const CONTINUATION: &str = "     |";

pub fn non_blank(lines: impl Iterator<Item = String>) -> Vec<String> {
    lines.map(|l| l.trim_end().to_owned()).filter(|l| !l.is_empty()).collect()
}

/// The transcript of the inputs of `expected` as `session` answers them.
pub fn answer(session: &mut Session, expected: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = expected.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(first) = line.strip_prefix(PROMPT) else { continue };
        let mut input = vec![first.to_owned()];
        out.push(line.to_owned());
        while let Some(more) = lines.peek().and_then(|l| l.strip_prefix(CONTINUATION)) {
            input.push(more.strip_prefix(' ').unwrap_or(more).to_owned());
            out.push(lines.next().unwrap().to_owned());
        }
        out.extend(session.eval(&input.join("\n")));
    }
    out
}
