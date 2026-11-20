//! A file with top-level statements as scala-cli wraps a `.sc` script (its `ClassCodeWrapper`):
//! the statements form the body of class `main_script` (scala-cli's `main$_`), which object
//! `main_sc`'s `main` creates.

use syntax::diag::Diagnostics;

/// The text before the script; it ends a line, so the script keeps its lines.
const HEADER: &str = "final class main_script {\n";

/// The text after the script.
const FOOTER: &str = "
def args = main_sc.scriptArgs
def scriptPath = \"\"\"main.sc\"\"\"
}

object main_sc {
  private var passed: Array[String] = null
  def scriptArgs: Array[String] = passed
  def main(args: Array[String]): Unit = {
    passed = args
    val _ = new main_script
  }
}
";

/// The message number of dotty's `IllegalStartOfStatement`, a statement where only
/// definitions may stand.
const ILLEGAL_START: u16 = 103;

/// Whether `diags`, of a file compiled as definitions, show that it has top-level statements.
pub fn has_statements(diags: &Diagnostics) -> bool {
    diags.items.iter().any(|d| d.id.as_ref().is_some_and(|id| id.code == ILLEGAL_START))
}

/// Wraps the script `text` into a compilation unit.
pub fn wrap(text: &str) -> String {
    format!("{HEADER}{text}{FOOTER}")
}

/// Moves the positions of `diags`, of the unit `wrap` made of a script of `len` bytes, back
/// into the script; those in the wrapper go to its nearest end.
pub fn unwrap(diags: &mut Diagnostics, len: usize) {
    let at = |o: u32| (o as usize).saturating_sub(HEADER.len()).min(len) as u32;
    for d in &mut diags.items {
        d.start = at(d.start);
        d.end = at(d.end);
        d.offset = d.offset.map(at);
    }
}
