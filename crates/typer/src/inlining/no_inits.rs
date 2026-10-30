// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Classes whose construction runs no code (dotty's `NoInits` and `isNoInitsRealClass`), the
//! unit's classes judged by their typed templates (dotty's `parentsKind` and `bodyKind`).

use super::{Inlining, Purity};
use crate::tpd::{TNodeId, Tree as T};
use tasty::SymbolId;
use tasty::tags::{EXTENSION, SYNTHETIC, TRAIT};

impl Inlining<'_> {
    /// Records class definition `t` of the unit if its template runs code.
    pub fn note_inits(&mut self, t: TNodeId) {
        let (&T::TypeDef { rhs, .. }, Some(cls)) = (self.tpd.tree(t), self.tpd.sym(t)) else { return };
        let T::Template { parents, body, .. } = *self.tpd.tree(rhs) else { return };
        let parent_calls = !self.u.symbol(cls).flags.has(TRAIT)
            && self.tpd.items(parents).iter().any(|&p| match self.tpd.tree(p) {
                T::Apply { args, .. } => !args.is_empty(),
                T::Block { .. } => true,
                _ => false,
            });
        let body = self.tpd.items(body).to_vec();
        if parent_calls || !body.into_iter().all(|s| self.is_no_init_member(s)) {
            self.with_inits.insert(cls);
        }
    }

    fn is_no_init_member(&mut self, s: TNodeId) -> bool {
        let sym = self.tpd.sym(s);
        match *self.tpd.tree(s) {
            T::Import { .. } | T::TypeDef { .. } => true,
            T::DefDef { .. } => !sym.is_some_and(|d| self.u.symbol(d).flags.has(EXTENSION)),
            T::ValDef { rhs, .. } => {
                rhs.is_none()
                    || sym.is_some_and(|v| self.u.is_module(v) && self.u.symbol(v).flags.has(SYNTHETIC))
            }
            _ => false,
        }
    }
}

impl Purity<'_> {
    /// Whether neither class `c` nor any of its base classes runs code when an instance is made;
    /// never for a `java.*` or `javax.*` class other than `Object`, not an interface.
    pub fn is_no_inits_class(&mut self, c: SymbolId) -> bool {
        let bases = self.u.linearization(c);
        bases.iter().all(|&b| {
            let name = self.u.full_name(b);
            let java = (name.starts_with("java.") || name.starts_with("javax."))
                && name != "java.lang.Object"
                && !self.u.symbol(b).flags.has(TRAIT);
            !java && !self.with_inits.contains(&b) && self.u.no_inits(b)
        })
    }
}
