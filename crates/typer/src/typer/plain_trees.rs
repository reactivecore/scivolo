//! Trees dotty types the same whatever the expected type, and references whose types do not rest
//! on what the typer infers.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::Tree;
use tasty::tags::{ABSTRACT, INLINE, TRAIT};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Whether dotty types `tree` the same whatever the expected type: a literal, `this`, a
    /// reference to a member that is not overloaded, or an application of such a method that takes
    /// neither type parameters nor implicits.
    pub fn is_plain_tree(&mut self, tree: TNodeId) -> bool {
        match *self.tpd.tree(tree) {
            T::Literal { .. } | T::This { .. } => true,
            T::Ident { .. } | T::Select { .. } => {
                self.tpd.sym(tree).is_some_and(|s| self.is_single_member(tree, s))
            }
            T::Apply { fun, .. } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let explicit = self.u.method(info).is_some_and(|m| !m.implicit);
                let new = match *self.tpd.tree(fun) {
                    T::Select { qualifier, .. } if matches!(self.tpd.tree(qualifier), T::New { .. }) => {
                        let class = self.tpd.tpe(qualifier);
                        Some(self.u.class_symbol(class).is_some_and(|c| self.is_concrete_class(c)))
                    }
                    _ => None,
                };
                let written = new.is_some() || !self.tpd.span(fun).is_synthetic();
                let plain_fun =
                    matches!(self.tpd.tree(fun), T::Ident { .. } | T::Select { .. } | T::Apply { .. });
                new != Some(false) && explicit && plain_fun && written && self.is_plain_tree(fun)
            }
            _ => false,
        }
    }

    /// Whether reference `tree` to `s`, not an inline method and of a settled type, finds no other
    /// term or type member, as `s` is, of that name in its prefix's class or else its owner, or `s`
    /// is local or top-level.
    fn is_single_member(&mut self, tree: TNodeId, s: SymbolId) -> bool {
        if self.u.symbol(s).flags.has(INLINE) || !self.type_is_settled(s) {
            return false;
        }
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if self.u.symbol(owner).kind.is_term() || self.u.symbol(owner).kind == Kind::Package {
            return true;
        }
        let prefix = match self.u.sym_ref(self.tpd.tpe(tree)) {
            Some((Some(p), _)) if !matches!(self.u.ty(p), Type::Package(_)) => Some(p),
            _ => None,
        };
        let class = match prefix {
            Some(p) => {
                let widened = self.u.widen(p);
                self.u.class_symbol(widened)
            }
            None => Some(owner).filter(|&o| self.u.symbol(o).kind == Kind::Class),
        };
        let Some(class) = class else { return false };
        let (name, is_type) = (self.u.symbol(s).name.clone(), self.u.is_type_symbol(s));
        let members = self.class_members(class, &name);
        members.into_iter().filter(|&m| self.u.is_type_symbol(m) == is_type).count() == 1
    }

    /// Whether the type of `s` does not rest on what the typer infers for its definition in this
    /// unit: a constructor's, written, or that of a literal right-hand side; not a closure's
    /// parameter's.
    pub fn type_is_settled(&self, s: SymbolId) -> bool {
        if self.u.symbol(s).name == nme::CONSTRUCTOR {
            return true;
        }
        if self.syms.closure_syms.contains(&s) {
            return false;
        }
        let Some((&id, _)) = self.syms.by_tree.iter().find(|&(_, &v)| v == s) else { return true };
        let (Tree::ValDef { tpt, rhs, .. } | Tree::DefDef { tpt, rhs, .. }) = *self.ast.tree(id) else {
            return true;
        };
        let literal =
            rhs.is_some_and(|r| matches!(self.ast.tree(r), Tree::Literal { .. } | Tree::Number { .. }));
        literal || !self.is_inferred_tpt(tpt)
    }

    /// Whether `new` may instantiate class `c`: neither abstract nor a trait.
    pub fn is_concrete_class(&self, c: SymbolId) -> bool {
        let flags = self.u.symbol(c).flags;
        !flags.has(ABSTRACT) && !flags.has(TRAIT)
    }
}
