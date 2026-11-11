// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The extractor method of a pattern, selected and applied up to its scrutinee parameter
//! (dotty's `unapplyFn` in `typedUnApply`).

use super::super::Typer;
use super::{UnapplyPattern, is_seq_wildcard};
use crate::ctx::Mode;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::{NodeId, Tree};
use tasty::{Binder, NameId, Type, TypeId};

/// The extractor method of a pattern, applied to everything before its scrutinee.
pub struct Extractor {
    /// `C.unapply` with its type arguments, or type variables, and the implicit clauses before
    /// its scrutinee applied.
    pub fun: TNodeId,
    /// Whether it is an `unapplySeq`.
    pub seq: bool,
    /// The number of type variables of the constraint before those of the extractor.
    pub vars: usize,
    /// The method type of `fun`.
    pub method: MethodShape,
}

/// The method type of an extractor: one scrutinee parameter, then at most implicit clauses.
pub struct MethodShape {
    /// The binder through which the result refers to the scrutinee parameter.
    pub binder: Binder,
    /// The name of the scrutinee parameter.
    pub param: NameId,
    /// The type of the scrutinee parameter.
    pub arg_type: TypeId,
    /// The result type, the implicit clauses still to apply included.
    pub result: TypeId,
}

impl UnapplyPattern {
    /// Selects the extractor method of `fun`, `C`, `C[Ts]` or `new C[T]`, and applies it to its
    /// type arguments, or type variables for its type parameters, and to the implicit clauses
    /// before its scrutinee; a transparent inline extractor is expanded. Refuses type arguments
    /// of other extractors, a sequence wildcard without `unapplySeq`, and methods that take more
    /// than the scrutinee and implicit clauses.
    pub fn extractor(&self, t: &mut Typer<'_>, fun: NodeId) -> Result<Extractor, &'static str> {
        let applied = fun;
        let type_args = self.unapply_type_args(t, fun);
        let fun = type_args.map_or(fun, |(qual, _)| qual);
        let with_targs = matches!(t.ast.tree(fun), Tree::TypeApply { .. });
        if !with_targs
            && !t.splices.contains_key(&fun)
            && !matches!(t.ast.tree(fun), Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. })
        {
            return Err("extractor with type arguments");
        }

        let (unapp, seq) = self.unapply_fun(t, fun)?;
        if with_targs && !is_new_instance(t, unapp) {
            return Err("extractor with type arguments");
        }
        if with_targs && let T::Select { qualifier, .. } = *t.tpd.tree(unapp) {
            let span = t.tpd.span(qualifier);
            t.tpd.get_mut(qualifier).span = span.to_synthetic();
        }
        if !seq && self.args.iter().any(|&a| is_seq_wildcard(t, a)) {
            return Err("sequence wildcard of an extractor without unapplySeq");
        }

        let vars = t.constraint.count();
        let unapp = match type_args {
            Some((_, args)) => self.with_unapply_type_args(t, applied, unapp, args)?,
            None => t.with_type_vars(unapp)?,
        };
        let unapp = self.leading_implicits_applied(t, unapp)?;
        let unapp = self.transparent_unapply(t, unapp)?;
        let method = method_shape(t, unapp)?;
        Ok(Extractor { fun: unapp, seq, vars, method })
    }

    /// Applies extractor `unapp` to the arguments the search finds for the implicit clauses
    /// before its scrutinee, as `unapply(using Ord[T])(x: T)` (dotty's `adapt` of `unapplyFn`
    /// to `FunProto(dummyArg)`).
    fn leading_implicits_applied(
        &self,
        t: &mut Typer<'_>,
        mut unapp: TNodeId,
    ) -> Result<TNodeId, &'static str> {
        loop {
            let tpe = t.tpd.tpe(unapp);
            let info = t.method_info(tpe).unwrap_or(tpe);
            if t.u.method(info).is_none_or(|m| !m.implicit) {
                return Ok(unapp);
            }
            unapp = t.implicit_clause_applied(unapp, self.ctx.with_mode(Mode::NONE))?;
        }
    }
}

/// Finds the method type of extractor `unapp`; refused unless it takes one parameter and then
/// at most implicit clauses.
fn method_shape(t: &mut Typer<'_>, unapp: TNodeId) -> Result<MethodShape, &'static str> {
    let fun_tpe = t.tpd.tpe(unapp);
    let info = t.method_info(fun_tpe).unwrap_or(fun_tpe);
    let m = t.u.method(info).filter(|m| !m.implicit).ok_or("not an extractor")?;
    let (binder, params, result) = (m.binder, m.params.to_vec(), m.result);
    let [(param, arg_type)] = params[..] else { return Err("not an extractor") };
    let implicit = t.u.method(result).is_some_and(|m| m.implicit);
    if !implicit && matches!(t.u.ty(result), Type::Method { .. } | Type::Poly { .. }) {
        return Err("extractor with more parameters");
    }
    Ok(MethodShape { binder, param, arg_type, result })
}

/// Whether selection `sel` selects from a new instance of a class (dotty's fallback of
/// `trySelectUnapply` for an extractor `C[T](...)`).
fn is_new_instance(t: &Typer<'_>, sel: TNodeId) -> bool {
    let T::Select { qualifier, .. } = *t.tpd.tree(sel) else { return false };
    let ctor = t.tpd.sym(t.tpd.meth_part(qualifier));
    ctor.is_some_and(|c| t.u.symbol(c).name == nme::CONSTRUCTOR)
}
