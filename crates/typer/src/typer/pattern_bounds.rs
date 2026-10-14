//! Wildcard arguments of a pattern's type as pattern-bound types (dotty's `typedType` with
//! `mapPatternBounds`, the type parameter bounds of `typedAppliedTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::CASE;
use syntax::ast::{Span, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type arguments `args` of `tycon` in a pattern's type tree with each wildcard replaced by a
    /// bind of a pattern-bound type `_$n`, bounded also by its type parameter; their types.
    pub fn pattern_bound_args(
        &mut self,
        tycon: TypeId,
        args: &mut [TNodeId],
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
            let T::TypeBoundsTree { alias: None, .. } = *self.tpd.tree(*a) else { continue };
            let name = self.fresh.next(self.names, "_$");
            let text = self.text(name).to_owned();
            let sym = self.syms.define(self.u, &text, Kind::Type, ctx.owner, CASE);
            let bounds = self.tpd.tpe(*a);
            let info = self.with_param_bounds(bounds, params.get(i).copied(), &params);
            self.u.set_info(sym, info);
            let tpe = self.u.intern_type(Type::Sym(None, sym));
            let span = self.tpd.span(*a);
            let name = TName { name, is_type: true };
            let span = Span::synthetic(span.start(), span.end());
            *a = self.tpd.node(T::Bind { name, body: *a }, span, Some(tpe), Some(sym));
        }
        args.iter().map(|&a| self.tpd.tpe(a)).collect()
    }

    /// Bounds `bounds` intersected with those of type parameter `param` that refer to none of
    /// `params`.
    fn with_param_bounds(&mut self, bounds: TypeId, param: Option<SymbolId>, params: &[SymbolId]) -> TypeId {
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
