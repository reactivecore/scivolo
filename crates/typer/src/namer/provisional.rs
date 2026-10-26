// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The provisional info of a match type alias while its right-hand side is typed, so that the
//! alias may refer to itself (dotty's `typeDefSig` dummy info).

use crate::typer::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type};

impl Typer<'_> {
    /// Gives match type alias `sym` of definition `tree` the info of an abstract type over its
    /// type parameters; nothing for another definition.
    pub fn set_provisional_info(&mut self, sym: SymbolId, tree: NodeId) {
        let Some(tparams) = self.match_alias_params(tree) else { return };
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let info = match tparams.is_empty() {
            true => bounds,
            false => {
                let params = tparams
                    .iter()
                    .map(|&p| match *self.ast.tree(p) {
                        Tree::TypeDef { name, .. } => (self.text(name.name).to_owned(), bounds),
                        _ => (String::new(), bounds),
                    })
                    .collect();
                let binder = self.syms.fresh_binder();
                self.u.intern_type(Type::Lambda { binder, params, result: bounds })
            }
        };
        self.u.set_info(sym, info);
    }

    /// Whether definition `tree` is a match type alias, which has a provisional info while it
    /// completes.
    pub fn is_provisional(&self, tree: NodeId) -> bool {
        self.match_alias_params(tree).is_some()
    }

    /// The type parameters of definition `tree` if it is a match type alias.
    fn match_alias_params(&self, tree: NodeId) -> Option<Vec<NodeId>> {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return None };
        let (tparams, body) = match *self.ast.tree(rhs) {
            Tree::LambdaTypeTree { tparams, body } => (self.ast.items(tparams).to_vec(), body),
            _ => (Vec::new(), rhs),
        };
        matches!(self.ast.tree(body), Tree::MatchTypeTree { .. }).then_some(tparams)
    }
}
