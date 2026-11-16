// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/FormatChecker.scala

use super::Typer;
use super::implicits::Goal;
use crate::ctx::Ctx;
use crate::tpd::{FormatView, TNodeId, Target, Tree as T};
use tasty::SymbolId;
use tasty::tags::INLINE;

/// The method whose calls the lowering checks as format interpolations.
const STRING_CONTEXT_F: &str = "scala.StringContext.f";

impl Typer<'_> {
    /// Records the implicit views of the arguments `args` of a call of `meth` when that is
    /// `StringContext.f` (dotty's `TypedFormatChecker.argConvertsTo`, which the lowering asks for
    /// the targets the format check needs): for each format target an argument does not conform
    /// to, the conversion the search in `ctx` selects. Unknown, and refused by the lowering if
    /// needed, are views by a local implicit value or parameter, which dotty's search after the
    /// typer does not see, views by inline methods, and searches the typer does not support. An
    /// argument whose type is not yet fully defined gets no record.
    pub fn record_format_views(&mut self, meth: Option<SymbolId>, args: &[TNodeId], ctx: Ctx) {
        let Some(m) = meth.filter(|&m| self.u.symbol(m).name == "f") else { return };
        if self.u.full_name(m) != STRING_CONTEXT_F {
            return;
        }
        let Some(&seq) = args.last() else { return };
        let T::Typed { expr, .. } = *self.tpd.tree(seq) else { return };
        let T::SeqLiteral { elems, .. } = *self.tpd.tree(expr) else { return };
        for arg in self.tpd.items(elems).to_vec() {
            if !self.is_fully_defined(self.tpd.tpe(arg)) {
                continue;
            }
            let views =
                Target::CLASSES.iter().filter_map(|&t| Some((t, self.format_view(arg, t, ctx)?))).collect();
            self.tpd.set_format_views(arg, views);
        }
    }

    /// The view of `arg` to `target`; `None` if it conforms or none applies.
    fn format_view(&mut self, arg: TNodeId, target: Target, ctx: Ctx) -> Option<FormatView> {
        let class = self.u.class_ref(target.class_name()?);
        let from = self.zonk(self.tpd.tpe(arg));
        if self.types().frozen_conforms(from, class) {
            return None;
        }
        match self.converted(arg, Goal::Conform(class), ctx) {
            Ok(None) => None,
            Ok(Some(view)) if self.is_global_view(view) => Some(FormatView::Converted(view)),
            _ => Some(FormatView::Unknown),
        }
    }

    /// Whether conversion `view` calls a method that is not inline on a member, not on a local
    /// value or parameter.
    fn is_global_view(&self, view: TNodeId) -> bool {
        let meth = self.tpd.meth_part(view);
        if self.tpd.sym(meth).is_some_and(|m| self.u.symbol(m).flags.has(INLINE)) {
            return false;
        }
        let mut t = meth;
        while let T::Apply { fun: inner, .. }
        | T::TypeApply { fun: inner, .. }
        | T::Select { qualifier: inner, .. } = *self.tpd.tree(t)
        {
            t = inner;
        }
        let owner = self.tpd.sym(t).and_then(|s| self.u.symbol(s).owner);
        !matches!(*self.tpd.tree(t), T::Ident { .. })
            || owner.is_none_or(|o| !self.u.symbol(o).kind.is_term())
    }
}
