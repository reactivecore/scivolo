// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The arguments of an overloaded application typed before an alternative is chosen, functions
//! without parameter types against the parameter types all alternatives agree on (dotty's
//! `pretypeArgs`, `FunProto.typedArgs`).

use super::poly_instances::typed_unmapped;
use super::shapes::{formal_at, unblocked_all, unknown_param_function};
use super::{MODIFIED_FUNCTION, Resolution};
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId};

impl Resolution {
    /// Gives the types of the arguments of `proto`, typed once for alternatives `alts`; a
    /// function argument without parameter types that no common parameter type types, as
    /// `x => x` against `Int => Int` and `String => String`, as a function of unknown types.
    pub fn proto_arg_types(
        self,
        t: &mut Typer,
        proto: usize,
        alts: &[SymbolId],
    ) -> Result<Vec<TypeId>, &'static str> {
        let args = self.pretyped_args(t, proto, alts)?;
        let mut arg_types = Vec::new();
        for (&a, &id) in args.iter().zip(&t.protos[proto].args.clone()) {
            arg_types.push(match a {
                Some(a) => {
                    let tpe = t.tpd.tpe(a);
                    t.zonk(tpe)
                }
                None => untyped_arg_type(t, id)?,
            });
        }
        Ok(arg_types)
    }

    /// Types the arguments of `proto` once: a function without parameter types against the
    /// function type of the parameter types all of `alts` agree on at its position, else left
    /// for the chosen alternative (`None`); the others without an expected type.
    pub fn pretyped_args(
        self,
        t: &mut Typer,
        proto: usize,
        alts: &[SymbolId],
    ) -> Result<Vec<Option<TNodeId>>, &'static str> {
        if let Some(typed) = &t.protos[proto].typed {
            return Ok(typed.clone());
        }
        let (args, ctx) = (t.protos[proto].args.clone(), t.protos[proto].ctx);
        let mut typed = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let inner = match *t.ast.tree(a) {
                Tree::NamedArg { arg, .. } => Some(arg),
                _ => None,
            };
            typed.push(match (unknown_param_function(t, a), inner) {
                (Some(_), Some(_)) => None,
                (Some(f), None) => {
                    self.common_formal(t, f, i, alts)?.map(|p| typed_unmapped(t, a, Pt::Type(p), ctx))
                }
                (None, None) if t.is_sequence_arg(a) => Some(t.sequence_arg(a, None, ctx)),
                (None, inner) => {
                    let arg = inner.unwrap_or_else(|| unblocked_all(t.ast, a));
                    Some(typed_unmapped(t, arg, Pt::Any, ctx))
                }
            });
        }
        t.protos[proto].typed = Some(typed.clone());
        Ok(typed)
    }

    /// Finds for function `f` without parameter types at position `i` the function or partial
    /// function type of the parameter types all of `alts` agree on there.
    fn common_formal(
        self,
        t: &mut Typer,
        f: NodeId,
        i: usize,
        alts: &[SymbolId],
    ) -> Result<Option<TypeId>, &'static str> {
        if matches!(t.ast.tree(f), Tree::Function { mods: Some(_), .. }) {
            return Err(MODIFIED_FUNCTION);
        }

        let mut common: Option<Vec<TypeId>> = None;
        let mut partial = false;
        for &alt in alts {
            let Some(formal) = formal_at(t, alt, i, self.prefix) else { return Ok(None) };
            let pf = partial_function_arg(t, formal);
            partial |= pf.is_some();
            let ps = pf.map_or_else(|| t.function_params(formal), |a| vec![a]);
            match &common {
                _ if ps.is_empty() => return Ok(None),
                None => common = Some(ps),
                Some(c) => {
                    let same = c.len() == ps.len()
                        && c.iter().zip(&ps).all(|(&a, &b)| t.u.is_subtype(a, b) && t.u.is_subtype(b, a));
                    if !same {
                        return Ok(None);
                    }
                }
            }
        }

        let Some(common) = common else { return Ok(None) };
        let mut params = Vec::new();
        for p in common {
            params.push(t.fully_defined(p)?);
        }
        let unknown = t.u.intern_type(Type::Wildcard(None));
        if partial && matches!(t.ast.tree(f), Tree::Match { .. }) {
            return t.partial_function_of(params[0], unknown).map(Some);
        }
        params.push(unknown);
        t.function_of(params).map(Some)
    }
}

/// Gives the type of argument `arg` of no type yet while alternatives are compared: a function
/// of as many parameters of unknown types, of an unknown result; a case lambda of an unknown type
/// (dotty's `typedArgs` without forcing).
fn untyped_arg_type(t: &mut Typer, arg: NodeId) -> Result<TypeId, &'static str> {
    let f = unknown_param_function(t, arg).ok_or("argument of an overload")?;
    match *t.ast.tree(f) {
        Tree::Function { args, .. } => {
            let params = t.ast.items(args).to_vec();
            if params.iter().any(|&p| !t.is_untyped_param(p)) {
                return Err("function argument of an overload with some parameter types");
            }
            let mut parts = vec![ANY; params.len()];
            parts.push(NOTHING);
            t.function_of(parts)
        }
        _ => Ok(t.u.intern_type(Type::Wildcard(None))),
    }
}

/// Finds the argument type `A` of `tpe` if it is a `PartialFunction[A, B]`.
fn partial_function_arg(t: &mut Typer, tpe: TypeId) -> Option<TypeId> {
    if !t.is_partial_function(tpe) {
        return None;
    }
    let tpe = t.zonk(tpe);
    let tpe = t.u.dealias(tpe);
    let (_, args) = t.u.applied(tpe)?;
    (args.len() == 2).then(|| args[0])
}
