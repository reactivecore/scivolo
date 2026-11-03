// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! The local variables flow typing tracks and the spans of their assignments (dotty's
//! `assignmentSpans`).

use super::Typer;
use rustc_hash::FxHashMap;
use syntax::ast::flags as f;
use syntax::ast::{Ast, Attrs, NodeId, Span, Tree};
use syntax::names::Name;
use tasty::SymbolId;

/// The definitions of the tracked variables with the spans of their assignments.
pub type Assignments = FxHashMap<NodeId, Vec<Span>>;

#[derive(Default)]
struct Populate {
    tracked: Assignments,
    candidates: FxHashMap<Name, (NodeId, Vec<Span>)>,
    reachable: Vec<Name>,
}

impl Populate {
    fn traverse(&mut self, ast: &Ast, t: NodeId) {
        let saved = self.reachable.len();
        match *ast.tree(t) {
            Tree::Block { stats, .. } => {
                let vars: Vec<(Name, NodeId)> = ast
                    .items(stats)
                    .iter()
                    .filter_map(|&s| match *ast.tree(s) {
                        Tree::ValDef { name, mods, .. }
                            if ast.modifiers(mods).flags.is(f::MUTABLE)
                                && !ast.attrs(s).contains(Attrs::PATTERN_VAR) =>
                        {
                            Some((name.name, s))
                        }
                        _ => None,
                    })
                    .collect();
                let mut shadowed = Vec::new();
                for &(name, def) in &vars {
                    if let Some(prev) = self.candidates.insert(name, (def, Vec::new())) {
                        shadowed.push((name, prev));
                    }
                    self.reachable.push(name);
                }
                self.children(ast, t);
                for (name, def) in vars {
                    if let Some((d, spans)) = self.candidates.remove(&name).filter(|c| c.0 == def) {
                        self.tracked.insert(d, spans);
                    }
                }
                self.candidates.extend(shadowed);
            }
            Tree::Assign { lhs, .. } => {
                if let Tree::Ident { name } = *ast.tree(lhs)
                    && self.candidates.contains_key(&name.name)
                {
                    if self.reachable.contains(&name.name) {
                        self.candidates.get_mut(&name.name).expect("a candidate").1.push(ast.span(t));
                    } else {
                        self.candidates.remove(&name.name);
                    }
                }
                self.children(ast, t);
            }
            Tree::If { .. }
            | Tree::WhileDo { .. }
            | Tree::Typed { .. }
            | Tree::Match { .. }
            | Tree::CaseDef { .. }
            | Tree::ParsedTry { .. }
            | Tree::Try { .. } => self.children(ast, t),
            _ => {
                let outer = std::mem::take(&mut self.reachable);
                self.children(ast, t);
                self.reachable = outer;
            }
        }
        self.reachable.truncate(saved);
    }

    fn children(&mut self, ast: &Ast, t: NodeId) {
        for c in ast.child_nodes(t) {
            self.traverse(ast, c);
        }
    }
}

impl Typer<'_> {
    /// The spans of the assignments to local variable `sym` if flow typing tracks it: it is
    /// assigned only where its block reaches through conditionals, loops, matches and `try`s.
    pub fn assignment_spans(&mut self, sym: SymbolId) -> Option<&[Span]> {
        if self.nullables.assignments.is_none() {
            let mut p = Populate::default();
            p.traverse(self.ast, self.unit);
            self.nullables.assignments = Some(p.tracked);
        }
        let def = self.syms.def_trees.get(&sym)?;
        self.nullables.assignments.as_ref()?.get(def).map(Vec::as_slice)
    }
}
