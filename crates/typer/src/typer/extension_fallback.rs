// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The in-scope extension methods a selection cannot call, so that the implicit scope is searched
//! as dotty's `tryExtensionOrConversion` does when the in-scope extension method fails.

use super::Typer;
use super::implicits::conversions::{Goal, Viewed};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Whether no extension method named `name` that `ctx` sees takes `qual` as its receiver:
    /// its type conforms to none of their receiver types, their type parameters as variables, and
    /// no implicit view converts it to one.
    pub fn extensions_reject(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> bool {
        let lookup = super::extension_calls::extension_ctx(ctx);
        let Ok(Some(found)) = self.find_ref(name, false, lookup) else { return false };
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let qual_tpe = self.u.widen(qual_tpe);
        for s in found.syms {
            if !self.u.symbol(s).flags.has(tasty::tags::EXTENSION) {
                return false;
            }
            self.complete(s);
            let info = self.u.info(s);
            let (accepted, clean, _) = self.speculatively(|t| {
                let Some(receiver) = t.receiver_type(s, info) else { return true };
                t.conforms(qual_tpe, receiver)
                    || !matches!(t.viewed(qual, Goal::Conform(receiver), ctx), Ok(None))
            });
            if accepted || !clean {
                return false;
            }
        }
        true
    }

    /// The type of the first parameter after the leading implicit clauses of method type `info`,
    /// the method's type parameters replaced by fresh type variables; `None` for a dependent or
    /// by-name receiver.
    pub fn receiver_type(&mut self, method: tasty::SymbolId, info: TypeId) -> Option<TypeId> {
        let mut t = info;
        let tparams = self.u.type_params(method);
        let mut binders = Vec::new();
        let receiver = loop {
            match self.u.types().get(t).clone() {
                Type::Poly { binder, params, result } => {
                    let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                    let vars: Vec<TypeId> = names.iter().map(|_| self.constraint.fresh(self.u)).collect();
                    let inst = |t: &mut Self, x: TypeId| {
                        let x = t.u.subst(x, binder, &names, &vars);
                        t.u.map_type(x, &mut |u, part| match *u.types().get(part) {
                            Type::Sym(_, s) => {
                                tparams.iter().position(|&p| p == s).and_then(|i| vars.get(i).copied())
                            }
                            _ => None,
                        })
                    };
                    for (&(_, bounds), &v) in params.iter().zip(&vars) {
                        let bounds = inst(self, bounds);
                        if let (Some(lo), hi, 0) = self.bounds_of(bounds) {
                            let _ = self.conforms(lo, v) && self.conforms(v, hi);
                        }
                    }
                    t = inst(self, result);
                }
                Type::Method { binder, result, implicit: true, .. } => {
                    binders.push(binder);
                    t = result;
                }
                Type::Method { binder, params, .. } => {
                    let first = params.first()?.1;
                    binders.push(binder);
                    let dependent = binders.into_iter().any(|b| self.mentions_params(first, b, None));
                    break (!dependent).then_some(first)?;
                }
                _ => return None,
            }
        };
        (!matches!(self.u.types().get(receiver), Type::ByName(_))).then_some(receiver)
    }

    /// Whether `viewed` found an extension or conversion for selection of `name` from `qual`.
    pub fn implicit_member_found(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> bool {
        let (found, clean, _) = self.speculatively(|t| t.viewed(qual, Goal::Member(name), ctx));
        clean && matches!(found, Ok(Some(Viewed::Extension(_) | Viewed::Converted(_))))
    }
}
