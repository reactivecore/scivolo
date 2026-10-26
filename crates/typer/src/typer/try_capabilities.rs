// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The `CanThrow` capabilities a `try` gives its body under `saferExceptions`: an erased given
//! for the checked exceptions its cases catch (dotty's `addCanThrowCapabilities`).

use super::Typer;
use crate::ctx::Ctx;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, TName, Tree};
use syntax::config::Features;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Body `expr` of a `try` with cases `cases` as `{ erased given canThrow$N: CanThrow[E1 | …]
    /// = caps.internal.erasedValue; expr }` for the checked exceptions `E1, …` the cases catch, if
    /// `saferExceptions` is enabled and they catch one; `expr` otherwise.
    pub fn with_can_throw(&mut self, expr: NodeId, cases: &[NodeId], ctx: Ctx) -> NodeId {
        if !self.feature_enabled(Features::SAFER_EXCEPTIONS, "experimental.saferExceptions", ctx) {
            return expr;
        }
        let mut caught: Option<TypeId> = None;
        for &c in cases {
            let Tree::CaseDef { pat, .. } = *self.ast.tree(c) else { continue };
            let pat = match *self.ast.tree(pat) {
                Tree::Bind { body, .. } => body,
                _ => pat,
            };
            let Tree::Typed { tpt, .. } = *self.ast.tree(pat) else { continue };
            let typed = self.typed_type(tpt, ctx);
            let tpe = self.tpd.tpe(typed);
            if self.is_checked_exception(tpe) {
                caught = Some(caught.map_or(tpe, |c| self.u.intern_type(Type::Or(c, tpe, true))));
            }
        }
        let (Some(caught), Some(can_throw)) = (caught, self.resolve_full_name("scala.CanThrow", true)) else {
            return expr;
        };
        let can_throw = crate::defn::type_ref(self.u, can_throw);
        let tpe = self.u.intern_type(Type::Applied(can_throw, vec![caught]));
        let span = self.ast.span(expr).to_synthetic();
        let at = span.start_pos();
        let typed = self.type_tree(tpe, at, false);
        let tpt = self.ast.node(Tree::TypeTree);
        self.ast.with_span(tpt, at);
        self.splices.insert(tpt, typed);
        let value = TName { name: self.name("scala.caps.internal$.erasedValue"), is_type: false };
        let rhs = self.ast.node(Tree::TypedSplice { sym: value });
        self.ast.with_span(rhs, at);
        let name = TName { name: self.fresh.next(self.names, "canThrow$"), is_type: false };
        let mods = self.ast.mods(Modifiers::with_flags(f::GIVEN.or(f::FINAL).or(f::ERASED)));
        let given = self.ast.node(Tree::ValDef { name, tpt, rhs: Some(rhs), mods });
        self.ast.with_span(given, span.with_point(span.start()));
        let stats = self.ast.list(&[given]);
        let block = self.ast.node(Tree::Block { stats, expr: Some(expr) });
        self.ast.with_span(block, span);
        block
    }

    /// Checks that a `CanThrow` capability for the exception `thrown` is in scope if it is a
    /// checked one and `saferExceptions` is enabled (dotty's `checkCanThrow`).
    pub fn check_can_throw(&mut self, thrown: crate::tpd::TNodeId, ctx: Ctx) -> Result<(), &'static str> {
        if !self.feature_enabled(Features::SAFER_EXCEPTIONS, "experimental.saferExceptions", ctx) {
            return Ok(());
        }
        let tpe = self.tpd.tpe(thrown);
        let tpe = self.u.widen(tpe);
        let Some(can_throw) = self.resolve_full_name("scala.CanThrow", true) else { return Ok(()) };
        if !self.is_checked_exception(tpe) {
            return Ok(());
        }
        let can_throw = crate::defn::type_ref(self.u, can_throw);
        let capability = self.u.intern_type(Type::Applied(can_throw, vec![tpe]));
        let span = self.tpd.span(thrown);
        match self.implicit_arg(capability, span, ctx, 0) {
            Ok(_) => Ok(()),
            Err(_) => Err("throw without a CanThrow capability"),
        }
    }

    /// Whether `t` derives from `Exception` but not from `RuntimeException` (dotty's
    /// `isCheckedException`).
    fn is_checked_exception(&mut self, t: TypeId) -> bool {
        let class = |s: &mut Self, name: &str| s.resolve_full_name(name, true);
        let (Some(exception), Some(runtime)) =
            (class(self, "java.lang.Exception"), class(self, "java.lang.RuntimeException"))
        else {
            return false;
        };
        self.u
            .class_symbol(t)
            .is_some_and(|c| self.u.derives_from(c, exception) && !self.u.derives_from(c, runtime))
    }
}
