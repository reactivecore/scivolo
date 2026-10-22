// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! The function part of an application with its prefix and arguments lifted into local values
//! (dotty's `Lifter.liftApp`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Call `f` with its prefix lifted unless idempotent, or for a method with default
    /// arguments unless a literal, `this`, `new` or an idempotent reference, the arguments of a
    /// partial application lifted, and the statements of a block moved to `defs` (dotty's
    /// `liftApp`, `liftPrefix`); `Err` for another function tree.
    pub fn lift_app(
        &mut self,
        defs: &mut Vec<TNodeId>,
        f: TNodeId,
        complex: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(f);
        match *self.tpd.tree(f) {
            T::Apply { fun, args } => {
                let lifted = self.lift_app(defs, fun, complex, ctx)?;
                let tpe = self.tpd.tpe(fun);
                let info = self.method_info(tpe).unwrap_or(tpe);
                let Type::Method { params, .. } = self.u.types().get(info).clone() else {
                    return Err("lifting of a call of a function value");
                };
                let args = self.tpd.items(args).to_vec();
                let mut arg_defs = Vec::new();
                let lifted_args = self.lift_args(&mut arg_defs, &params, &args, complex, ctx);
                defs.extend(arg_defs.into_iter().map(|(_, d)| d));
                if lifted == fun && lifted_args == args {
                    return Ok(f);
                }
                let tpe = match lifted == fun {
                    true => self.tpd.tpe(f),
                    false => self.result_of(lifted).ok_or("lifting of a call of a function value")?,
                };
                Ok(self.build().apply(lifted, &lifted_args, tpe, span))
            }
            T::Block { stats, expr } => {
                defs.extend_from_slice(self.tpd.items(stats));
                self.lift_app(defs, expr, complex, ctx)
            }
            T::TypeApply { fun, args } => {
                let lifted = self.lift_app(defs, fun, complex, ctx)?;
                if lifted == fun {
                    return Ok(f);
                }
                let types: Vec<TypeId> = self.tpd.items(args).iter().map(|&a| self.tpd.tpe(a)).collect();
                let tpe = self.instantiated(lifted, &types)?;
                Ok(self.tpd.node(T::TypeApply { fun: lifted, args }, span, Some(tpe), None))
            }
            T::Select { qualifier, name } => {
                let Some(sym) = self.tpd.sym(f) else {
                    let tpe = self.tpd.tpe(f);
                    self.poly_apply_info(tpe).ok_or("lifting")?;
                    if self.is_idempotent_expr(qualifier) {
                        return Ok(f);
                    }
                    let lifted = self.lift(defs, qualifier, "", complex, ctx);
                    let site = self.tpd.tpe(lifted);
                    return self.poly_apply(lifted, site, span).ok_or("lifting");
                };
                let defaults = self.has_defaults(sym);
                let keep = match *self.tpd.tree(qualifier) {
                    T::Literal { .. } | T::This { .. } | T::New { .. } => true,
                    T::Ident { .. } | T::Select { .. } => self.is_idempotent_expr(qualifier),
                    _ => !defaults && self.is_idempotent_expr(qualifier),
                };
                let lifted = if keep { qualifier } else { self.lift(defs, qualifier, "", complex, ctx) };
                if lifted == qualifier {
                    return Ok(f);
                }
                let site = self.tpd.tpe(lifted);
                let tpe = self.ref_type(Some(site), sym);
                Ok(self.tpd.node(T::Select { qualifier: lifted, name }, span, Some(tpe), Some(sym)))
            }
            T::New { .. } => Ok(f),
            T::Ident { .. } => Ok(self.lift(defs, f, "", complex, ctx)),
            _ => Err("lifting of another function tree"),
        }
    }
}
