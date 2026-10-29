// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! A pattern's type maximized (dotty's `maximizeType`), its pattern-bound types bound on the
//! extractor's type arguments (`addBinders`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use syntax::ast::flags::CASE;
use tasty::tags::FINAL;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Instantiates the type variables from id `vars` on that occur in `t`: a covariant one from
    /// above, a contravariant one from below, an invariant one from above if its bounds coincide
    /// or its upper bound is a final class, else to a fresh pattern-bound type of its bounds; those
    /// types, named after the type parameters of extractor `unapp`.
    pub fn maximize_type(
        &mut self,
        t: TypeId,
        vars: usize,
        unapp: TNodeId,
        ctx: Ctx,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let t = self.zonk(t);
        let variances = self.variances(t, None, vars);
        let names = self.type_var_names(unapp);
        let mut bound = Vec::new();
        for i in vars..self.constraint.count() {
            let v = VarId(i as u32);
            let Some(&variance) = variances.get(&v.0) else { continue };
            if self.constraint.instance(v).is_some() {
                continue;
            }
            if variance != 0 {
                self.instantiate_var(v, variance == -1)?;
                continue;
            }
            let (lo, hi) = self.constraint.bounds(v);
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            let final_hi = self.u.class_symbol(hi).is_some_and(|c| self.u.symbol(c).flags.has(FINAL));
            let attempt = self.constraint.attempt();
            let fixed = lo == hi || final_hi || self.conforms(hi, lo);
            self.constraint.abandon(attempt);
            if fixed {
                self.instantiate_var(v, false)?;
                continue;
            }
            let param = names.iter().find(|(w, _)| *w == v).map_or("T", |(_, n)| n.as_str());
            let name = self.fresh.next(self.names, &format!("{param}$"));
            let text = self.text(name).to_owned();
            let sym = self.syms.define(self.u, &text, Kind::Type, ctx.owner, CASE);
            let tpe = self.u.intern_type(Type::Sym(None, sym));
            self.constraint.instantiate(v, tpe);
            bound.push((sym, lo, hi));
        }
        Ok(bound
            .into_iter()
            .map(|(sym, lo, hi)| {
                let info = self.u.intern_type(Type::Bounds(lo, hi));
                let info = self.zonk(info);
                self.u.set_info(sym, info);
                sym
            })
            .collect())
    }

    /// The type variables that are type arguments of `unapp`, with their parameters' names.
    fn type_var_names(&mut self, unapp: TNodeId) -> Vec<(VarId, String)> {
        let T::TypeApply { fun, args } = *self.tpd.tree(unapp) else { return Vec::new() };
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Some(poly) = self.u.poly(info) else { return Vec::new() };
        let args = self.tpd.items(args).to_vec();
        args.iter()
            .zip(poly.params)
            .filter_map(|(&a, (name, _))| match *self.u.types().get(self.tpd.tpe(a)) {
                Type::Var(v) => Some((v, name.clone())),
                _ => None,
            })
            .collect()
    }

    /// `unapp`'s type arguments that are pattern-bound types `bound`, each first one bound.
    pub fn add_binders(&mut self, unapp: TNodeId, bound: &[SymbolId]) {
        let T::TypeApply { fun, args } = *self.tpd.tree(unapp) else { return };
        let mut remain = bound.to_vec();
        let mut args = self.tpd.items(args).to_vec();
        for a in &mut args {
            let tpe = self.tpd.tpe(*a);
            let tpe = self.zonk(tpe);
            let Type::Sym(None, s) = *self.u.types().get(tpe) else { continue };
            let Some(i) = remain.iter().position(|&r| r == s) else { continue };
            remain.swap_remove(i);
            let text = self.u.symbol(s).name.clone();
            let name = TName { name: self.name(&text), is_type: true };
            let span = self.tpd.span(*a).end_pos();
            let body = self.tpd.node(T::Ident { name }, span, Some(tpe), Some(s));
            *a = self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(s));
        }
        let args = self.tpd.list(&args);
        self.tpd.get_mut(unapp).tree = T::TypeApply { fun, args };
    }
}
