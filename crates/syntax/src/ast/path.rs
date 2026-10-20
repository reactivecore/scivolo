// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/NavigateAST.scala
//! The trees enclosing an offset (dotty's `NavigateAST.pathTo` with `skipZeroExtent`).

use super::{Ast, NodeId, Span};
use crate::source::Offset;

impl Ast {
    /// The trees from one of `roots` down to the innermost one whose span contains `offset`,
    /// innermost first; trees of zero extent are skipped.
    pub fn path_to(&self, roots: &[NodeId], offset: Offset) -> Vec<NodeId> {
        let mut path = self.child_path(roots, offset, Vec::new());
        path.reverse();
        path
    }

    /// `path`, outermost first, extended by the best fit among `nodes`.
    fn child_path(&self, nodes: &[NodeId], offset: Offset, path: Vec<NodeId>) -> Vec<NodeId> {
        let mut best: Option<Vec<NodeId>> = None;
        for &n in nodes {
            let span = self.span(n);
            if !span.exists() || span.start() == span.end() || span.start() > offset || span.end() < offset {
                continue;
            }
            let mut extended = path.clone();
            extended.push(n);
            let candidate = self.child_path(&self.child_nodes(n), offset, extended);
            if best.as_ref().is_none_or(|b| self.is_better_fit(b, &candidate, offset)) {
                best = Some(candidate);
            }
        }
        best.unwrap_or(path)
    }

    /// Whether path `candidate` fits `offset` better than `best`, by their innermost trees.
    fn is_better_fit(&self, best: &[NodeId], candidate: &[NodeId], offset: Offset) -> bool {
        let (Some(&b), Some(&c)) = (best.last(), candidate.last()) else { return false };
        let (b, c) = (self.span(b), self.span(c));
        let (bp, cp) = (b.point(), c.point());
        let better_point = offset <= cp && cp < bp || bp < cp && cp <= offset || cp <= offset && offset < bp;
        b != c && envelops(b, c)
            || b.contains(c) && b.is_synthetic() && !c.is_synthetic()
            || c.start() == b.start() && c.end() == b.end() && better_point
    }
}

fn envelops(a: Span, b: Span) -> bool {
    !b.exists()
        || a.exists()
            && (a.start() < b.start() && a.end() >= b.end() || a.start() <= b.start() && a.end() > b.end())
}

#[cfg(test)]
mod tests {
    use crate::ast::Tree;
    use crate::config::Config;
    use crate::names::Names;
    use crate::parse::Parser;
    use crate::source::SourceFile;

    /// The innermost tree at `offset` in `text`, printed as its kind and span.
    fn innermost(text: &str, offset: u32) -> String {
        let source = SourceFile::maybe_incomplete("t", text);
        let mut names = Names::default();
        let mut p = Parser::new(&source, &mut names, Config::default());
        let stats = p.parse_stats();
        let path = p.ast.path_to(&stats, offset);
        let Some(&head) = path.first() else { return "none".into() };
        let span = p.ast.span(head);
        let kind = match p.ast.tree(head) {
            Tree::Select { .. } => "Select",
            Tree::Ident { .. } => "Ident",
            Tree::Apply { .. } => "Apply",
            _ => "other",
        };
        format!("{kind} {}..{}", span.start(), span.end())
    }

    #[test]
    fn finds_the_innermost_tree() {
        assert_eq!(innermost("List(1).ma", 10), "Select 0..10");
        assert_eq!(innermost("x.", 2), "Select 0..2");
        assert_eq!(innermost("x.he\nx.size", 4), "Select 0..4");
        assert_eq!(innermost("x.map(_.toS)", 11), "Select 6..11");
        assert_eq!(innermost("val y = x.hea", 13), "Select 8..13");
        assert_eq!(innermost("f(a, b)", 5), "Ident 5..6");
        assert_eq!(innermost("", 0), "none");
    }
}
