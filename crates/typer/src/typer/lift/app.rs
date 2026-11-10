// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! The function part of an application with its prefix and arguments lifted into local values
//! (dotty's `Lifter.liftApp`).

use super::super::Typer;
use super::Lifter;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Lifter {
    /// Gives call `f` with its prefix lifted unless idempotent, or for a method with default
    /// arguments unless a literal, `this`, `new` or an idempotent reference, the arguments of a
    /// partial application lifted, and the statements of a block moved to the definitions
    /// (dotty's `liftApp`, `liftPrefix`); `Err` for another function tree.
    pub fn app(&mut self, t: &mut Typer, f: TNodeId) -> Result<TNodeId, &'static str> {
        let span = t.tpd.span(f);
        match *t.tpd.tree(f) {
            T::Apply { fun, args } => {
                let lifted = self.app(t, fun)?;
                let tpe = t.tpd.tpe(fun);
                let info = t.method_info(tpe).unwrap_or(tpe);
                let Type::Method { params, binder, result, .. } = t.u.types().get(info).clone() else {
                    return Err("lifting of a call of a function value");
                };
                let args = t.tpd.items(args).to_vec();
                let lifted_args: Vec<TNodeId> =
                    args.iter().zip(&params).map(|(&a, p)| self.arg(t, a, p)).collect();
                if lifted == fun && lifted_args == args {
                    return Ok(f);
                }
                let method = t.tpd.sym(fun);
                let tpe = match lifted == fun {
                    true if t.mentions_params(result, binder, method) => {
                        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                        let types: Vec<TypeId> = lifted_args.iter().map(|&a| t.tpd.tpe(a)).collect();
                        t.applied_result(result, (binder, &names), method, &types)
                    }
                    true => t.tpd.tpe(f),
                    false => result_of(t, lifted).ok_or("lifting of a call of a function value")?,
                };
                Ok(t.build().apply(lifted, &lifted_args, tpe, span))
            }
            T::Block { stats, expr } => {
                self.defs.extend_from_slice(t.tpd.items(stats));
                self.app(t, expr)
            }
            T::TypeApply { fun, args } => {
                let lifted = self.app(t, fun)?;
                if lifted == fun {
                    return Ok(f);
                }
                let types: Vec<TypeId> = t.tpd.items(args).iter().map(|&a| t.tpd.tpe(a)).collect();
                let tpe = t.instantiated(lifted, &types)?;
                Ok(t.tpd.node(T::TypeApply { fun: lifted, args }, span, Some(tpe), None))
            }
            T::Select { qualifier, name } => {
                let Some(sym) = t.tpd.sym(f) else {
                    let tpe = t.tpd.tpe(f);
                    if t.structural_selects.contains(&f) && !t.is_idempotent_expr(qualifier) {
                        t.structural_selects.remove(&f);
                        let lifted = self.lift(t, qualifier, "");
                        return Ok(t.structural_method(lifted, name, tpe, span));
                    }
                    if t.structural_selects.contains(&f) {
                        return Ok(f);
                    }
                    t.poly_apply_info(tpe).ok_or("lifting")?;
                    if t.is_idempotent_expr(qualifier) {
                        return Ok(f);
                    }
                    let lifted = self.lift(t, qualifier, "");
                    let site = t.tpd.tpe(lifted);
                    return t.poly_apply(lifted, site, span).ok_or("lifting");
                };
                let defaults = t.has_defaults(sym);
                let keep = match *t.tpd.tree(qualifier) {
                    T::Literal { .. } | T::This { .. } | T::New { .. } => true,
                    T::Ident { .. } | T::Select { .. } => t.is_idempotent_expr(qualifier),
                    _ => !defaults && t.is_idempotent_expr(qualifier),
                };
                let lifted = if keep { qualifier } else { self.lift(t, qualifier, "") };
                if lifted == qualifier {
                    return Ok(f);
                }
                let site = t.tpd.tpe(lifted);
                let tpe = t.ref_type(Some(site), sym);
                Ok(t.tpd.node(T::Select { qualifier: lifted, name }, span, Some(tpe), Some(sym)))
            }
            T::New { .. } => Ok(f),
            T::Ident { .. } => Ok(self.lift(t, f, "")),
            _ => Err("lifting of another function tree"),
        }
    }
}

/// The result type of method reference `f`.
fn result_of(t: &mut Typer, f: TNodeId) -> Option<TypeId> {
    let tpe = t.tpd.tpe(f);
    let info = match t.u.types().get(tpe) {
        Type::Method { .. } => tpe,
        _ => t.method_info(tpe)?,
    };
    match *t.u.types().get(info) {
        Type::Method { result, .. } => Some(result),
        _ => None,
    }
}
