//! Calls of transparent inline methods of the unit, expanded while typing (dotty's
//! `Inlines.needsInlining` and `inlineCall` in `adaptNoArgsOther`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::inlining::Unsupported;
use crate::tpd::{MatchKind, TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::NodeId;
use tasty::tags::{INLINE, MACRO, TRANSPARENT};
use tasty::{Kind, SymbolId, Type, TypeVars};

/// The definition of an inline method of the unit.
#[derive(Clone, Copy)]
pub enum InlineDef {
    /// Still to be typed, in the context it was entered in.
    Pending(NodeId, Ctx),
    /// Being typed ahead of a call.
    Typing,
    Typed(TNodeId),
    /// Typed, with the type variables in it instantiated.
    Defined(TNodeId),
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
            if !self.is_fully_defined(approx) && self.has_compiletime_op(tpe) {
                return Err((span, "compile-time operation type"));
            }
            let mark = self.constraint.mark();
            if !self.conforms(tpe, approx) {
                self.constraint.rollback(mark);
            }
        }
        self.define_call_types(call).map_err(|w| (span, w))?;
        let mut bodies = FxHashMap::default();
        self.collect_transparent_bodies(meth, false, &mut bodies).map_err(|w| (span, w))?;
        let with_inits = self.classes_with_inits();
        self.with_inlining(true, |i| i.inline_transparent((bodies, with_inits), call, ctx))
    }

    /// Whether `t` applies a type operation of `scala.compiletime.ops`.
    fn has_compiletime_op(&mut self, t: tasty::TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part) {
                found |= u.full_name(s).starts_with("scala.compiletime.ops.");
            }
            None
        });
        found
    }

    /// The classes of the unit that initialize something or are not completed yet.
    fn classes_with_inits(&self) -> FxHashSet<SymbolId> {
        let classes = self.syms.completions.keys().filter(|&&c| self.u.symbol(c).kind == Kind::Class);
        classes.filter(|c| self.syms.no_inits.get(c) != Some(&true)).copied().collect()
    }

    /// Adds to `bodies` the definitions of `meth` and of the transparent inline methods its body
    /// calls, of every inline method if `forced` or in the condition of an inline `if` or the
    /// selector of an inline match, typed ahead if they are not typed yet (dotty's
    /// `bodyToInline`).
    fn collect_transparent_bodies(
        &mut self,
        meth: SymbolId,
        forced: bool,
        bodies: &mut FxHashMap<SymbolId, TNodeId>,
    ) -> Result<(), &'static str> {
        if bodies.contains_key(&meth) {
            return Ok(());
        }
        let def = match self.inline_defs.get(&meth).copied() {
            Some(InlineDef::Typed(def) | InlineDef::Defined(def)) => def,
            Some(InlineDef::Pending(tree, ctx)) => {
                self.inline_defs.insert(meth, InlineDef::Typing);
                let def = self.typed_def_def(tree, ctx);
                self.typed_ahead(tree, def, ctx);
                def
            }
            Some(InlineDef::Typing) => return Err("transparent inline method called while typed"),
            None if self.u.symbol(meth).tree.is_some() => {
                crate::unpickle::inline_def(self.u, &mut self.tpd, self.names, meth)?
            }
            None => return Err("transparent inline method call"),
        };
        if !matches!(self.inline_defs.get(&meth), Some(InlineDef::Defined(_))) {
            self.define_tree(def, true)?;
            self.inline_defs.insert(meth, InlineDef::Defined(def));
        }
        bodies.insert(meth, def);
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else {
            return Err("transparent inline method without a body");
        };
        for (m, forced) in self.transparent_refs(rhs, forced) {
            self.collect_transparent_bodies(m, forced, bodies)?;
        }
        Ok(())
    }

    /// The inline methods `t` refers to that expand: transparent ones, every one if `forced` or
    /// in the condition of an inline `if` or the selector of an inline match; each with whether
    /// it is forced.
    fn transparent_refs(&self, t: TNodeId, forced: bool) -> Vec<(SymbolId, bool)> {
        let own = self.tpd.sym(t).filter(|&s| {
            let sym = self.u.symbol(s);
            let inline = sym.kind == Kind::Def && sym.flags.has(INLINE) && !sym.flags.has(MACRO);
            matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. })
                && (self.is_transparent_inline(s) || forced && inline)
        });
        let forcing = match *self.tpd.tree(t) {
            T::If { cond, inline: true, .. } => Some(cond),
            T::Match { selector, kind: MatchKind::Inline, .. } => selector,
            _ => None,
        };
        let inner = self
            .tpd
            .children(t)
            .into_iter()
            .flat_map(|c| self.transparent_refs(c, forced || Some(c) == forcing));
        own.map(|s| (s, forced)).into_iter().chain(inner).collect()
    }
}
