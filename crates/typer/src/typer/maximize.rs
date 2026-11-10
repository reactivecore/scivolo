// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! A pattern's type maximized (dotty's `maximizeType`), its pattern-bound types bound on the
//! extractor's type arguments (`addBinders`).

use super::Typer;
use super::gadt::gadt_ref;
use super::narrowing::GadtVar;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use syntax::ast::flags::CASE;
use tasty::tags::FINAL;
use tasty::{Kind, NameId, SymbolId, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Instantiates the type variables from id `vars` on that occur in `t`: a covariant one from
    /// above, a contravariant one from below, unless it occurs in the bounds of GADT variables
    /// `gadt`; else from above if its bounds coincide or its upper bound is a final class, else
    /// to a fresh pattern-bound type of its bounds; those types, named after the type parameters
    /// of extractor `unapp`.
    pub fn maximize_type(
        &mut self,
        t: TypeId,
        (vars, gadt): (usize, &[GadtVar]),
        unapp: TNodeId,
        ctx: Ctx,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let t = self.zonk(t);
        let variances = self.types().variances(t, None, vars);
        let names = self.type_var_names(unapp);
        let gadt_bounds = self.types().gadt_var_bounds(gadt);
        let mut bound = Vec::new();
        for i in vars..self.constraint.count() {
            let v = VarId(i as u32);
            let Some(&variance) = variances.get(&v.0) else { continue };
            if self.constraint.instance(v).is_some() {
                continue;
            }
            let var = self.u.intern_type(Type::Var(v));
            if gadt.iter().any(|&(_, g)| g == var) {
                continue;
            }
            if variance != 0 && !gadt_bounds.iter().any(|&b| self.types().refers_to_type(b, var)) {
                self.instantiate_var(v, variance == -1)?;
                continue;
            }
            let (lo, hi) = self.constraint.bounds(v);
            let (lo, hi) = (self.zonk(lo), self.zonk(hi));
            let lo = self.with_gadt_params_below(lo, var, gadt);
            let final_hi = self.u.class_symbol(hi).is_some_and(|c| self.u.symbol(c).flags.has(FINAL));
            let fixed = lo == hi || final_hi || self.u.is_subtype_frozen_in(hi, lo, &mut self.constraint);
            if fixed {
                self.instantiate_var(v, false)?;
                continue;
            }
            let param = names.iter().find(|(w, _)| *w == v).map_or("T", |&(_, n)| self.u.name(n));
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

    /// Lower bound `lo` of type variable `var` joined with each GADT variable of `gadt` whose upper
    /// bound is `var` or an intersection with it (dotty's `fullLowerBound`).
    fn with_gadt_params_below(&mut self, mut lo: TypeId, var: TypeId, gadt: &[GadtVar]) -> TypeId {
        for &(g, gv) in gadt {
            let Some(id) = self.u.var(gv) else { continue };
            let (_, hi) = self.constraint.bounds(id);
            if !and_parts(self.u, hi).contains(&var) {
                continue;
            }
            let below = gadt_ref(self.u, g);
            lo = self.u.lub(lo, below);
        }
        lo
    }

    /// The type variables that are type arguments of `unapp`, with their parameters' names.
    fn type_var_names(&mut self, unapp: TNodeId) -> Vec<(VarId, NameId)> {
        let T::TypeApply { fun, args } = *self.tpd.tree(unapp) else { return Vec::new() };
        let fun_tpe = self.tpd.tpe(fun);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Some(poly) = self.u.poly(info) else { return Vec::new() };
        let args = self.tpd.items(args).to_vec();
        args.iter()
            .zip(poly.params)
            .filter_map(|(&a, &(name, _))| self.u.var(self.tpd.tpe(a)).map(|v| (v, name)))
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
            let Some((None, s)) = self.u.sym_ref(tpe) else { continue };
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

/// The parts of intersection `t`, or `t`.
fn and_parts(u: &mut tasty::Universe, t: TypeId) -> Vec<TypeId> {
    match *u.ty(t) {
        Type::And(a, b) => [and_parts(u, a), and_parts(u, b)].concat(),
        _ => vec![t],
    }
}
