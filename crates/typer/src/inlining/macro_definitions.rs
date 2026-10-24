// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MacroAnnotations.scala
//! The definitions a macro annotation returns: the symbols their copies keep, the owners of
//! what their right-hand sides define and their places among the declarations of their owners.

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, SymbolId};

impl Inlining<'_> {
    /// The symbols of the definitions in definition `t`, which a macro's copy of it keeps; none
    /// if `t` is no definition.
    pub fn definition_symbols(&self, t: TNodeId) -> Vec<SymbolId> {
        let is_def =
            |t: TNodeId| is_definition(self.tpd.tree(t)) || matches!(self.tpd.tree(t), T::Bind { .. });
        if !is_definition(self.tpd.tree(t)) {
            return Vec::new();
        }
        let (mut todo, mut syms) = (vec![t], Vec::new());
        while let Some(n) = todo.pop() {
            if is_def(n) {
                syms.extend(self.tpd.sym(n));
            }
            todo.extend(self.tpd.children(n));
        }
        syms
    }

    /// Gives the definitions in the right-hand sides of definition `d`, or of the members of
    /// class definition `d`, the definition as owner.
    pub fn own_right_hand_sides(&mut self, d: TNodeId) {
        match *self.tpd.tree(d) {
            T::ValDef { rhs: Some(rhs), .. } | T::DefDef { rhs: Some(rhs), .. } => {
                if let Some(s) = self.tpd.sym(d) {
                    self.reown(rhs, s);
                }
            }
            T::TypeDef { rhs, .. } => {
                if let T::Template { body, .. } = *self.tpd.tree(rhs) {
                    for m in self.tpd.items(body).to_vec() {
                        self.own_right_hand_sides(m);
                    }
                }
            }
            _ => {}
        }
    }

    /// Enters definition `d` a macro annotation made among the declarations of `owner` if it is
    /// a class, and the members of a class `d` defines among its own (dotty's
    /// `enterMissingSymbols`); a refusal for a definition of another owner.
    pub fn enter_made(&mut self, d: TNodeId, owner: SymbolId, span: Span) -> Result<(), Unsupported> {
        let s = self.tpd.sym(d).filter(|_| is_definition(self.tpd.tree(d)));
        let s = s.ok_or((span, "a macro annotation returning other than definitions"))?;
        if self.u.symbol(s).owner != Some(owner) {
            return Err((span, "a macro annotation adding a definition of another owner"));
        }
        if self.u.symbol(owner).kind == Kind::Class && !self.u.symbol(owner).decls.contains(&s) {
            self.u.enter(s);
        }
        if let T::TypeDef { rhs, .. } = *self.tpd.tree(d)
            && let T::Template { body, .. } = *self.tpd.tree(rhs)
        {
            for m in self.tpd.items(body).to_vec() {
                if is_definition(self.tpd.tree(m)) {
                    self.enter_made(m, s, span)?;
                }
            }
        }
        Ok(())
    }
}

pub fn is_definition(t: &T) -> bool {
    matches!(t, T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
}
