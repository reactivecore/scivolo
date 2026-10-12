//! Blocks and inlined trees made by the inliner, typed to avoid their local definitions (dotty's
//! `seq`, `Positioned.envelope`, `TypeAssigner.avoid`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::INLINE;
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

    /// Gives inlined tree `t` the type of its expansion, widened to avoid its bindings.
    pub fn retype_inlined(&mut self, t: TNodeId) {
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
            if widened == tpe {
                return None;
            }
            tpe = widened;
        }
        Some(tpe)
    }

    /// Whether type `t` refers to symbol `s`.
    pub fn mentions(&mut self, t: TypeId, s: SymbolId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(*u.types().get(part), Type::Sym(_, x) if x == s);
            None
        });
        found
    }

    /// Whether `owner` is an inline method.
    pub fn is_inline_method(&self, owner: SymbolId) -> bool {
        let s = self.u.symbol(owner);
        s.kind == Kind::Def && s.flags.has(INLINE)
    }
}
