//! Calls of transparent inline methods of the unit, expanded while typing (dotty's
//! `Inlines.needsInlining` and `inlineCall` in `adaptNoArgsOther`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::inlining::{Unsupported, inline_transparent};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::NodeId;
use tasty::tags::{INLINE, MACRO, TRANSPARENT};
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// The definition of an inline method of the unit.
#[derive(Clone, Copy)]
pub enum InlineDef {
    /// Still to be typed, in the context it was entered in.
    Pending(NodeId, Ctx),
    Typing,
    Typed(TNodeId),
}

impl Typer<'_> {
    /// Whether `sym` is a transparent inline method, not a macro.
    pub fn is_transparent_inline(&self, sym: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        s.kind == Kind::Def && s.flags.has(INLINE) && s.flags.has(TRANSPARENT) && !s.flags.has(MACRO)
    }

    /// `tree` expanded if it is a call of a transparent inline method outside an inline method,
    /// `None` if it is not one.
    pub fn inlined_transparent(&mut self, tree: TNodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        if let T::Block { stats, expr } = *self.tpd.tree(tree) {
            let inlined = self.inlined_transparent(expr, pt, ctx)?;
            let stats = self.tpd.items(stats).to_vec();
            return Some(self.wrap_defs(stats, inlined));
        }
        let meth = self.tpd.sym(self.tpd.meth_part(tree))?;
        let widened = self.u.widen(self.tpd.get(tree).tpe?);
        if !self.is_transparent_inline(meth)
            || matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. })
            || ctx.mode.is(Mode::PATTERN)
            || self.enclosing_inline(ctx.owner).is_some()
        {
            return None;
        }
        match self.expand_transparent(tree, meth, pt, ctx) {
            Ok(inlined) => Some(inlined),
            Err((span, what)) => {
                self.unsupported_at(span, what);
                None
            }
        }
    }

    fn expand_transparent(
        &mut self,
        call: TNodeId,
        meth: SymbolId,
        pt: Pt,
        ctx: Ctx,
    ) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(call);
        if matches!(self.u.symbol(meth).name.as_str(), "unapply" | "unapplySeq") {
            return Err((span, "transparent inline extractor"));
        }
        if let Pt::Type(p) = pt {
            let (tpe, approx) = (self.tpd.tpe(call), self.wild_approx(p));
            let mark = self.constraint.mark();
            if !self.conforms(tpe, approx) {
                self.constraint.rollback(mark);
            }
        }
        self.fully_define_tree(call).map_err(|w| (span, w))?;
        let mut bodies = FxHashMap::default();
        self.collect_transparent_bodies(meth, &mut bodies).map_err(|w| (span, w))?;
        inline_transparent(
            (&mut *self.u, &mut self.tpd),
            (&mut *self.names, &mut *self.fresh),
            bodies,
            call,
            ctx.owner,
        )
    }

    /// Adds to `bodies` the definitions of `meth` and of the transparent inline methods its body
    /// calls, typed ahead if they are not typed yet (dotty's `bodyToInline`).
    fn collect_transparent_bodies(
        &mut self,
        meth: SymbolId,
        bodies: &mut FxHashMap<SymbolId, TNodeId>,
    ) -> Result<(), &'static str> {
        if bodies.contains_key(&meth) {
            return Ok(());
        }
        let def = match self.inline_defs.get(&meth).copied() {
            Some(InlineDef::Typed(def)) => def,
            Some(InlineDef::Pending(tree, ctx)) => {
                self.inline_defs.insert(meth, InlineDef::Typing);
                let def = self.typed_def_def(tree, ctx);
                self.typed_ahead(tree, def, ctx);
                def
            }
            Some(InlineDef::Typing) => return Err("transparent inline method called while typed"),
            None => return Err("transparent inline method call"),
        };
        self.fully_define_tree(def)?;
        bodies.insert(meth, def);
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else {
            return Err("transparent inline method without a body");
        };
        for m in self.transparent_refs(rhs) {
            self.collect_transparent_bodies(m, bodies)?;
        }
        Ok(())
    }

    /// The transparent inline methods `t` refers to.
    fn transparent_refs(&self, t: TNodeId) -> Vec<SymbolId> {
        let own = self.tpd.sym(t).filter(|&s| {
            matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. }) && self.is_transparent_inline(s)
        });
        let inner = self.tpd.children(t).into_iter().flat_map(|c| self.transparent_refs(c));
        own.into_iter().chain(inner).collect()
    }

    /// Makes the types in `t` fully defined (dotty's `isFullyDefined` on the call's type
    /// arguments, with `ForceDegree.flipBottom`).
    fn fully_define_tree(&mut self, t: TNodeId) -> Result<(), &'static str> {
        if let Some(tpe) = self.tpd.get(t).tpe {
            let tpe = self.fully_defined(tpe)?;
            self.tpd.get_mut(t).tpe = Some(tpe);
        }
        if let Some(s) = self.tpd.sym(t)
            && self.syms.closure_syms.contains(&s)
        {
            let info = self.u.info(s);
            let info = self.fully_defined(info)?;
            self.u.set_info(s, info);
        }
        for c in self.tpd.children(t) {
            self.fully_define_tree(c)?;
        }
        Ok(())
    }

    /// `t` with its uninstantiated type variables replaced by wildcards of their bounds (dotty's
    /// `wildApprox`).
    pub fn wild_approx(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        let constraint = &self.constraint;
        self.u.map_type(t, &mut |u, part| {
            let Type::Var(v) = *u.types().get(part) else { return None };
            let (lo, hi) = constraint.bounds(v);
            let mut open = false;
            for b in [lo, hi] {
                u.map_type(b, &mut |u, x| {
                    open |= matches!(u.types().get(x), Type::Var(_));
                    None
                });
            }
            let bounds = (!open).then(|| u.intern_type(Type::Bounds(lo, hi)));
            Some(u.intern_type(Type::Wildcard(bounds)))
        })
    }
}
