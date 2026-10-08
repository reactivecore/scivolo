//! What an input is, as dotty's `ParseResult` tells: nothing, a command, syntax errors, or
//! statements.

use crate::commands::{self, Command};
use crate::report;
use syntax::ast::{Ast, NodeId};
use syntax::config::Config;
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The statements of an input without syntax errors.
pub struct Parsed {
    pub source: SourceFile,
    pub ast: Ast,
    pub stats: Vec<NodeId>,
    /// The settings, with those the input's language imports chose.
    pub cfg: Config,
    /// The warnings so far.
    pub diags: Diagnostics,
}

impl Parsed {
    /// The lines of the diagnostics so far and `more`, of an input that does not compile.
    pub fn errors(&mut self, more: Diagnostics, width: usize) -> Vec<String> {
        self.diags.items.extend(more.items);
        report::compile_errors(&self.source, &self.diags, width)
    }
}

pub enum Input {
    Empty,
    Command(Command),
    Errors(Vec<String>),
    Stats(Box<Parsed>),
}

/// Whether `text` ends where more input is expected (dotty's `ParseResult.isIncomplete`).
pub fn is_incomplete(text: &str) -> bool {
    if text.trim().is_empty() || commands::parse(text).is_some() {
        return false;
    }
    let source = SourceFile::maybe_incomplete("<incomplete-handler>", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    p.parse_stats();
    p.s.diags.is_incomplete()
}

/// `text` parsed as the input of source `name`, with fresh names from `fresh`; errors drawn
/// `width` columns wide.
pub fn classify(name: &str, text: &str, names: &mut Names, fresh: &mut FreshNames, width: usize) -> Input {
    if text.trim().is_empty() {
        return Input::Empty;
    }
    if let Some(command) = commands::parse(text) {
        return Input::Command(command);
    }
    let source = SourceFile::new(name, text);
    let mut p = Parser::new(&source, names, Config::default());
    p.fresh = std::mem::take(fresh);
    let stats = p.parse_stats();
    *fresh = std::mem::take(&mut p.fresh);
    if p.s.diags.has_errors() {
        return Input::Errors(report::syntax_errors(&source, &p.s.diags, width));
    }
    if stats.is_empty() {
        return Input::Empty;
    }
    let (ast, cfg, diags) = (std::mem::take(&mut p.ast), p.s.cfg, std::mem::take(&mut p.s.diags));
    drop(p);
    Input::Stats(Box::new(Parsed { source, ast, stats, cfg, diags }))
}

#[cfg(test)]
mod tests {
    use super::{Input, classify, is_incomplete};

    #[test]
    fn blank_inputs_are_empty() {
        for text in ["", "  \t\n ", "// c", "/* c */ ;"] {
            let input = classify("t", text, &mut Default::default(), &mut Default::default(), 80);
            assert!(matches!(input, Input::Empty), "{text:?}");
        }
    }

    #[test]
    fn incomplete_inputs() {
        assert!(is_incomplete("def f(x: Int) ="));
        assert!(is_incomplete("object O:\n  val x = 1"));
        assert!(!is_incomplete("1 + 1"));
        assert!(!is_incomplete("val x = )"));
        assert!(!is_incomplete(":help"));
        assert!(!is_incomplete(""));
    }
}
