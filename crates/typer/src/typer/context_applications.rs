// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! A context function value applied to its implicit arguments (dotty's `adaptNoArgsOther`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};

impl Typer<'_> {
    /// `tree`, a value of a context function type, applied to the implicit arguments of its
    /// `apply` for `pt`; `None` if it is no such value.
    pub fn context_function_applied(
        &mut self,
        tree: TNodeId,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<Result<TNodeId, &'static str>> {
        let type_tree = self.tpd.sym(tree).is_some_and(|s| self.u.is_type_symbol(s))
            || matches!(self.tpd.tree(tree), T::Select { name, .. } if name.is_type);
        if ctx.mode.is(Mode::PATTERN)
            || ctx.mode.is(Mode::ASSIGNMENT_LHS.with(Mode::SINGLETON_REF))
            || type_tree
            || self.is_closure_tree(tree)
        {
            return None;
        }
        let tpe = self.tpd.tpe(tree);
        let wide = self.u.widen(tpe);
        if self.context_function_parts(wide).is_none() {
            let wide = self.u.dealias(wide);
            if self.dependent_context_function(wide).is_none() {
                self.erased_context_function(wide)?;
            }
        }
        let tree = self.inlined_transparent(tree, Pt::Any, ctx).unwrap_or(tree);
        let span = self.tpd.span(tree);
        let receiver_call = self.is_extension_receiver_call(tree);
        if receiver_call {
            self.tpd.get_mut(tree).span = span.to_synthetic();
        }
        let applied = self.with_apply(tree, Pt::Any, ctx).map(|apply| self.adapt(apply, pt, ctx));
        if let Ok(t) = applied
            && receiver_call
            && self.tpd.span(t).start() == span.start()
            && self.tpd.span(t).end() == span.end()
        {
            self.tpd.get_mut(t).span = span;
        }
        Some(applied)
    }

    /// Whether `t` applies an extension method to its receiver only, as a selection `x.m` of
    /// the method types.
    fn is_extension_receiver_call(&self, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        let fun = match *self.tpd.tree(fun) {
            T::TypeApply { fun, .. } => fun,
            _ => fun,
        };
        !matches!(self.tpd.tree(fun), T::Apply { .. })
            && self.tpd.sym(fun).is_some_and(|m| self.u.symbol(m).flags.has(tasty::tags::EXTENSION))
    }

    /// Whether typed tree `t` is a closure, possibly in blocks (dotty's `isContextualClosure` of a
    /// context function value).
    fn is_closure_tree(&self, t: TNodeId) -> bool {
        match *self.tpd.tree(t) {
            T::Closure { .. } => true,
            T::Block { stats, expr } => {
                matches!(self.tpd.tree(expr), T::Closure { .. })
                    || self.tpd.items(stats).is_empty() && self.is_closure_tree(expr)
            }
            _ => false,
        }
    }
}
