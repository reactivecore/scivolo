//! Definitions outside the subset as far as inlining goes.

use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::tree::DerivedKind;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// What about definition `tree` of `kind` with `flags` in `owner` is outside the subset as
    /// far as inlining goes: macros, erased definitions, case classes in an inline method, nested
    /// inline methods, transparent inline givens and inline givens with a class body.
    pub fn inline_problem(
        &self,
        tree: NodeId,
        kind: Kind,
        owner: SymbolId,
        flags: FlagSet,
    ) -> Option<&'static str> {
        let mut outer = Some(owner);
        let mut nested = false;
        while let Some(o) = outer {
            let s = self.u.symbol(o);
            nested |= s.kind == Kind::Def && s.flags.has(tasty::tags::INLINE);
            outer = s.owner;
        }
        if flags.is(f::MACRO) {
            return Some("macro definition");
        }
        if flags.is(f::ERASED) {
            return Some("erased definition");
        }
        if nested && kind == Kind::Class && flags.is(f::CASE) {
            return Some("case class defined in an inline method");
        }
        if !flags.is(f::INLINE) {
            return None;
        }
        Some(if nested {
            "inline definition nested in an inline method"
        } else if flags.is_all_of(f::TRANSPARENT.or(f::GIVEN)) {
            "transparent inline given"
        } else if flags.is(f::GIVEN) && self.makes_given_class(tree) {
            "inline given with a class body"
        } else if flags.is_one_of(f::MUTABLE.or(f::PARAM_ACCESSOR).or(f::TERM_PARAM)) {
            "inline variable or class parameter"
        } else if kind != Kind::Def && kind != Kind::Val {
            "inline definition"
        } else {
            return None;
        })
    }

    /// Whether method definition `tree` is the instance desugaring makes of a given class: its
    /// body creates the class through a derived type reference.
    fn makes_given_class(&self, tree: NodeId) -> bool {
        let Tree::DefDef { rhs: Some(mut t), .. } = *self.ast.tree(tree) else { return false };
        loop {
            match *self.ast.tree(t) {
                Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => t = fun,
                Tree::Select { qualifier, .. } => t = qualifier,
                Tree::New { tpt } => {
                    let tpt = match *self.ast.tree(tpt) {
                        Tree::AppliedTypeTree { tpt, .. } => tpt,
                        _ => tpt,
                    };
                    return matches!(
                        self.ast.tree(tpt),
                        Tree::DerivedTypeTree { kind: DerivedKind::TypeRef, .. }
                    );
                }
                _ => return false,
            }
        }
    }
}
