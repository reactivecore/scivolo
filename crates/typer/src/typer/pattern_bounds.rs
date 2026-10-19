//! Wildcard arguments of a pattern's type as pattern-bound types (dotty's `typedType` with
//! `mapPatternBounds`, the type parameter bounds of `typedAppliedTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::CASE;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type argument `id` of a pattern's type tree, a variable pattern typed as a wildcard `_`;
    /// its name and span if it is one.
    pub fn typed_pattern_type_arg(&mut self, id: NodeId, ctx: Ctx) -> (TNodeId, Option<(TName, Span)>) {
        let var = match *self.ast.tree(id) {
            Tree::Ident { name } if ctx.mode.is(Mode::PATTERN_TYPE) => {
                Some(name).filter(|n| self.is_var_pattern(id, self.text(n.name)))
            }
            _ => None,
        };
        let Some(name) = var else { return (self.typed_type(id, ctx), None) };
        let span = self.ast.span(id);
        let wild = TName { name: self.name("_"), is_type: false };
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let at = Span::synthetic(span.start(), span.start());
        (self.tpd.node(T::Ident { name: wild }, at, Some(bounds), None), Some((name, span)))
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
        let class = match self.u.types().get(tycon).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, &name, true),
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
        let (Some(param), Type::Bounds(lo, hi)) = (param, self.u.types().get(bounds).clone()) else {
            return bounds;
        };
        self.complete(param);
        let info = self.u.info(param);
        let Type::Bounds(plo, phi) = *self.u.types().get(info) else { return bounds };
        let b = self.u.builtins();
        let (any, nothing) = (b.any, b.nothing);
        let independent = |t: &mut Self, x| !t.refers_to(x, params);
        let is = |t: &mut Self, x, c| t.u.class_symbol(x) == Some(c);
        let hi = match (is(self, hi, any), is(self, phi, any) || !independent(self, phi)) {
            (_, true) => hi,
            (true, false) => phi,
            _ => self.u.intern_type(Type::And(hi, phi)),
        };
        let lo = match (is(self, lo, nothing), is(self, plo, nothing) || !independent(self, plo)) {
            (_, true) => lo,
            (true, false) => plo,
            _ => self.u.intern_type(Type::Or(lo, plo, false)),
        };
        self.u.intern_type(Type::Bounds(lo, hi))
    }
}
