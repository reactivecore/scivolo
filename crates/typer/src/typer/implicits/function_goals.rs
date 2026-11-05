// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Implicit methods as values of a function type (dotty's `candidateKind` for a `FunctionN`
//! goal, and `typedImplicit` adapting the reference).

use super::super::{Pt, Typer};
use super::search::MISFITS;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::diag::Level;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type `t` of implicit `sym` as a value compared with `pt`: its implicit clauses stripped, a
    /// method as a function type if `pt` is a `FunctionN` type (dotty's `normalize`); `None` if
    /// it stays a method.
    pub fn implicit_value_type(&mut self, t: TypeId, sym: SymbolId, pt: TypeId) -> Option<TypeId> {
        let t = self.strip_implicit(t);
        let method = matches!(self.u.types().get(t), Type::Method { .. });
        let t = match method && self.is_function_type(pt) {
            true => self.proto_normalized((t, Some(sym)), pt, true, 8)?,
            false => t,
        };
        (!matches!(self.u.types().get(t), Type::Method { .. } | Type::Poly { .. })).then_some(t)
    }

    /// Method reference `tree` eta-expanded to function type `pt`; a misfit if it refers to an
    /// inline implicit method, typing the function reports an error, or fails by a misfit.
    pub fn eta_expanded_implicit(
        &mut self,
        tree: TNodeId,
        pt: TypeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        if self.is_inline_implicit_ref(tree) {
            return Err("implicit of another type");
        }
        self.typed_as_implicit(|t| t.adapt(tree, Pt::Type(pt), ctx))
    }

    /// Whether implicit `sym` of type `t` fits `pt`: `Some(true)` if its value type conforms,
    /// `Some(false)` if it does once context function types on either side are replaced by their
    /// results (dotty's `normalize` following them).
    pub fn implicit_fit(&mut self, t: TypeId, sym: SymbolId, pt: TypeId) -> Option<bool> {
        let value = self.implicit_value_type(t, sym, pt)?;
        if self.conforms(value, pt) {
            return Some(true);
        }
        let (result, pt_result) = (self.context_result(value), self.context_result(pt));
        ((result != value || pt_result != pt) && self.conforms(result, pt_result)).then_some(false)
    }

    /// `tree`, a value of a context function type, applied to its implicit arguments for `pt`.
    pub fn context_applied_implicit(
        &mut self,
        tree: TNodeId,
        pt: TypeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        self.typed_as_implicit(|t| match t.context_function_applied(tree, Pt::Type(pt), ctx) {
            Some(Ok(applied)) => applied,
            Some(Err(what)) => t.unsupported_tree(t.tpd.span(tree), what),
            None => tree,
        })
    }

    /// Candidate reference `tree` as an argument of context function type `pt`: a context
    /// closure around it typed with its parameters in scope (dotty's `typedImplicit` of a context
    /// function goal).
    pub fn contextual_implicit(
        &mut self,
        tree: TNodeId,
        pt: TypeId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let splice = self.ast.node(syntax::ast::Tree::TypeTree);
        self.ast.with_span(splice, self.tpd.span(tree));
        self.splices.insert(splice, tree);
        let closure = self.typed_as_implicit(|t| t.typed(splice, Pt::Type(pt), ctx))?;
        let t = self.tpd.tpe(closure);
        if self.conforms(t, pt) { Ok(closure) } else { Err("implicit of another type") }
    }

    /// The tree `typing` gives; a misfit if it reports an error or fails by a misfit.
    fn typed_as_implicit(
        &mut self,
        typing: impl FnOnce(&mut Self) -> TNodeId,
    ) -> Result<TNodeId, &'static str> {
        let (outer, diags) = (self.unsupported.take(), self.diags.items.len());
        let adapted = typing(self);
        let inner = std::mem::replace(&mut self.unsupported, outer);
        if self.diags.items[diags..].iter().any(|d| d.level == Level::Error) {
            self.diags.items.truncate(diags);
            return Err("implicit of another type");
        }
        let Some((span, what)) = inner else { return Ok(adapted) };
        if let Some(misfit) = MISFITS.iter().find(|&&m| m == what) {
            return Err(misfit);
        }
        self.unsupported.get_or_insert((span, what));
        Err("implicit method as a function value")
    }
}
