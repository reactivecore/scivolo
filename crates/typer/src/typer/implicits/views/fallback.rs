// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The in-scope extension methods a selection cannot call, so that the implicit scope is searched
//! as dotty's `tryExtensionOrConversion` does when the in-scope extension method fails.

use super::super::super::Typer;
use super::super::super::extension_calls::extension_ctx;
use super::{Goal, Viewed};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Tells whether no extension method named `name` that `ctx` sees takes `qual` as its
    /// receiver, as `extension (s: String) def twice` for `1.twice`: its type conforms to none of
    /// their receiver types, their type parameters as variables, and no implicit view converts it
    /// to one.
    pub fn extensions_reject(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> bool {
        let Ok(Some(found)) = self.find_ref(name, false, extension_ctx(ctx)) else { return false };
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let qual_tpe = self.u.widen(qual_tpe);
        for s in found.syms {
            if !self.u.symbol(s).is_extension() {
                return false;
            }
            self.complete(s);
            let info = self.u.info(s);
            let (accepted, clean, _) = self.speculatively(|t| {
                let Some(receiver) = receiver_type(t, s, info) else { return true };
                t.conforms(qual_tpe, receiver)
                    || !matches!(t.viewed(qual, Goal::Conform(receiver), ctx), Ok(None))
            });
            if accepted || !clean {
                return false;
            }
        }
        true
    }

    /// Tells whether a view search finds an extension or a conversion for the selection of `name`
    /// from `qual`, tried without changing the constraint.
    pub fn implicit_member_found(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> bool {
        let (found, clean, _) = self.speculatively(|t| t.viewed(qual, Goal::Member(name), ctx));
        clean && matches!(found, Ok(Some(Viewed::Extension(_) | Viewed::Converted(_))))
    }
}

/// The type of the first parameter after the leading implicit clauses of method type `info`,
/// the type parameters of `method` replaced by fresh type variables; `None` for a dependent or
/// by-name receiver.
pub fn receiver_type(t: &mut Typer<'_>, method: SymbolId, info: TypeId) -> Option<TypeId> {
    let mut tpe = info;
    let tparams = t.u.type_params(method);
    let mut binders = Vec::new();
    let receiver = loop {
        match *t.u.ty(tpe) {
            Type::Poly { binder, params, result } => {
                let params = t.u.params_of(params).to_vec();
                let names: Vec<tasty::NameId> = params.iter().map(|&(n, _)| n).collect();
                let vars: Vec<TypeId> = names.iter().map(|_| t.constraint.fresh(t.u)).collect();
                let inst = |t: &mut Typer<'_>, x: TypeId| {
                    let x = t.u.subst(x, binder, &names, &vars);
                    t.u.map_type(x, &mut |u, part| {
                        let s = u.sym_of(part)?;
                        tparams.iter().position(|&p| p == s).and_then(|i| vars.get(i).copied())
                    })
                };
                for (&(_, bounds), &v) in params.iter().zip(&vars) {
                    let bounds = inst(t, bounds);
                    if let (Some(lo), hi, 0) = t.bounds_of(bounds) {
                        t.constraint.declare(t.u, v, lo, hi);
                        let _ = t.conforms(lo, v) && t.conforms(v, hi);
                    }
                }
                tpe = inst(t, result);
            }
            Type::Method { binder, result, implicit: true, .. } => {
                binders.push(binder);
                tpe = result;
            }
            Type::Method { binder, params, .. } => {
                let first = t.u.params_of(params).first()?.1;
                binders.push(binder);
                let dependent = binders.into_iter().any(|b| t.mentions_params(first, b, None));
                break (!dependent).then_some(first)?;
            }
            _ => return None,
        }
    };
    t.u.by_name(receiver).is_none().then_some(receiver)
}
