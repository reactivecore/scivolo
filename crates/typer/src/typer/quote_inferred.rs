// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! Quotes under capture checking: the inferred type arguments of inline methods annotated
//! `@caps.internal.inferred` (dotty's `PostTyper.markInferred`), as the quote's pickle keeps them.

use super::Typer;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Tree;
use tasty::Type;
use tasty::tags::INLINE;

impl Typer<'_> {
    /// Whether the unit writes a capture set, an annotation, a function or a by-name type.
    pub fn has_capture_syntax(&self) -> bool {
        self.ast.exists_subtree(self.unit, &|a, n| match a.tree(n) {
            Tree::CapturesAndResult { .. } | Tree::UseRef { .. } | Tree::Annotated { .. } => true,
            Tree::Function { .. } | Tree::ByNameTypeTree { .. } => a.is_type(n),
            _ => false,
        })
    }

    /// Whether a term under quote body `body` outside its splices refers to a member whose type
    /// has a capture-checking annotation, which the types drop.
    pub fn refers_to_capturing(&mut self, body: TNodeId) -> bool {
        let mut todo = vec![body];
        while let Some(t) = todo.pop() {
            if !self.is_splice(t) {
                todo.extend(self.tpd.children(t));
            }
            let Some(s) = self.tpd.sym(t).filter(|&s| self.u.symbol(s).kind.is_term()) else { continue };
            let info = self.u.info(s);
            let mut found = false;
            self.u.map_type(info, &mut |u, x| {
                if let Type::Annotated(_, a) = *u.types().get(x) {
                    found |= u.class_symbol(a).is_some_and(|c| u.full_name(c).starts_with("scala.annotation.retains"));
                }
                None
            });
            if found {
                return true;
            }
        }
        false
    }

    /// Marks the inferred type arguments of inline methods under `body`.
    pub fn mark_inferred(&mut self, body: TNodeId) {
        let Some(inferred) = self.resolve_full_name("scala.caps.internal$.inferred", true) else { return };
        let inferred = type_ref(self.u, inferred);
        let mut todo = vec![body];
        while let Some(t) = todo.pop() {
            todo.extend(self.tpd.children(t));
            let T::TypeApply { fun, args } = *self.tpd.tree(t) else { continue };
            if !self.tpd.sym(fun).is_some_and(|f| self.u.symbol(f).flags.has(INLINE)) {
                continue;
            }
            for arg in self.tpd.items(args).to_vec() {
                let arg = match *self.tpd.tree(arg) {
                    T::NamedArg { arg, .. } => arg,
                    _ => arg,
                };
                if let T::TypeTree { inferred: true } = self.tpd.tree(arg)
                    && let Some(tpe) = self.tpd.get(arg).tpe
                {
                    let marked = self.u.intern_type(Type::Annotated(tpe, inferred));
                    let node = self.tpd.get_mut(arg);
                    node.tree = T::TypeTree { inferred: false };
                    node.tpe = Some(marked);
                }
            }
        }
    }
}
