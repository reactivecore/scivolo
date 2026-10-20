// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Positioned.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Blocks and inlined trees made by the inliner, typed to avoid their local definitions (dotty's
//! `seq`, `Positioned.envelope`, `TypeAssigner.avoid`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::{CONTRAVARIANT, COVARIANT, INLINE, INLINEPROXY};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// `{ stats; expr }` of the type of `expr` widened to avoid the definitions of `stats`
    /// (dotty's `seq`).
    pub fn seq(&mut self, stats: &[TNodeId], expr: TNodeId) -> Result<TNodeId, Unsupported> {
        if stats.is_empty() {
            return Ok(expr);
        }
        if let T::Block { stats: inner, expr: e } = *self.tpd.tree(expr)
            && !matches!(self.tpd.tree(e), T::Closure { .. })
        {
            let stats = [stats, self.tpd.items(inner)].concat();
            let span = self.tpd.span(expr);
            return self.block(&stats, e, span);
        }
        let span = self.envelope(stats, expr);
        self.block(stats, expr, span)
    }

    /// The span of a new block of `stats` and `expr`: the union of theirs (dotty's `envelope`).
    pub fn envelope(&self, stats: &[TNodeId], expr: TNodeId) -> Span {
        let parts = stats.iter().chain([&expr]);
        parts.fold(Span::NONE, |s, &x| s.union(self.tpd.span(x))).to_synthetic()
    }

    /// `{ stats; expr }` at `span`, typed as `seq` types it.
    pub fn block(&mut self, stats: &[TNodeId], expr: TNodeId, span: Span) -> Result<TNodeId, Unsupported> {
        let locals: Vec<SymbolId> = stats.iter().filter_map(|&s| self.tpd.sym(s)).collect();
        let tpe = self.tpd.tpe(expr);
        let tpe = self.avoid(tpe, &locals).ok_or((span, "inline match of a type referring to a binding"))?;
        let stats = self.tpd.list(stats);
        Ok(self.tpd.node(T::Block { stats, expr }, span, Some(tpe), None))
    }

    /// Gives inlined tree `t` the type of its expansion, widened to avoid its bindings, and a
    /// selection `t` from an inlined tree its member as seen from that tree's type.
    pub fn retype_inlined(&mut self, t: TNodeId) {
        if let T::Select { qualifier, .. } = *self.tpd.tree(t)
            && matches!(self.tpd.tree(qualifier), T::Inlined { .. })
            && let Some(tpe) = self.tpd.get(t).tpe
            && let Type::Sym(Some(_), m) = *self.u.types().get(tpe)
        {
            let prefix = self.tpd.tpe(qualifier);
            self.tpd.get_mut(t).tpe = Some(self.u.intern_type(Type::Sym(Some(prefix), m)));
            return;
        }
        let T::Inlined { bindings, expansion, .. } = *self.tpd.tree(t) else { return };
        let locals: Vec<SymbolId> =
            self.tpd.items(bindings).iter().filter_map(|&b| self.tpd.sym(b)).collect();
        let tpe = self.tpd.tpe(expansion);
        self.tpd.get_mut(t).tpe = Some(self.avoid(tpe, &locals).unwrap_or(tpe));
    }

    /// `tpe` widened until it refers to none of `locals`; `None` if it cannot be.
    pub fn avoid(&mut self, mut tpe: TypeId, locals: &[SymbolId]) -> Option<TypeId> {
        while locals.iter().any(|&s| self.mentions(tpe, s)) {
            let widened = self.u.widen_once(tpe).unwrap_or(tpe);
            let widened = match *self.u.types().get(widened) {
                Type::ByName(r) => r,
                _ => widened,
            };
            let widened = self.u.dealias(widened);
            let widened = crate::typer::simplify::normalize_all(self.u, widened);
            let widened = if widened == tpe { self.avoid_parts(tpe, locals) } else { widened };
            if widened == tpe {
                return None;
            }
            tpe = widened;
        }
        Some(tpe)
    }

    /// `tpe` with its parts that refer to `locals` replaced: a type selected from a path through
    /// one by its alias, a local value by its type; `tpe` itself if a local value is misplaced.
    fn avoid_parts(&mut self, tpe: TypeId, locals: &[SymbolId]) -> TypeId {
        if self.misplaced_local(tpe, locals, 1, 16) {
            return tpe;
        }
        self.u.map_type(tpe, &mut |u, part| {
            let Type::Sym(p, x) = *u.types().get(part) else { return None };
            if locals.contains(&x) && !u.is_type_symbol(x) {
                return Some(u.info(x)).filter(|&i| i != part);
            }
            let p = p?;
            if !u.is_type_symbol(x) || !mentions_any(u, p, locals) {
                return None;
            }
            Some(u.dealias(part)).filter(|&d| d != part)
        })
    }

    /// Whether a local value of `locals` occurs in `t`, at a position of variance `variance`
    /// (1 covariant, 0 invariant, -1 contravariant), where its type cannot stand for it: other
    /// than covariantly, or contravariantly for an inline proxy; a type selection with an alias
    /// counts by its alias.
    fn misplaced_local(&mut self, t: TypeId, locals: &[SymbolId], variance: i8, depth: u32) -> bool {
        if depth == 0 {
            return mentions_any(self.u, t, locals);
        }
        match self.u.types().get(t).clone() {
            Type::Sym(p, x) if !self.u.is_type_symbol(x) => {
                if locals.contains(&x) {
                    let proxy = self.u.symbol(x).flags.has(INLINEPROXY);
                    return if proxy { variance < 0 } else { variance < 1 };
                }
                p.is_some_and(|p| self.misplaced_local(p, locals, 0, depth - 1))
            }
            Type::Sym(p, _) => {
                let d = self.u.dealias(t);
                if d != t {
                    return self.misplaced_local(d, locals, variance, depth - 1);
                }
                p.is_some_and(|p| self.misplaced_local(p, locals, 0, depth - 1))
            }
            Type::Applied(tycon, args) => {
                let class = self.u.class_symbol(tycon).filter(|&c| self.u.is_class(c));
                let params = class.map(|c| self.u.type_params(c)).unwrap_or_default();
                let mut found = self.misplaced_local(tycon, locals, 0, depth - 1);
                for (i, a) in args.into_iter().enumerate() {
                    let flags = params.get(i).map(|&p| self.u.symbol(p).flags);
                    let param = match flags {
                        Some(f) if f.has(COVARIANT) => 1,
                        Some(f) if f.has(CONTRAVARIANT) => -1,
                        _ => 0,
                    };
                    found = found || self.misplaced_local(a, locals, variance * param, depth - 1);
                }
                found
            }
            _ => mentions_any(self.u, t, locals),
        }
    }

    /// Whether type `t` refers to symbol `s`.
    pub fn mentions(&mut self, t: TypeId, s: SymbolId) -> bool {
        mentions_any(self.u, t, &[s])
    }
}

/// Whether type `t` refers to one of `syms`.
fn mentions_any(u: &mut tasty::Universe, t: TypeId, syms: &[SymbolId]) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= matches!(*u.types().get(part), Type::Sym(_, x) if syms.contains(&x));
        None
    });
    found
}

impl Inlining<'_> {
    /// Whether `owner` is an inline method.
    pub fn is_inline_method(&self, owner: SymbolId) -> bool {
        let s = self.u.symbol(owner);
        s.kind == Kind::Def && s.flags.has(INLINE)
    }
}
