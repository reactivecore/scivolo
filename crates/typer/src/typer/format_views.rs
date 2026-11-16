// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/FormatChecker.scala

use super::Typer;
use super::implicits::Goal;
use crate::ctx::Ctx;
use crate::tpd::{FormatView, List, TNodeId, Target, Tree as T};
use tasty::tags::{INLINE, LOCAL, PARAMsetter, PRIVATE, PROTECTED};
use tasty::{Kind, SymbolId, Type};

/// The method whose calls the lowering checks as format interpolations.
const STRING_CONTEXT_F: &str = "scala.StringContext.f";

impl Typer<'_> {
    /// Records the implicit views of the arguments `args` of a call of `meth` when that is
    /// `StringContext.f` (dotty's `TypedFormatChecker.argConvertsTo`, which the lowering asks for
    /// the targets the format check needs): for each format target an argument does not conform
    /// to, the conversion the search in `ctx` selects. Unknown, and refused by the lowering if
    /// needed, are views that refer to a local symbol, a protected member, or a private value or
    /// class parameter, views by inline methods or of by-name parameters, and searches the typer
    /// does not support. An argument whose type is not yet fully defined gets no record.
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
            Ok(Some(view)) if self.is_global_view(view, arg) => Some(FormatView::Converted(view)),
            _ => Some(FormatView::Unknown),
        }
    }

    /// Whether conversion `view` of `arg` calls a method that is not inline and takes `arg` by
    /// value, and refers outside `arg` to global symbols only.
    fn is_global_view(&mut self, view: TNodeId, arg: TNodeId) -> bool {
        let meth = self.tpd.meth_part(view);
        if self.tpd.sym(meth).is_none_or(|m| self.u.symbol(m).flags.has(INLINE)) {
            return false;
        }

        let mut todo = vec![view];
        while let Some(t) = todo.pop() {
            if t == arg {
                continue;
            }
            if let Some(s) = self.tpd.sym(t)
                && !self.is_global_ref(s)
            {
                return false;
            }
            if let T::Apply { fun, args } = *self.tpd.tree(t)
                && self.tpd.items(args).contains(&arg)
                && !self.passes_by_value(fun, args, arg)
            {
                return false;
            }
            self.tpd.extend_children(t, &mut todo);
        }
        true
    }

    /// Whether `s` is a type, an object, or a member that is not protected and, if a value, neither
    /// private nor a class parameter.
    fn is_global_ref(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        if !sym.kind.is_term() || self.u.is_module(s) {
            return true;
        }
        if self.u.is_local(s) || sym.flags.has(PROTECTED) {
            return false;
        }
        sym.kind == Kind::Def || ![PRIVATE, LOCAL, PARAMsetter].iter().any(|&f| sym.flags.has(f))
    }

    /// Whether `fun(args)` passes `arg` to a parameter that is not by-name.
    fn passes_by_value(&mut self, fun: TNodeId, args: List, arg: TNodeId) -> bool {
        let Some(i) = self.tpd.items(args).iter().position(|&a| a == arg) else { return false };
        let fun_tpe = self.tpd.tpe(fun);
        let tpe = self.u.widen(fun_tpe);
        let Some(param) = self.u.method(tpe).and_then(|m| m.params.get(i).map(|&(_, p)| p)) else {
            return false;
        };
        !matches!(*self.u.ty(param), Type::ByName(_))
    }
}
