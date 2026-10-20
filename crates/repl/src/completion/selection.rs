// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/interactive/Completion.scala
//! The selection under the cursor and the prefix of the names it completes (dotty's
//! `completionMode` and `completionPrefix` for a term `Select`).

use syntax::ast::{Ast, NodeId, Tree};
use syntax::names::Names;
use syntax::source::SourceFile;

/// A term selection `qual.name` the cursor is on.
#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    pub target: NodeId,
    /// The start of the name up to the cursor, without a leading backtick.
    pub prefix: String,
    /// Whether the name starts with a backtick, so that every label is backticked.
    pub backticked: bool,
}

/// The term selection innermost around byte offset `cursor` among `stats`, if the cursor is on
/// one; `None` for any other tree, a type selection, a recovery tree, a package clause, or in
/// the tuple right of a tuple pattern definition (`val (a, b) = (x.he, 1)`).
pub fn selection(
    ast: &Ast,
    names: &Names,
    source: &SourceFile,
    stats: &[NodeId],
    cursor: u32,
) -> Option<Selection> {
    let path = ast.path_to(stats, cursor);
    let (&target, outer) = path.split_first()?;
    let Tree::Select { name, .. } = *ast.tree(target) else { return None };
    let span = ast.span(target);
    let package = outer.first().is_some_and(|&o| matches!(ast.tree(o), Tree::PackageDef { .. }));
    if name.is_type || span.is_synthetic() || package || outer.iter().any(|&o| tuple_def(ast, o)) {
        return None;
    }
    let text = &source.text;
    let point = span.point() as usize;
    let backticked = text[point..].starts_with('`');
    let raw = match names.text(name.name) {
        "<error>" if backticked => text.get(point..span.end() as usize)?,
        "<error>" => "",
        name => {
            let start = point + usize::from(backticked);
            let len = (cursor as usize).checked_sub(start)?;
            name.get(..len.min(name.len()))?
        }
    };
    Some(Selection { target, prefix: raw.trim_start_matches('`').to_owned(), backticked })
}

/// Whether `t` is a pattern definition of a tuple pattern by a tuple.
fn tuple_def(ast: &Ast, t: NodeId) -> bool {
    let Tree::PatDef { pats, rhs: Some(rhs), .. } = *ast.tree(t) else { return false };
    let tuple = |n: NodeId| matches!(ast.tree(n), Tree::Tuple { .. });
    ast.items(pats).iter().any(|&p| tuple(p)) && tuple(rhs)
}

/// The selection of unit `root` that desugaring made of selection `target`: the first of the
/// same span and name.
pub fn desugared(ast: &Ast, root: NodeId, target: NodeId) -> Option<NodeId> {
    let (span, tree) = (ast.span(target), ast.tree(target));
    let Tree::Select { name, .. } = *tree else { return None };
    let same = |ast: &Ast, n: NodeId| {
        ast.span(n) == span && matches!(*ast.tree(n), Tree::Select { name: other, .. } if other == name)
    };
    ast.subtrees(root, &same).first().copied()
}

#[cfg(test)]
mod tests {
    use super::selection;
    use syntax::config::Config;
    use syntax::names::Names;
    use syntax::parse::Parser;
    use syntax::source::SourceFile;

    /// The prefix of the selection at `@@` in `text`, and whether it is backticked.
    fn prefix(text: &str) -> Option<(String, bool)> {
        let cursor = text.find("@@").expect("a cursor") as u32;
        let source = SourceFile::maybe_incomplete("t", text.replace("@@", ""));
        let mut names = Names::default();
        let mut p = Parser::new(&source, &mut names, Config::default());
        let stats = p.parse_stats();
        let found = selection(&p.ast, p.s.names, &source, &stats, cursor)?;
        Some((found.prefix, found.backticked))
    }

    #[test]
    fn takes_the_name_up_to_the_cursor() {
        assert_eq!(prefix("x.hea@@"), Some(("hea".into(), false)));
        assert_eq!(prefix("x.he@@ad"), Some(("he".into(), false)));
        assert_eq!(prefix("x.@@"), Some((String::new(), false)));
        assert_eq!(prefix("x.`hea@@"), Some(("hea".into(), true)));
        assert_eq!(prefix("x.`he@@ad`"), Some(("he".into(), true)));
        assert_eq!(prefix("x.@@head"), Some((String::new(), false)));
    }

    #[test]
    fn needs_a_term_selection() {
        assert_eq!(prefix("pri@@"), None);
        assert_eq!(prefix("val y: scala.In@@"), None);
        assert_eq!(prefix("val y = @@"), None);
        assert_eq!(prefix("val (a, b) = (x.he@@, 1)"), None);
        assert_eq!(prefix("val (a, b) = x.he@@"), Some(("he".into(), false)));
        assert_eq!(prefix("val a, b = x.he@@"), Some(("he".into(), false)));
        assert_eq!(prefix("val y = x.he@@"), Some(("he".into(), false)));
    }
}
