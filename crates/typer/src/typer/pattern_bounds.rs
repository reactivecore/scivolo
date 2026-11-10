// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Wildcard arguments of a pattern's type as pattern-bound types (dotty's `typedType` with
//! `mapPatternBounds`, the type parameter bounds of `typedAppliedTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::symbols::Completion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::CASE;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type argument `id`, the `i`th of `tycon`: a wildcard without bounds for a parameter of a
    /// higher or any kind a type tree of the parameter's bounds; in a pattern's type tree a
    /// variable pattern typed as a wildcard `_`, with its name and span.
    pub fn typed_pattern_type_arg(
        &mut self,
        id: NodeId,
        (tycon, i): (TypeId, usize),
        ctx: Ctx,
    ) -> (TNodeId, Option<(TName, Span)>) {
        if let Tree::TypeBoundsTree { lo: None, hi: None, alias: None } = *self.ast.tree(id) {
            match self.lambda_sub_param_bounds(tycon, i) {
                Ok(Some(bounds)) => {
                    let span = self.ast.span(id);
                    return (self.tpd.node(T::TypeTree { inferred: false }, span, Some(bounds), None), None);
                }
                Ok(None) => {}
                Err(what) => return (self.unsupported(id, what), None),
            }
        }
        let var = match *self.ast.tree(id) {
            Tree::Ident { name } if ctx.mode.is(Mode::PATTERN_TYPE) => {
                Some(name).filter(|n| self.is_var_pattern(id, self.text(n.name)))
            }
            _ => None,
        };
        let Some(name) = var else { return (self.typed_type(id, ctx), None) };
        let span = self.ast.span(id);
        let wild = TName { name: self.name("_"), is_type: false };
        let bounds = match self.lambda_sub_param_bounds(tycon, i) {
            Ok(Some(bounds)) => bounds,
            _ => self.u.intern_type(Type::Bounds(NOTHING, ANY)),
        };
        let at = Span::synthetic(span.start(), span.start());
        (self.tpd.node(T::Ident { name: wild }, at, Some(bounds), None), Some((name, span)))
    }

    /// The bounds of the `i`th type parameter of class `tycon` if they are of a higher or any kind
    /// and refer to no other parameter (dotty's `isLambdaSub`); an error while the parameter
    /// with bounds is being completed.
    fn lambda_sub_param_bounds(&mut self, tycon: TypeId, i: usize) -> Result<Option<TypeId>, &'static str> {
        let Some(c) = self.u.sym_of(tycon) else { return Ok(None) };
        if self.u.symbol(c).kind != Kind::Class {
            return Ok(None);
        }
        let params = self.u.type_params(c);
        let Some(&param) = params.get(i) else { return Ok(None) };
        let completion = self.syms.completions.get(&param).copied();
        if matches!(completion, Some(Completion::Pending { .. } | Completion::Completing(_)))
            && !self.class_param_tree(c, i).is_some_and(|p| self.may_be_lambda_sub(p))
        {
            return Ok(None);
        }
        match completion {
            Some(Completion::Pending { .. }) => self.complete(param),
            Some(Completion::Completing(_)) => {
                return Err("wildcard argument of a higher or any kind parameter being completed");
            }
            _ => {}
        }
        let info = self.u.info(param);
        let Some((_, hi)) = self.u.bounds(info) else { return Ok(None) };
        let any_kind = self.u.class_symbol(hi) == Some(self.u.defn().any_kind);
        let lambda = self.u.lambda(hi).is_some();
        Ok(((any_kind || lambda) && !self.refers_to(info, &params)).then_some(info))
    }

    /// Type arguments `args` of `tycon` in a pattern's type tree with each wildcard replaced by a
    /// bind of a pattern-bound type, named by `vars` or `_$n`, bounded also by its type
    /// parameter; their types.
    pub fn pattern_bound_args(
        &mut self,
        tycon: TypeId,
        args: &mut [TNodeId],
        vars: &[Option<(TName, Span)>],
        types: Vec<TypeId>,
        ctx: Ctx,
    ) -> Vec<TypeId> {
        if !ctx.mode.is(Mode::PATTERN_TYPE) {
            return types;
        }
        let class = match *self.u.ty(tycon) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, name, true),
            _ => None,
        };
        let params = match class {
            Some(c) if self.u.symbol(c).kind == Kind::Class => self.u.type_params(c),
            _ => Vec::new(),
        };
        for (i, a) in args.iter_mut().enumerate() {
            let var = vars.get(i).copied().flatten();
            if var.is_none() && !matches!(*self.tpd.tree(*a), T::TypeBoundsTree { alias: None, .. }) {
                continue;
            }
            let name = var.map_or_else(|| self.fresh.next(self.names, "_$"), |(v, _)| v.name);
            let text = self.text(name).to_owned();
            let sym = self.syms.define(self.u, &text, Kind::Type, ctx.owner, CASE);
            if var.is_some() {
                self.levels.enter(ctx.level, &text, sym);
            }
            let bounds = self.tpd.tpe(*a);
            let info = self.with_param_bounds(bounds, params.get(i).copied(), &params);
            self.u.set_info(sym, info);
            let tpe = self.u.intern_type(Type::Sym(None, sym));
            let span = self.tpd.span(*a);
            let name = TName { name, is_type: true };
            let body = match var {
                Some(_) => self.tpd.node(self.tpd.tree(*a).clone(), span, Some(info), None),
                None => *a,
            };
            let span = var.map_or_else(|| Span::synthetic(span.start(), span.end()), |(_, s)| s);
            *a = self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(sym));
        }
        args.iter().map(|&a| self.tpd.tpe(a)).collect()
    }

    /// Bounds `bounds` intersected with those of type parameter `param` that refer to none of
    /// `params`.
    pub fn with_param_bounds(
        &mut self,
        bounds: TypeId,
        param: Option<SymbolId>,
        params: &[SymbolId],
    ) -> TypeId {
        let (Some(param), Some((lo, hi))) = (param, self.u.bounds(bounds)) else {
            return bounds;
        };
        self.complete(param);
        let info = self.u.info(param);
        let Some((plo, phi)) = self.u.bounds(info) else { return bounds };
        let b = self.u.defn();
        let (any, nothing) = (b.any, b.nothing);
        let independent = |t: &mut Self, x| !t.refers_to(x, params);
        let is = |t: &mut Self, x, c| t.u.class_symbol(x) == Some(c);
        let hi = match (is(self, hi, any), is(self, phi, any) || !independent(self, phi)) {
            (_, true) => hi,
            _ if hi == phi => hi,
            (true, false) => phi,
            _ => self.u.intern_type(Type::And(hi, phi)),
        };
        let lo = match (is(self, lo, nothing), is(self, plo, nothing) || !independent(self, plo)) {
            (_, true) => lo,
            _ if lo == plo => lo,
            (true, false) => plo,
            _ => self.u.intern_type(Type::Or(lo, plo, false)),
        };
        self.u.intern_type(Type::Bounds(lo, hi))
    }

    /// The types bound in pattern `t`, appended to `out`, each with whether it is bound on an
    /// extractor's type arguments (`in_targs`), its bounds known.
    pub fn pattern_types(&self, t: TNodeId, out: &mut Vec<(SymbolId, bool)>, in_targs: bool) {
        if let T::Bind { .. } = self.tpd.tree(t)
            && let Some(s) = self.tpd.sym(t)
            && self.u.is_type_symbol(s)
        {
            out.push((s, in_targs));
        }
        let targs = matches!(self.tpd.tree(t), T::TypeApply { .. });
        for c in self.tpd.children(t) {
            self.pattern_types(c, out, targs);
        }
    }

    /// Whether `t` is `Any`, `Matchable` or `AnyRef`, or an abstract type bounded by one, against
    /// which a pattern's type constrains none of its bound types.
    pub fn is_top(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        let b = self.u.defn();
        let tops = [b.any, b.matchable, b.object];
        let Some((None, s)) = self.u.sym_ref(t) else {
            return self.u.sym_of(t).is_some_and(|s| tops.contains(&s));
        };
        if tops.contains(&s) {
            return true;
        }
        if !matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type) {
            return false;
        }
        let info = self.u.info(s);
        match self.u.bounds(info) {
            Some((_, hi)) => self.is_top(hi),
            None => false,
        }
    }
}
