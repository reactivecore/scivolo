// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The extractor applied to the scrutinee and its implicit clauses, `unapply(x)(using …)`
//! (dotty's `typedUnApply` typing `unapplyFn(dummyArg)` and its `unapplyImplicits`).

use super::super::Typer;
use super::super::dependent_params::applied_result;
use super::super::type_ops::TypeOps;
use super::UnapplyPattern;
use super::extractor::MethodShape;
use crate::ctx::Mode;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use tasty::{SymbolId, Type, TypeId, Universe};

impl UnapplyPattern {
    /// Applies extractor `unapp` to the arguments the search finds for the implicit clauses
    /// before its scrutinee, as `unapply(using Ord[T])(x: T)` (dotty's `adapt` of `unapplyFn`
    /// to `FunProto(dummyArg)`).
    pub fn leading_implicits_applied(
        &self,
        t: &mut Typer<'_>,
        mut unapp: TNodeId,
    ) -> Result<TNodeId, &'static str> {
        loop {
            let tpe = t.tpd.tpe(unapp);
            let info = t.method_info(tpe).unwrap_or(tpe);
            if !matches!(t.u.types().get(info), Type::Method { implicit: true, .. }) {
                return Ok(unapp);
            }
            unapp = t.implicit_clause_applied(unapp, self.ctx.with_mode(Mode::NONE))?;
        }
    }

    /// Applies extractor `unapp`, applied to a value of type `own` with result type `result`, to
    /// the arguments the search finds for its implicit clauses, as `unapply(x)(using Ord[T])`;
    /// the result after them, and those arguments in one list. Refuses lifted implicit
    /// arguments and further explicit parameter lists.
    pub fn unapply_implicits(
        &self,
        t: &mut Typer<'_>,
        unapp: TNodeId,
        result: TypeId,
        own: TypeId,
    ) -> Result<(TypeId, Vec<TNodeId>), &'static str> {
        if !matches!(t.u.types().get(result), Type::Method { implicit: true, .. }) {
            return Ok((result, Vec::new()));
        }

        let span = t.tpd.span(unapp);
        let dummy = t.build().literal(Constant::Null, span.end_pos());
        t.tpd.get_mut(dummy).tpe = Some(own);
        let mut app = t.build().apply(unapp, &[dummy], result, span);
        let mut implicits = Vec::new();
        loop {
            let applied = t.implicit_clause_applied(app, self.ctx.with_mode(Mode::NONE))?;
            let T::Apply { fun, args } = *t.tpd.tree(applied) else {
                return Err("extractor with implicits");
            };
            if fun != app {
                return Err("extractor with lifted implicit arguments");
            }
            implicits.extend_from_slice(t.tpd.items(args));
            let tpe = t.tpd.tpe(applied);
            match t.u.types().get(tpe) {
                Type::Method { implicit: true, .. } => app = applied,
                Type::Method { .. } | Type::Poly { .. } => return Err("extractor with more parameters"),
                _ => return Ok((tpe, implicits)),
            }
        }
    }
}

/// The result type of extractor method `sym` of method type `method` applied to a value of type
/// `own`: an intersection `(?1 : S) & T` with the skolem of an unstable `own` as `S & T`
/// simplified, type variables instantiated.
pub fn applied_unapply_result(
    t: &mut TypeOps<'_>,
    sym: Option<SymbolId>,
    method: &MethodShape,
    own: TypeId,
) -> TypeId {
    let names = [method.param.clone()];
    let applied = applied_result(t.u, method.result, (method.binder, &names), sym, &[own]);
    let skolem = |u: &Universe, x| match *u.types().get(x) {
        Type::Skolem(_, w) => Some(w),
        _ => None,
    };
    let result = t.u.map_type(applied, &mut |u, part| {
        let Type::And(a, b) = *u.types().get(part) else { return None };
        match (skolem(u, a), skolem(u, b)) {
            (Some(w), None) => Some(u.glb(w, b)),
            (None, Some(w)) => Some(u.glb(a, w)),
            _ => None,
        }
    });
    t.zonk(result)
}
